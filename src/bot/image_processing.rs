//! Image processing: download, ingredient extraction and the review message

use crate::bot::html::{edit_html, Html};
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{debug, info, warn};

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use super::ui_builder::{create_ingredient_review_keyboard, render_review};

// Import HandlerContext
// use super::HandlerContext;

// Import observability
use crate::extraction::image::MAX_IMAGE_BYTES;
use crate::extraction::mapping::to_measurement_matches;
use crate::extraction::{ExtractionError, ImageInput, SharedExtractor};
use crate::observability;

// Import error logging utilities
use crate::errors::error_logging;

#[derive(Debug)]
pub struct ImageProcessingParams<'a> {
    pub file_id: teloxide::types::FileId,
    pub chat_id: ChatId,
    pub success_message: &'a str,
    pub language_code: Option<&'a str>,
    pub dialogue: RecipeDialogue,
    pub pool: Arc<PgPool>,
    pub caption: Option<String>,
    /// Engine that reads the ingredients (Gemini, behind the extraction guard).
    pub extractor: SharedExtractor,
    /// The user's photo or image document, deleted when the review ends.
    pub photo_message_id: teloxide::types::MessageId,
}

/// Downloads a Telegram file into memory, rejecting files larger than the extraction limit
/// before and during the download.
pub async fn download_file_bytes(bot: &Bot, file_id: teloxide::types::FileId) -> Result<Vec<u8>> {
    let max_file_size = MAX_IMAGE_BYTES;
    let file = bot.get_file(file_id).await?;
    ensure_file_size_allowed(u64::from(file.size), max_file_size)?;

    // The URL contains the bot token: it must never appear in errors or logs.
    let url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file.path
    );
    // The bot's client has the request timeout configured at startup.
    fetch_file_bytes(bot.client(), &url, max_file_size).await
}

/// Rejects files larger than `max_file_size` bytes.
fn ensure_file_size_allowed(size: u64, max_file_size: u64) -> Result<()> {
    if size > max_file_size {
        return Err(ExtractionError::ImageTooLarge {
            size,
            max: max_file_size,
        }
        .into());
    }
    Ok(())
}

/// Downloads `url` with `client`, rejecting HTTP errors and bodies larger than `max_file_size`.
///
/// Errors never contain the URL, which embeds the bot token for Telegram file downloads.
async fn fetch_file_bytes(
    client: &reqwest::Client,
    url: &str,
    max_file_size: u64,
) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(reqwest::Error::without_url)?;

    if let Some(content_length) = response.content_length() {
        ensure_file_size_allowed(content_length, max_file_size)?;
    }

    let bytes = response
        .bytes()
        .await
        .map_err(reqwest::Error::without_url)?;
    ensure_file_size_allowed(bytes.len() as u64, max_file_size)?;
    Ok(bytes.into())
}

/// A photo that gave no review: the photo and the bot's answer stay at the bottom, the answer
/// carrying "📚 Liste des recettes", until the next action ends (U6.3).
async fn end_failed_photo(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    keep: &[teloxide::types::MessageId],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) {
    crate::bot::home::send_home_keeping(bot, pool, chat_id, keep, localization, language_code)
        .await;
}

