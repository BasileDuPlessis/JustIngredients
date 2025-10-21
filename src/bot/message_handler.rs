//! Message Handler module for processing incoming Telegram messages

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::io::Write;
use std::sync::Arc;
use teloxide::prelude::*;
use tempfile::NamedTempFile;
use tracing::{debug, error, info, warn};

// Import localization
use crate::localization::t_lang;

// Import text processing
use crate::text_processing::{MeasurementDetector, MeasurementMatch};

// Import OCR types
use crate::circuit_breaker::CircuitBreaker;
use crate::instance_manager::OcrInstanceManager;
use crate::ocr_config::OcrConfig;
use crate::ocr_errors::OcrError;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import database functions
use crate::db::get_user_recipes_paginated;

// Import UI builder functions
use super::ui_builder::{
    create_ingredient_review_keyboard, create_recipes_pagination_keyboard, format_ingredients_list,
};

// Import dialogue manager functions
use super::dialogue_manager::{
    handle_add_ingredient_input, handle_ingredient_edit_input, handle_ingredient_review_input,
    handle_recipe_name_after_confirm_input, handle_recipe_name_input, handle_recipe_rename_input,
    handle_saved_ingredient_edit_input, AddIngredientInputParams, DialogueContext,
    IngredientEditInputParams, IngredientReviewInputParams, RecipeNameAfterConfirmInputParams,
    RecipeNameInputParams, RecipeRenameInputParams, SavedIngredientEditInputParams,
};

// Import observability
use crate::observability;

/// RAII guard for temporary files that ensures cleanup on drop
pub struct TempFileGuard {
    path: String,
}

impl TempFileGuard {
    fn new(path: String) -> Self {
        Self { path }
    }

    fn path(&self) -> &str {
        &self.path
    }
}

impl std::fmt::Display for TempFileGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl AsRef<std::path::Path> for TempFileGuard {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.path)
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            error!(path = %self.path, error = %e, "Failed to clean up temporary file in drop");
        } else {
            debug!(path = %self.path, "Temporary file cleaned up successfully in drop");
        }
    }
}

/// Parameters for image processing
#[derive(Debug)]
pub struct ImageProcessingParams<'a> {
    pub file_id: teloxide::types::FileId,
    pub chat_id: ChatId,
    pub success_message: &'a str,
    pub language_code: Option<&'a str>,
    pub dialogue: RecipeDialogue,
    pub pool: Arc<PgPool>,
    pub caption: Option<String>,
}

// Create OCR configuration with default settings
static OCR_CONFIG: std::sync::LazyLock<OcrConfig> = std::sync::LazyLock::new(OcrConfig::default);
static OCR_INSTANCE_MANAGER: std::sync::LazyLock<OcrInstanceManager> =
    std::sync::LazyLock::new(OcrInstanceManager::default);
static CIRCUIT_BREAKER: std::sync::LazyLock<CircuitBreaker> =
    std::sync::LazyLock::new(|| CircuitBreaker::new(OCR_CONFIG.recovery.clone()));

