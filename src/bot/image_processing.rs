//! Image Processing module for OCR and image handling

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{debug, info, warn};

// Import localization
use crate::localization::t_lang;

// Import text processing
use crate::text_processing::{MeasurementDetector, MeasurementMatch};

// Import OCR types
use crate::circuit_breaker::CircuitBreaker;
use crate::instance_manager::OcrInstanceManager;
use crate::ocr::{
    extract_hocr_from_image, map_measurement_to_bbox, parse_hocr_to_lines, perform_constrained_ocr,
};
use crate::ocr_config::OcrConfig;
use crate::ocr_errors::OcrError;
use crate::preprocessing::{
    crop_measurement_region, preprocess_measurement_region, CroppedImageResult,
};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use super::ui_builder::{
    create_ingredient_review_keyboard, create_processing_keyboard, format_ingredients_list,
};

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
    /// Engine that reads the ingredients (Gemini or Tesseract, from `EXTRACTION_BACKEND`).
    pub extractor: SharedExtractor,
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
        pool: _pool,
        caption,
        extractor,
    } = params;

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
            bot.send_message(chat_id, t_lang(localization, message_key, language_code))
                .await?;
            return Err(e);
        }
    };

    // Processing message with a cancel button, replaced below by the result
    let processing_keyboard = create_processing_keyboard(language_code, localization);
    let processing_message = bot
        .send_message(chat_id, success_message)
        .reply_markup(processing_keyboard)
        .await?;
    let message_id = processing_message.id;

    let extraction = match ImageInput::prepare(bytes).await {
        Ok(image) => extractor.extract(&image).await,
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
        return Ok(String::new());
    }

    let ingredients = to_measurement_matches(&extraction);
    let extracted_text = extraction.transcription;

    if ingredients.is_empty() {
        let mut no_ingredients_msg = format!(
            "📝 {}\n\n{}",
            t_lang(localization, "no-ingredients-found", language_code),
            t_lang(localization, "no-ingredients-suggestion", language_code),
        );
        if !extracted_text.trim().is_empty() {
            no_ingredients_msg.push_str(&format!("\n\n```\n{extracted_text}\n```"));
        }
        bot.edit_message_text(chat_id, message_id, &no_ingredients_msg)
            .await?;
        return Ok(extracted_text);
    }

    // Ingredients found, go directly to review interface
    info!(user_id = %chat_id, backend = extractor.name(), ingredients_count = ingredients.len(), "Sending ingredients review interface");
    let review_message = format!(
        "📝 **{}**\n\n{}\n\n{}",
        t_lang(localization, "review-title", language_code),
        t_lang(localization, "review-description", language_code),
        format_ingredients_list(&ingredients, language_code, localization)
    );
    let keyboard = create_ingredient_review_keyboard(&ingredients, language_code, localization);
    let sent_message = bot
        .edit_message_text(chat_id, message_id, review_message)
        .reply_markup(keyboard)
        .await?;

    // Determine recipe name: use the caption if it gives one, otherwise "Recipe" (and the name
    // is asked after confirmation). PHOTO CAPTION FEATURE: captions name recipes directly.
    let (recipe_name_candidate, recipe_name_from_caption) =
        match caption.as_deref().and_then(recipe_name_from_caption) {
            Some(name) => {
                info!(user_id = %chat_id, recipe_name = %name, "Using caption as recipe name");
                (name.clone(), Some(name))
            }
            None => {
                debug!(user_id = %chat_id, "No usable caption, using default recipe name");
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

    info!(user_id = %chat_id, "Ingredients review interface sent successfully");
    Ok(extracted_text)
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

/// Attempts automated recovery of anomalous quantity measurements using targeted re-OCR
///
/// This function implements the complete automated recovery pipeline:
/// 1. Extract HOCR from the original image to get spatial data
/// 2. Map the anomalous measurement to its bounding box
/// 3. Crop the image around the measurement region
/// 4. Apply targeted preprocessing for quantity recognition
/// 5. Run constrained OCR optimized for numbers and fractions
/// 6. Update the measurement match if recovery succeeds
async fn attempt_automated_quantity_recovery(
    image_path: &str,
    measurement_match: &mut MeasurementMatch,
    ocr_config: &OcrConfig,
    instance_manager: &OcrInstanceManager,
    circuit_breaker: &CircuitBreaker,
) -> Result<bool, OcrError> {
    info!(
        line_number = measurement_match.line_number,
        ingredient = %measurement_match.ingredient_name,
        "Attempting automated quantity recovery for anomalous measurement"
    );

    // Step 1: Extract HOCR from the original image
    let hocr_text =
        extract_hocr_from_image(image_path, ocr_config, instance_manager, circuit_breaker).await?;

    // Step 2: Parse HOCR to get line bounding boxes
    let hocr_lines = parse_hocr_to_lines(&hocr_text)?;

    // Step 3: Map the measurement to its bounding box
    let bbox = map_measurement_to_bbox(measurement_match, &hocr_lines).ok_or_else(|| {
        OcrError::Extraction(format!(
            "Could not map measurement to bounding box for line {}",
            measurement_match.line_number
        ))
    })?;

    // Step 4: Load the original image for cropping
    let _original_image = image::open(image_path)
        .map_err(|e| OcrError::Validation(format!("Failed to load image for cropping: {}", e)))?;

    // Step 5: Crop the measurement region
    let CroppedImageResult {
        image: cropped_image,
        ..
    } = crop_measurement_region(image_path, &bbox)
        .map_err(|e| OcrError::Extraction(format!("Failed to crop measurement region: {}", e)))?;

    // Step 6: Apply targeted preprocessing
    let preprocessed_image = preprocess_measurement_region(&cropped_image).map_err(|e| {
        OcrError::Extraction(format!("Failed to preprocess measurement region: {}", e))
    })?;

    // Step 7: Run constrained OCR
    let constrained_result =
        perform_constrained_ocr(&preprocessed_image.image, instance_manager, ocr_config).await?;

    // Step 8: Validate and update the measurement if recovery succeeded
    if is_valid_recovered_quantity(&constrained_result.text) {
        info!(
            original_quantity = %measurement_match.quantity,
            recovered_quantity = %constrained_result.text,
            confidence = constrained_result.confidence,
            "Automated quantity recovery succeeded"
        );

        // Update the measurement match with recovered quantity
        measurement_match.quantity = constrained_result.text.clone();
        measurement_match.requires_quantity_confirmation = false;

        Ok(true) // Recovery succeeded
    } else {
        debug!(
            recovered_text = %constrained_result.text,
            confidence = constrained_result.confidence,
            "Automated quantity recovery produced invalid result, keeping original anomaly"
        );
        Ok(false) // Recovery failed, keep original anomaly
    }
}

/// Validates if a recovered quantity string is acceptable for automated use
pub fn is_valid_recovered_quantity(text: &str) -> bool {
    let text = text.trim();

    // Must not be empty
    if text.is_empty() {
        return false;
    }

    // Must contain at least one digit
    if !text.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }

    // Must not contain letters (except in valid contexts like "1st", "2nd")
    let has_letters = text.chars().any(|c| c.is_ascii_alphabetic());
    if has_letters {
        // Allow ordinal numbers like "1st", "2nd", "3rd" but not other letters
        let valid_ordinals = ["1st", "2nd", "3rd"];
        if !valid_ordinals.contains(&text) {
            return false;
        }
    }

    // Must be a reasonable length (1-10 characters for quantities)
    if text.len() > 10 {
        return false;
    }

    // Must be parseable as a valid number or fraction, or be a valid ordinal
    let is_valid_ordinal = ["1st", "2nd", "3rd"].contains(&text);
    if text.parse::<f64>().is_err() && !is_valid_fraction(text) && !is_valid_ordinal {
        return false;
    }

    true
}

/// Checks if a string represents a valid fraction (like "1/2", "3/4", etc.)
pub fn is_valid_fraction(text: &str) -> bool {
    if let Some(slash_pos) = text.find('/') {
        let numerator = &text[..slash_pos];
        let denominator = &text[slash_pos + 1..];

        // Both parts must be numeric and denominator must not be zero
        numerator.parse::<u32>().is_ok() && denominator.parse::<u32>().is_ok() && denominator != "0"
    } else {
        false
    }
}

/// Process extracted text and return measurement matches with automated quantity recovery
pub async fn process_ingredients_with_recovery(
    extracted_text: &str,
    image_path: &str,
    ocr_config: &OcrConfig,
    instance_manager: &OcrInstanceManager,
    circuit_breaker: &CircuitBreaker,
    _language_code: Option<&str>,
) -> Vec<MeasurementMatch> {
    debug!(
        text_length = extracted_text.len(),
        "Processing extracted text for ingredients with automated recovery"
    );

    // Create measurement detector with default configuration
    let detector = match MeasurementDetector::new() {
        Ok(detector) => detector,
        Err(e) => {
            error_logging::log_internal_error(
                &e,
                "MeasurementDetector",
                "create_measurement_detector",
                None,
            );
            return Vec::new();
        }
    };

    // Find all measurements in the text
    let mut matches = detector.extract_ingredient_measurements(extracted_text);
    info!(
        matches_found = matches.len(),
        "Initial measurement detection completed"
    );

    // Count anomalies before recovery
    let anomalies_before = matches
        .iter()
        .filter(|m| m.requires_quantity_confirmation)
        .count();

    if anomalies_before > 0 {
        info!(
            anomalies = anomalies_before,
            "Detected anomalous measurements, attempting automated recovery"
        );

        // Attempt automated recovery for each anomalous measurement
        for measurement_match in &mut matches {
            if measurement_match.requires_quantity_confirmation {
                match attempt_automated_quantity_recovery(
                    image_path,
                    measurement_match,
                    ocr_config,
                    instance_manager,
                    circuit_breaker,
                )
                .await
                {
                    Ok(recovered) => {
                        if recovered {
                            info!(
                                ingredient = %measurement_match.ingredient_name,
                                "Successfully recovered quantity automatically"
                            );
                        } else {
                            debug!(
                                ingredient = %measurement_match.ingredient_name,
                                "Automated recovery failed, keeping anomaly for manual confirmation"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            ingredient = %measurement_match.ingredient_name,
                            error = %e,
                            "Automated recovery encountered error, keeping anomaly for manual confirmation"
                        );
                    }
                }
            }
        }

        // Count anomalies after recovery
        let anomalies_after = matches
            .iter()
            .filter(|m| m.requires_quantity_confirmation)
            .count();
        let recovered_count = anomalies_before - anomalies_after;

        info!(
            recovered = recovered_count,
            remaining_anomalies = anomalies_after,
            "Automated quantity recovery completed"
        );
    } else {
        debug!("No anomalous measurements detected, skipping automated recovery");
    }

    matches
}

/// Process extracted text and return measurement matches
pub fn process_ingredients_and_extract_matches(
    extracted_text: &str,
    _language_code: Option<&str>,
) -> Vec<MeasurementMatch> {
    debug!(
        text_length = extracted_text.len(),
        "Processing extracted text for ingredients"
    );

    // Create measurement detector with default configuration
    let detector = match MeasurementDetector::new() {
        Ok(detector) => detector,
        Err(e) => {
            error_logging::log_internal_error(
                &e,
                "MeasurementDetector",
                "create_measurement_detector",
                None,
            );
            return Vec::new();
        }
    };

    // Find all measurements in the text
    let matches = detector.extract_ingredient_measurements(extracted_text);
    info!(
        matches_found = matches.len(),
        "Measurement detection completed"
    );

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

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