pub async fn download_and_process_image(
    bot: &Bot,
    params: ImageProcessingParams<'_>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<String> {
    let ImageProcessingParams {
        file_id,
        chat_id,
        success_message,
        language_code,
        dialogue,
        pool,
        caption,
        extractor,
        photo_message_id,
    } = params;

    // A photo starts a new creation: a running action ends (its messages go), and the list or
    // the active card leaves the bottom (U6.3)
    if dialogue.get().await?.is_some_and(|state| state.is_action()) {
        dialogue.exit().await?;
        crate::bot::cleanup::delete_action_messages(bot, chat_id).await;
    }
    crate::bot::cleanup::delete_lists(bot, chat_id).await;

    let bytes = match download_file_bytes(bot, file_id).await {
        Ok(bytes) => {
            debug!(user_id = %chat_id, bytes = bytes.len(), "Image downloaded successfully");
            bytes
        }
        Err(e) => {
            // Files over the limit are not a download failure: say why they are refused.
            let message_key = match e.downcast_ref::<ExtractionError>() {
                Some(too_large @ ExtractionError::ImageTooLarge { .. }) => {
                    extraction_error_key(too_large)
                }
                _ => {
                    error_logging::log_network_error(&e, "download_image_file", None, None);
                    "error-download-failed"
                }
            };
            let error_message = bot
                .send_message(chat_id, t_lang(localization, message_key, language_code))
                .await?;
            let keep = [photo_message_id, error_message.id];
            end_failed_photo(bot, &pool, chat_id, &keep, language_code, localization).await;
            return Err(e);
        }
    };

    // Processing message, replaced below by the result. It has no Cancel button: the chat's
    // next update waits for this one, so a press would only act once the review is shown.
    let processing_message = bot.send_message(chat_id, success_message).await?;
    let message_id = processing_message.id;

    let extraction = match ImageInput::prepare(bytes).await {
        Ok(image) => extractor.extract(&image.requested_by(chat_id.0)).await,
        Err(error) => Err(error),
    };
    let extraction = match extraction {
        Ok(extraction) => extraction,
        Err(error) => {
            observability::record_error_metrics(error.kind(), extractor.name());
            if is_operator_error(&error) {
                tracing::error!(user_id = %chat_id, backend = extractor.name(), error = %error, "Ingredient extraction misconfigured");
            } else {
                warn!(user_id = %chat_id, backend = extractor.name(), error = %error, "Ingredient extraction failed");
            }
            bot.edit_message_text(
                chat_id,
                message_id,
                t_lang(localization, extraction_error_key(&error), language_code),
            )
            .await?;
            let keep = [photo_message_id, message_id];
            end_failed_photo(bot, &pool, chat_id, &keep, language_code, localization).await;
            return Err(anyhow::anyhow!("ingredient extraction failed: {error}"));
        }
    };

    if !extraction.is_recipe {
        info!(user_id = %chat_id, backend = extractor.name(), "Image shows no ingredient list");
        bot.edit_message_text(
            chat_id,
            message_id,
            t_lang(localization, "extraction-not-a-recipe", language_code),
        )
        .await?;
        let keep = [photo_message_id, message_id];
        end_failed_photo(bot, &pool, chat_id, &keep, language_code, localization).await;
        return Ok(String::new());
    }

    let ingredients = to_measurement_matches(&extraction);
    let extracted_text = extraction.transcription;
    let title = extraction.title;

    if ingredients.is_empty() {
        // The transcription is shown as a preformatted block
        let mut no_ingredients_msg = Html::text("📝 ")
            + Html::bold(&t_lang(localization, "no-ingredients-found", language_code))
            + Html::text("\n\n")
            + Html::text(&t_lang(
                localization,
                "no-ingredients-suggestion",
                language_code,
            ));
        if !extracted_text.trim().is_empty() {
            no_ingredients_msg += Html::text("\n\n")
                + Html::pre(&crate::bot::ui_builder::fit_lines(
                    &extracted_text,
                    crate::bot::ui_builder::MAX_LIST_CHARS,
                ));
        }
        edit_html(bot, chat_id, message_id, no_ingredients_msg).await?;
        let keep = [photo_message_id, message_id];
        end_failed_photo(bot, &pool, chat_id, &keep, language_code, localization).await;
        return Ok(extracted_text);
    }

    // Ingredients found, go directly to review interface
    info!(user_id = %chat_id, backend = extractor.name(), ingredients_count = ingredients.len(), "Sending ingredients review interface");
    let review_message = render_review(&ingredients, language_code, localization);
    let keyboard = create_ingredient_review_keyboard(&ingredients, language_code, localization);
    let sent_message = edit_html(bot, chat_id, message_id, review_message)
        .reply_markup(keyboard)
        .await?;

    // Recipe name: the caption, else the title read on the page; otherwise "Recipe" and the
    // name is asked after confirmation.
    let (recipe_name_candidate, recipe_name_from_caption) =
        match known_recipe_name(caption.as_deref(), title.as_deref()) {
            Some(name) => {
                debug!(user_id = %chat_id, "Recipe name from the caption or the page title");
                (name.clone(), Some(name))
            }
            None => {
                debug!(user_id = %chat_id, "No caption or title, the name will be asked");
                ("Recipe".to_string(), None)
            }
        };

    // Update dialogue state to review ingredients with caption-derived recipe name
    dialogue
        .update(RecipeDialogueState::ReviewIngredients {
            recipe_name: recipe_name_candidate,
            ingredients,
            language_code: language_code.map(|s| s.to_string()),
            message_id: Some(sent_message.id.0 as i32),
            extracted_text: extracted_text.clone(),
            recipe_name_from_caption,
        })
        .await?;

    // The photo and the list stay while the ingredients are reviewed, then go with the review's
    // messages, also when a command ends the review (a cancel keeps the list as its notice)
    crate::bot::cleanup::track(chat_id, photo_message_id).await;
    crate::bot::cleanup::track(chat_id, sent_message.id).await;

    debug!(user_id = %chat_id, "Ingredients review interface sent successfully");
    Ok(extracted_text)
}