pub async fn download_file(bot: &Bot, file_id: teloxide::types::FileId) -> Result<TempFileGuard> {
    let file = bot.get_file(file_id).await?;
    let file_path = file.path;
    let url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file_path
    );

    let response = reqwest::get(&url).await?;

    // Check Content-Length header to prevent downloading oversized files
    if let Some(content_length) = response.content_length() {
        let max_file_size = OCR_CONFIG.max_file_size;
        if content_length > max_file_size {
            return Err(anyhow::anyhow!(
                "File too large: {} bytes (maximum allowed: {} bytes)",
                content_length,
                max_file_size
            ));
        }
    }

    let bytes = response.bytes().await?;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.as_file_mut().write_all(&bytes)?;
    let path = temp_file.path().to_string_lossy().to_string();

    // Create a guard that will clean up the file when dropped
    // The NamedTempFile is forgotten here, but our guard will handle cleanup
    std::mem::forget(temp_file);
    Ok(TempFileGuard::new(path))
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
    } = params;
    let temp_file_guard = match download_file(bot, file_id).await {
        Ok(guard) => {
            debug!(user_id = %chat_id, temp_path = %guard, "Image downloaded successfully");
            guard
        }
        Err(e) => {
            error!(user_id = %chat_id, error = %e, "Failed to download image for user");
            bot.send_message(
                chat_id,
                t_lang(localization, "error-download-failed", language_code),
            )
            .await?;
            return Err(e);
        }
    }; // The guard will be moved into the async block below
    let result = async {
        info!("Image downloaded to: {}", temp_file_guard);

        // Send initial success message
        bot.send_message(chat_id, success_message).await?;

        // Validate image format before OCR processing
        if !crate::ocr::is_supported_image_format(temp_file_guard.path(), &OCR_CONFIG) {
            warn!(user_id = %chat_id, "Unsupported image format rejected");
            bot.send_message(chat_id, t_lang(localization, "error-unsupported-format", language_code))
                .await?;
            return Ok(String::new());
        }

        // Extract text from the image using OCR with circuit breaker protection
        match crate::ocr::extract_text_from_image(
            temp_file_guard.path(),
            &OCR_CONFIG,
            &OCR_INSTANCE_MANAGER,
            &CIRCUIT_BREAKER,
        )
        .await
        {
            Ok(extracted_text) => {
                if extracted_text.is_empty() {
                    warn!(user_id = %chat_id, "OCR extraction returned empty text");
                    bot.send_message(chat_id, t_lang(localization, "error-no-text-found", language_code))
                        .await?;
                    Ok(String::new())
                } else {
                    info!(
                        user_id = %chat_id,
                        chars_extracted = extracted_text.len(),
                        "OCR extraction completed successfully"
                    );

                    // Process the extracted text to find ingredients with measurements
                    let ingredients =
                        process_ingredients_and_extract_matches(&extracted_text, language_code);

                    if ingredients.is_empty() {
                        // No ingredients found, send message directly without dialogue
                        let no_ingredients_msg = format!(
                            "📝 {}\n\n{}\n\n```\n{}\n```",
                            t_lang(localization, "no-ingredients-found", language_code),
                            t_lang(localization, "no-ingredients-suggestion", language_code),
                            extracted_text
                        );
                        bot.send_message(chat_id, &no_ingredients_msg).await?;
                    } else {
                        // Ingredients found, go directly to review interface
                        info!(user_id = %chat_id, ingredients_count = ingredients.len(), "Sending ingredients review interface");
                        let review_message = format!(
                            "📝 **{}**\n\n{}\n\n{}",
                            t_lang(localization, "review-title", language_code),
                            t_lang(localization, "review-description", language_code),
                            format_ingredients_list(&ingredients, language_code, localization)
                        );

                                                let keyboard = create_ingredient_review_keyboard(&ingredients, language_code, localization);

                        let sent_message = bot.send_message(chat_id, review_message)
                            .reply_markup(keyboard)
                            .await?;

                        // Determine recipe name: use caption if valid, otherwise "Recipe"
                        // PHOTO CAPTION FEATURE: Automatically uses photo captions as recipe name candidates
                        // This enhances UX by allowing users to name recipes directly when sending photos
                        let (recipe_name_candidate, recipe_name_from_caption) = match &caption {
                            Some(caption_text) if !caption_text.trim().is_empty() => {
                                // Validate the caption as a recipe name using existing validation logic
                                // This ensures captions meet the same standards as manually entered names
                                match crate::dialogue::validate_recipe_name(caption_text) {
                                    Ok(validated_name) => {
                                        info!(user_id = %chat_id, recipe_name = %validated_name, "Using caption as recipe name");
                                        // Send feedback message about using caption
                                        let caption_msg = t_lang(localization, "caption-used", language_code)
                                            .replace("{$caption}", &validated_name);
                                        bot.send_message(chat_id, caption_msg).await?;
                                        (validated_name, Some(caption_text.clone())) // Caption was successfully used
                                    }
                                    Err(_) => {
                                        // Caption is invalid (empty, too long, etc.), fall back to default
                                        // This provides graceful degradation and maintains functionality
                                        warn!(user_id = %chat_id, caption = %caption_text, "Caption is invalid, using default recipe name");
                                        let default_name = "Recipe".to_string();
                                        // Send feedback message about invalid caption
                                        let invalid_caption_msg = t_lang(localization, "caption-invalid", language_code)
                                            .replace("{$caption}", caption_text)
                                            .replace("{$default_name}", &default_name);
                                        bot.send_message(chat_id, invalid_caption_msg).await?;
                                        (default_name, None) // Caption was not used
                                    }
                                }
                            }
                            _ => {
                                // No caption or empty caption, use default
                                // This maintains backward compatibility - existing users see no change
                                debug!(user_id = %chat_id, "No caption provided, using default recipe name");
                                ("Recipe".to_string(), None) // No caption available
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
                                recipe_name_from_caption, // Only set when caption was successfully validated and used
                            })
                            .await?;

                        info!(user_id = %chat_id, "Ingredients review interface sent successfully");
                    }

                    Ok(extracted_text)
                }
            }
            Err(e) => {
                error!(
                    user_id = %chat_id,
                    error = %e,
                    "OCR processing failed for user"
                );

                // Provide more specific error messages based on the error type
                let error_message = match &e {
                    OcrError::Validation(msg) => {
                        observability::record_error_metrics("validation", "ocr");
                        t_lang(localization, "error-validation", language_code).replace("{}", msg)
                    }
                    OcrError::ImageLoad(_) => {
                        observability::record_error_metrics("image_load", "ocr");
                        t_lang(localization, "error-image-load", language_code)
                    }
                    OcrError::Initialization(_) => {
                        observability::record_error_metrics("initialization", "ocr");
                        t_lang(localization, "error-ocr-initialization", language_code)
                    }
                    OcrError::Extraction(_) => {
                        observability::record_error_metrics("extraction", "ocr");
                        t_lang(localization, "error-ocr-extraction", language_code)
                    }
                    OcrError::Timeout(msg) => {
                        observability::record_error_metrics("timeout", "ocr");
                        t_lang(localization, "error-ocr-timeout", language_code).replace("{}", msg)
                    }
                    OcrError::_InstanceCorruption(_) => {
                        observability::record_error_metrics("instance_corruption", "ocr");
                        t_lang(localization, "error-ocr-corruption", language_code)
                    }
                    OcrError::_ResourceExhaustion(_) => {
                        observability::record_error_metrics("resource_exhaustion", "ocr");
                        t_lang(localization, "error-ocr-exhaustion", language_code)
                    }
                };

                bot.send_message(chat_id, &error_message).await?;
                Err(anyhow::anyhow!("OCR processing failed: {:?}", e))
            }
        }
    }
    .await;

    result
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
            error!(error = %e, "Failed to create measurement detector - ingredient extraction disabled");
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