/// Name a recipe gets without asking: the caption (without links) wins over the title read on
/// the page; `None` when neither gives a valid name.
pub fn known_recipe_name(caption: Option<&str>, title: Option<&str>) -> Option<String> {
    caption.and_then(recipe_name_from_caption).or_else(|| {
        title
            .map(str::trim)
            .and_then(|title| crate::validation::validate_recipe_name(title).ok())
            .map(str::to_string)
    })
}

/// Recipe name suggested by a photo caption. Links are removed: sharing a web page to Telegram
/// puts its URL in the caption, and a URL is not a recipe name. `None` when nothing valid is left.
pub fn recipe_name_from_caption(caption: &str) -> Option<String> {
    let is_link = |word: &&str| {
        let word = word.to_lowercase();
        word.starts_with("http://") || word.starts_with("https://") || word.starts_with("www.")
    };
    let without_links = caption
        .split_whitespace()
        .filter(|word| !is_link(word))
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = without_links.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '-' | '–' | '—' | '|' | ':' | '·' | ',')
    });
    crate::validation::validate_recipe_name(cleaned)
        .ok()
        .map(str::to_string)
}

/// Locale key of the message shown to the user for an extraction failure.
pub fn extraction_error_key(error: &ExtractionError) -> &'static str {
    match error {
        ExtractionError::UnsupportedImage(_) => "error-extraction-unsupported-image",
        ExtractionError::ImageTooLarge { .. } => "error-extraction-image-too-large",
        ExtractionError::RateLimited { .. } => "error-extraction-busy",
        ExtractionError::Blocked(_)
        | ExtractionError::Recitation
        | ExtractionError::Truncated
        | ExtractionError::InvalidResponse(_) => "error-extraction-unreadable",
        // A client error that is not ours to fix (e.g. a corrupt image refused by the API)
        ExtractionError::Api { status, .. }
            if *status < 500 && *status != 429 && !is_operator_error(error) =>
        {
            "error-extraction-unreadable"
        }
        ExtractionError::Config(_)
        | ExtractionError::Timeout
        | ExtractionError::Network(_)
        | ExtractionError::Api { .. }
        | ExtractionError::CircuitOpen => "error-extraction-unavailable",
        ExtractionError::DailyLimitReached { .. } => "error-extraction-daily-limit",
    }
}

/// Failures the operator must fix (configuration, invalid key, unknown model), logged as errors.
fn is_operator_error(error: &ExtractionError) -> bool {
    match error {
        ExtractionError::Config(_) => true,
        ExtractionError::Api {
            status: 401 | 403 | 404,
            ..
        } => true,
        // Google answers 400 for an invalid key; other 400s are about the request content.
        ExtractionError::Api {
            status: 400,
            message,
        } => message.contains("API key") || message.contains("API_KEY"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caption_wins_over_the_title_then_the_name_is_asked() {
        assert_eq!(
            known_recipe_name(Some("Tarte de Mamie"), Some("Tarte aux pommes")).as_deref(),
            Some("Tarte de Mamie")
        );
        assert_eq!(
            known_recipe_name(None, Some("  Tarte aux pommes ")).as_deref(),
            Some("Tarte aux pommes")
        );
        // A caption made only of a link gives no name: the title is used
        assert_eq!(
            known_recipe_name(Some("https://www.marmiton.org/x.aspx"), Some("Crêpes")).as_deref(),
            Some("Crêpes")
        );
        assert_eq!(known_recipe_name(None, Some("   ")), None);
        assert_eq!(known_recipe_name(None, Some(&"x".repeat(300))), None);
        assert_eq!(known_recipe_name(None, None), None);
    }

    #[test]
    fn captions_lose_their_links() {
        assert_eq!(
            recipe_name_from_caption("Gâteau au chocolat https://www.marmiton.org/recettes/x.aspx")
                .as_deref(),
            Some("Gâteau au chocolat")
        );
        assert_eq!(
            recipe_name_from_caption("https://www.marmiton.org/x.aspx - Tarte aux pommes")
                .as_deref(),
            Some("Tarte aux pommes")
        );
        assert_eq!(
            recipe_name_from_caption("Crêpes").as_deref(),
            Some("Crêpes")
        );
    }

    #[test]
    fn a_caption_that_is_only_a_link_gives_no_name() {
        assert_eq!(
            recipe_name_from_caption("https://www.marmiton.org/recettes/x.aspx"),
            None
        );
        assert_eq!(recipe_name_from_caption("www.example.com"), None);
        assert_eq!(recipe_name_from_caption("   "), None);
    }

    #[test]
    fn every_extraction_error_has_a_message_in_both_languages() {
        let errors = [
            ExtractionError::Config(String::new()),
            ExtractionError::UnsupportedImage(String::new()),
            ExtractionError::ImageTooLarge { size: 1, max: 0 },
            ExtractionError::RateLimited { retry_after: None },
            ExtractionError::Timeout,
            ExtractionError::Network(String::new()),
            ExtractionError::Api {
                status: 500,
                message: String::new(),
            },
            ExtractionError::Blocked(String::new()),
            ExtractionError::Recitation,
            ExtractionError::Truncated,
            ExtractionError::InvalidResponse(String::new()),
            ExtractionError::CircuitOpen,
            ExtractionError::DailyLimitReached { limit: 1 },
        ];
        let locales = [
            include_str!("../../locales/en/main.ftl"),
            include_str!("../../locales/fr/main.ftl"),
        ];
        let mut keys: Vec<&str> = errors.iter().map(extraction_error_key).collect();
        keys.push("extraction-not-a-recipe");
        for key in keys {
            for locale in locales {
                assert!(
                    locale
                        .lines()
                        .any(|line| line.starts_with(&format!("{key} = "))),
                    "missing locale key {key}"
                );
            }
        }
    }

    #[test]
    fn only_configuration_problems_are_operator_errors() {
        let api = |status: u16, message: &str| ExtractionError::Api {
            status,
            message: message.to_string(),
        };
        assert!(is_operator_error(&ExtractionError::Config(String::new())));
        assert!(is_operator_error(&api(
            400,
            "INVALID_ARGUMENT: API key not valid"
        )));
        assert!(is_operator_error(&api(
            404,
            "NOT_FOUND: models/x is not found"
        )));
        assert!(!is_operator_error(&api(
            400,
            "INVALID_ARGUMENT: Unable to process input image"
        )));
        assert!(!is_operator_error(&api(429, "")));
        assert!(!is_operator_error(&api(503, "")));
        assert!(!is_operator_error(&ExtractionError::Timeout));
    }

    #[test]
    fn a_refused_image_is_unreadable_but_a_bad_key_is_unavailable() {
        let api = |status: u16, message: &str| ExtractionError::Api {
            status,
            message: message.to_string(),
        };
        assert_eq!(
            extraction_error_key(&api(400, "Unable to process input image")),
            "error-extraction-unreadable"
        );
        assert_eq!(
            extraction_error_key(&api(400, "API key not valid")),
            "error-extraction-unavailable"
        );
        assert_eq!(
            extraction_error_key(&api(503, "")),
            "error-extraction-unavailable"
        );
    }

    #[test]
    fn file_size_limit_is_inclusive() {
        assert!(ensure_file_size_allowed(10, 10).is_ok());
        assert!(ensure_file_size_allowed(11, 10).is_err());
    }

    #[tokio::test]
    async fn download_errors_do_not_contain_the_url_or_token() {
        // Port 1 on localhost refuses connections immediately.
        let url = "http://127.0.0.1:1/file/bot123456:SECRET-TOKEN/photos/file_1.jpg";
        let result = fetch_file_bytes(&reqwest::Client::new(), url, 1024).await;
        let message = match result {
            Ok(_) => String::from("unexpected success"),
            Err(error) => format!("{error:#} {error:?}"),
        };
        assert!(!message.contains("SECRET-TOKEN"), "token leaked: {message}");
        assert!(
            !message.contains("127.0.0.1:1/file"),
            "url leaked: {message}"
        );
        assert_ne!(message, "unexpected success");
    }
}