async fn handle_text_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    if let Some(text) = msg.text() {
        debug!(user_id = %msg.chat.id, message_length = text.len(), "Received text message from user");

        // Extract user's language code from Telegram
        let language_code = msg
            .from
            .as_ref()
            .and_then(|user| user.language_code.as_ref())
            .map(|s| s.as_str());

        // Check dialogue state first
        let dialogue_state = dialogue.get().await?;
        match dialogue_state {
            Some(RecipeDialogueState::WaitingForRecipeName {
                extracted_text,
                ingredients,
                language_code: dialogue_lang_code,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe name input
                return handle_recipe_name_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeNameInputParams {
                        pool,
                        recipe_name_input: text,
                        extracted_text,
                        ingredients,
                        language_code: effective_language_code,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                ingredients,
                language_code: dialogue_lang_code,
                extracted_text,
                recipe_name_from_caption: _,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe name input after ingredient confirmation
                return handle_recipe_name_after_confirm_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeNameAfterConfirmInputParams {
                        pool,
                        recipe_name_input: text,
                        ingredients,
                        language_code: effective_language_code,
                        extracted_text,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: dialogue_lang_code,
                message_id: _,
                extracted_text,
                recipe_name_from_caption: _,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle ingredient review commands
                return handle_ingredient_review_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    IngredientReviewInputParams {
                        pool,
                        review_input: text,
                        recipe_name,
                        ingredients,
                        language_code: effective_language_code,
                        extracted_text,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingIngredient {
                recipe_name,
                ingredients,
                editing_index,
                language_code: dialogue_lang_code,
                message_id,
                extracted_text,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle ingredient edit input
                return handle_ingredient_edit_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    IngredientEditInputParams {
                        edit_input: text,
                        recipe_name,
                        ingredients,
                        editing_index,
                        language_code: effective_language_code,
                        message_id,
                        extracted_text,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::RenamingRecipe {
                recipe_id,
                current_name,
                language_code: dialogue_lang_code,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe rename input
                return handle_recipe_rename_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeRenameInputParams {
                        pool,
                        new_name_input: text,
                        recipe_id,
                        current_name,
                        language_code: effective_language_code,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::AddingIngredientToSavedRecipe {
                recipe_id,
                original_ingredients,
                current_matches,
                language_code: dialogue_lang_code,
                message_id,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle adding new ingredient input for saved recipes
                return handle_add_ingredient_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    AddIngredientInputParams {
                        pool: &pool,
                        add_input: text,
                        recipe_id,
                        original_ingredients: &original_ingredients,
                        current_matches: &current_matches,
                        language_code: effective_language_code,
                        message_id,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingSavedIngredient {
                recipe_id,
                original_ingredients,
                current_matches,
                editing_index,
                language_code: dialogue_lang_code,
                message_id,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle editing individual ingredient input for saved recipes
                return handle_saved_ingredient_edit_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    SavedIngredientEditInputParams {
                        pool: &pool,
                        edit_input: text,
                        recipe_id,
                        original_ingredients: &original_ingredients,
                        current_matches: &current_matches,
                        language_code: effective_language_code,
                        message_id,
                        editing_index,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
                // Users should use buttons in this state, not type text
                let effective_language_code = language_code; // No dialogue language code available
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "use-buttons-instruction", effective_language_code),
                )
                .await?;
                return Ok(());
            }
            Some(RecipeDialogueState::Start) | None => {
                // Continue with normal command handling
            }
        }

        // Handle /start command
        if text == "/start" {
            // Record user engagement metric for start command
            if let Some(user) = msg.from.as_ref() {
                crate::observability::record_user_engagement_metrics(
                    user.id.0 as i64,
                    crate::observability::UserAction::StartCommand,
                    None, // No session duration for individual actions
                    language_code,
                );
            }

            let welcome_message = format!(
                "👋 **{}**\n\n{}\n\n{}\n\n{}\n{}\n{}\n\n{}",
                t_lang(localization, "welcome-title", language_code),
                t_lang(localization, "welcome-description", language_code),
                t_lang(localization, "welcome-features", language_code),
                t_lang(localization, "welcome-commands", language_code),
                t_lang(localization, "welcome-start", language_code),
                t_lang(localization, "welcome-help", language_code),
                t_lang(localization, "welcome-send-image", language_code)
            );
            bot.send_message(msg.chat.id, welcome_message).await?;
        }
        // Handle /help command
        else if text == "/help" {
            // Record user engagement metric for help command
            if let Some(user) = msg.from.as_ref() {
                crate::observability::record_user_engagement_metrics(
                    user.id.0 as i64,
                    crate::observability::UserAction::HelpCommand,
                    None, // No session duration for individual actions
                    language_code,
                );
            }

            let help_message = vec![
                t_lang(localization, "help-title", language_code),
                t_lang(localization, "help-description", language_code),
                t_lang(localization, "help-step1", language_code),
                t_lang(localization, "help-step2", language_code),
                t_lang(localization, "help-step3", language_code),
                t_lang(localization, "help-step4", language_code),
                t_lang(localization, "help-formats", language_code),
                t_lang(localization, "help-commands", language_code),
                t_lang(localization, "help-start", language_code),
                t_lang(localization, "help-tips", language_code),
                t_lang(localization, "help-tip1", language_code),
                t_lang(localization, "help-tip2", language_code),
                t_lang(localization, "help-tip3", language_code),
                t_lang(localization, "help-tip4", language_code),
                t_lang(localization, "help-final", language_code),
            ]
            .join("\n\n");
            bot.send_message(msg.chat.id, help_message).await?;
        }
        // Handle /recipes command
        else if text == "/recipes" {
            // Record user engagement metric for recipes command
            if let Some(user) = msg.from.as_ref() {
                crate::observability::record_user_engagement_metrics(
                    user.id.0 as i64,
                    crate::observability::UserAction::RecipesCommand,
                    None, // No session duration for individual actions
                    language_code,
                );
            }

            handle_recipes_command(bot, msg, pool, language_code, localization).await?;
        }
        // Handle regular text messages
        else {
            bot.send_message(
                msg.chat.id,
                format!(
                    "{} {}",
                    t_lang(localization, "text-response", language_code),
                    t_lang(localization, "text-tip", language_code)
                ),
            )
            .await?;
        }
    }
    Ok(())
}

async fn handle_photo_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    debug!(user_id = %msg.chat.id, "Received photo message from user");

    // Record user engagement metric for photo upload
    if let Some(user) = msg.from.as_ref() {
        crate::observability::record_user_engagement_metrics(
            user.id.0 as i64,
            crate::observability::UserAction::PhotoUpload,
            None, // No session duration for individual actions
            language_code,
        );
    }

    if let Some(photos) = msg.photo() {
        if let Some(largest_photo) = photos.last() {
            // Extract caption if present - this will be used as recipe name candidate
            // PHOTO CAPTION FEATURE: Captions provide automatic recipe naming for better UX
            let caption = msg.caption().map(|s| s.to_string());

            let _temp_path = download_and_process_image(
                bot,
                ImageProcessingParams {
                    file_id: largest_photo.file.id.clone(),
                    chat_id: msg.chat.id,
                    success_message: &t_lang(localization, "processing-photo", language_code),
                    language_code,
                    dialogue,
                    pool,
                    caption,
                },
                localization,
            )
            .await;
        }
    }
    Ok(())
}

async fn handle_document_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    if let Some(doc) = msg.document() {
        if let Some(mime_type) = &doc.mime_type {
            if mime_type.to_string().starts_with("image/") {
                debug!(user_id = %msg.chat.id, mime_type = %mime_type, "Received image document from user");

                // Record user engagement metric for document upload
                if let Some(user) = msg.from.as_ref() {
                    crate::observability::record_user_engagement_metrics(
                        user.id.0 as i64,
                        crate::observability::UserAction::DocumentUpload,
                        None, // No session duration for individual actions
                        language_code,
                    );
                }

                let _temp_path = download_and_process_image(
                    bot,
                    ImageProcessingParams {
                        file_id: doc.file.id.clone(),
                        chat_id: msg.chat.id,
                        success_message: &t_lang(
                            localization,
                            "processing-document",
                            language_code,
                        ),
                        language_code,
                        dialogue,
                        pool,
                        caption: None, // Documents don't have captions like photos do
                    },
                    localization,
                )
                .await;
            } else {
                debug!(user_id = %msg.chat.id, mime_type = %mime_type, "Received non-image document from user");
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "error-unsupported-format", language_code),
                )
                .await?;
            }
        } else {
            debug!(user_id = %msg.chat.id, "Received document without mime type from user");
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "error-no-mime-type", language_code),
            )
            .await?;
        }
    }
    Ok(())
}

async fn handle_recipes_command(
    bot: &Bot,
    msg: &Message,
    pool: Arc<PgPool>,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(user_id = %msg.chat.id, "Handling /recipes command");

    // Get paginated recipes for the user
    let (recipes, total_count) = get_user_recipes_paginated(&pool, msg.chat.id.0, 5, 0).await?;

    if recipes.is_empty() {
        // No recipes found
        let no_recipes_message = format!(
            "📚 {}\n\n{}",
            t_lang(localization, "no-recipes-found", language_code),
            t_lang(localization, "no-recipes-suggestion", language_code)
        );
        bot.send_message(msg.chat.id, no_recipes_message).await?;
    } else {
        // Create the message text
        let recipes_message = format!(
            "📚 **{}**\n\n{}",
            t_lang(localization, "your-recipes", language_code),
            t_lang(localization, "select-recipe", language_code)
        );

        // Create the pagination keyboard
        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            0,
            total_count,
            5,
            language_code,
            localization,
        );

        bot.send_message(msg.chat.id, recipes_message)
            .reply_markup(keyboard)
            .await?;
    }

    Ok(())
}

async fn handle_unsupported_message(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    debug!(user_id = %msg.chat.id, "Received unsupported message type from user");

    let help_message = format!(
        "{}\n\n{}\n{}\n{}\n{}\n{}\n\n{}",
        t_lang(localization, "unsupported-title", language_code),
        t_lang(localization, "unsupported-description", language_code),
        t_lang(localization, "unsupported-feature1", language_code),
        t_lang(localization, "unsupported-feature2", language_code),
        t_lang(localization, "unsupported-feature3", language_code),
        t_lang(localization, "unsupported-feature4", language_code),
        t_lang(localization, "unsupported-final", language_code)
    );
    bot.send_message(msg.chat.id, help_message).await?;
    Ok(())
}

/// Main message handler for Telegram bot interactions
///
/// Implements comprehensive message routing and dialogue state management.
/// This function orchestrates the entire bot interaction flow, handling different
/// message types and managing conversation state across multiple dialogue phases.
///
/// ## Message Routing Algorithm
///
/// ```text
/// 1. Extract message metadata (type, language, user info)
/// 2. Record telemetry metrics for monitoring
/// 3. Route by message type:
///    ├── Text → handle_text_message()
///    ├── Photo → handle_photo_message()
///    ├── Document → handle_document_message()
///    └── Other → handle_unsupported_message()
/// 4. Handle dialogue state transitions
/// 5. Record performance metrics
/// 6. Return result with error handling
/// ```
///
/// ## Dialogue State Machine
///
/// The bot maintains complex conversation state using `RecipeDialogueState`:
///
/// ```text
/// Start ────photo received────► WaitingForRecipeName
///    │                              │
///    │                              │ user provides name
///    │                              ▼
///    └───────────────► ReviewIngredients ────user confirms───► WaitingForRecipeNameAfterConfirm
///                              │                                      │
///                              │ user edits                           │ user provides name
///                              ▼                                      ▼
///                       EditingIngredient ──► ReviewIngredients ──► [Recipe Saved]
/// ```
///
/// ## State-Specific Message Handling
///
/// ### Text Messages
/// - **Start State**: Handle `/start`, `/help`, `/recipes` commands
/// - **WaitingForRecipeName**: Process recipe name input with validation
/// - **ReviewIngredients**: Handle ingredient review commands (edit/delete/confirm)
/// - **EditingIngredient**: Process ingredient edit input
/// - **WaitingForRecipeNameAfterConfirm**: Handle post-confirmation recipe naming
///
/// ### Photo Messages
/// - Extract caption for automatic recipe naming
/// - Download and process image via OCR pipeline
/// - Transition to ingredient review interface
/// - Handle caption validation and fallback logic
///
/// ### Document Messages
/// - Validate image MIME types
/// - Process supported image formats
/// - Same OCR pipeline as photos (no caption support)
///
/// ## Language Detection & Localization
///
/// ```text
/// 1. Extract language_code from Telegram user.language_code
/// 2. Fallback to 'en' if not available
/// 3. Load appropriate Fluent bundle for localization
/// 4. Use localized messages throughout interaction
/// ```
///
/// ## Error Handling Strategy
///
/// - **Graceful Degradation**: Unsupported messages get helpful guidance
/// - **User-Friendly Messages**: Localized error responses
/// - **State Preservation**: Dialogue state maintained across errors
/// - **Logging**: Comprehensive error logging for debugging
///
/// ## Performance Monitoring
///
/// Tracks multiple metrics:
/// - Message type distribution
/// - Processing duration
/// - User language preferences
/// - Media attachment statistics
/// - Error rates by message type
///
/// ## Thread Safety
///
/// - Uses `Arc<PgPool>` for database connection sharing
/// - Dialogue state managed per chat_id
/// - Localization manager shared across requests
///
/// # Arguments
///
/// * `bot` - Telegram bot instance for sending responses
/// * `msg` - Incoming Telegram message to process
/// * `pool` - PostgreSQL connection pool for data persistence
/// * `dialogue` - Dialogue state manager for conversation flow
/// * `localization` - Localization manager for multi-language support
///
/// # Returns
///
/// Returns `Result<(), anyhow::Error>` indicating success or failure
///
/// # Message Type Support
///
/// - **Text**: Commands, dialogue input, recipe management
/// - **Photo**: Image processing with optional captions
/// - **Document**: Image files uploaded as documents
/// - **Unsupported**: Guidance for unsupported message types
pub async fn message_handler(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let span = crate::observability::telegram_span(
        "message_handler",
        msg.from.as_ref().map(|u| u.id.0 as i64),
    );
    let _enter = span.enter();

    let start_time = std::time::Instant::now();
    let message_type = if msg.text().is_some() {
        "text"
    } else if msg.photo().is_some() {
        "photo"
    } else if msg.document().is_some() {
        "document"
    } else {
        "unsupported"
    };

    observability::record_telegram_message(message_type);

    let result = if msg.text().is_some() {
        handle_text_message(&bot, &msg, dialogue, pool, &localization).await
    } else if msg.photo().is_some() {
        handle_photo_message(&bot, &msg, dialogue, pool, &localization).await
    } else if msg.document().is_some() {
        handle_document_message(&bot, &msg, dialogue, pool, &localization).await
    } else {
        handle_unsupported_message(&bot, &msg, &localization).await
    };

    let duration = start_time.elapsed();
    observability::record_request_metrics("telegram_message", 200, duration);

    // Record enhanced Telegram performance metrics
    let message_size =
        msg.text().map(|t| t.len()).unwrap_or(0) + msg.caption().map(|c| c.len()).unwrap_or(0);
    let has_media = msg.photo().is_some() || msg.document().is_some();
    observability::record_telegram_performance_metrics(
        message_type,
        duration,
        msg.from.as_ref().map(|u| u.id.0 as i64),
        message_size,
        has_media,
    );

    result
}

/// Cache-enabled message handler for improved performance
///
/// This version includes caching for database queries and OCR results
/// to reduce processing time and database load.
pub async fn message_handler_with_cache(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    _cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    // For now, delegate to the original handler
    // TODO: Integrate caching into specific operations
    message_handler(bot, msg, pool, dialogue, localization).await
}
