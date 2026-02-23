//! Image Processing module for OCR and image handling

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::io::Write;
use std::sync::Arc;
use teloxide::prelude::*;
use tempfile::NamedTempFile;
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
use crate::observability;

// Import error logging utilities
use crate::errors::error_logging;

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
            error_logging::log_filesystem_error(&e, "cleanup_temp_file", Some(&self.path), None);
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
            error_logging::log_network_error(&e, "download_image_file", None, None);
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

        // Send initial success message with cancel button and capture its ID
        let processing_keyboard = create_processing_keyboard(language_code, localization);
        let success_msg = bot.send_message(chat_id, success_message)
            .reply_markup(processing_keyboard)
            .await?;
        let success_message_id = success_msg.id;

        // Validate image format before OCR processing
        if !crate::ocr::is_supported_image_format(temp_file_guard.path(), &OCR_CONFIG) {
            warn!(user_id = %chat_id, "Unsupported image format rejected");
            bot.edit_message_text(chat_id, success_message_id, t_lang(localization, "error-unsupported-format", language_code))
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
            Ok((extracted_text, confidence)) => {
                // Log confidence information
                info!(
                    user_id = %chat_id,
                    confidence_score = confidence.overall_score,
                    flags = ?confidence.flags,
                    "OCR extraction completed with confidence score"
                );

                // Check if OCR result should be flagged for review
                if crate::ocr::should_flag_for_review(&confidence, 0.7) {
                    warn!(
                        user_id = %chat_id,
                        confidence_score = confidence.overall_score,
                        flags = ?confidence.flags,
                        "OCR result flagged for review: {}",
                        crate::ocr::get_confidence_description(&confidence)
                    );
                }

                if extracted_text.is_empty() {
                    warn!(user_id = %chat_id, "OCR extraction returned empty text");
                    bot.edit_message_text(chat_id, success_message_id, t_lang(localization, "error-no-text-found", language_code))
                        .await?;
                    Ok(String::new())
                } else {
                    info!(
                        user_id = %chat_id,
                        chars_extracted = extracted_text.len(),
                        "OCR extraction completed successfully"
                    );

                    // Process the extracted text to find ingredients with measurements and automated recovery
                    let ingredients = process_ingredients_with_recovery(
                        &extracted_text,
                        temp_file_guard.path(),
                        &OCR_CONFIG,
                        &OCR_INSTANCE_MANAGER,
                        &CIRCUIT_BREAKER,
                        language_code,
                    ).await;

                    if ingredients.is_empty() {
                        // No ingredients found, edit the success message
                        let no_ingredients_msg = format!(
                            "📝 {}\n\n{}\n\n```\n{}\n```",
                            t_lang(localization, "no-ingredients-found", language_code),
                            t_lang(localization, "no-ingredients-suggestion", language_code),
                            extracted_text
                        );
                        bot.edit_message_text(chat_id, success_message_id, &no_ingredients_msg).await?;
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

                        // Edit the success message with the ingredients review
                        let sent_message = bot.edit_message_text(chat_id, success_message_id, review_message)
                            .reply_markup(keyboard)
                            .await?;

                        // Determine recipe name: use caption if valid, otherwise "Recipe"
                        // PHOTO CAPTION FEATURE: Automatically uses photo captions as recipe name candidates
                        // This enhances UX by allowing users to name recipes directly when sending photos
                        let (recipe_name_candidate, recipe_name_from_caption) = match &caption {
                            Some(caption_text) if !caption_text.trim().is_empty() => {
                                // Validate the caption as a recipe name using existing validation logic
                                // This ensures captions meet the same standards as manually entered names
                                match crate::validation::validate_recipe_name(caption_text) {
                                    Ok(validated_name) => {
                                        info!(user_id = %chat_id, recipe_name = %validated_name, "Using caption as recipe name");
                                        (validated_name.to_string(), Some(caption_text.clone())) // Caption was successfully used
                                    }
                                    Err(_) => {
                                        // Caption is invalid (empty, too long, etc.), fall back to default
                                        // This provides graceful degradation and maintains functionality
                                        warn!(user_id = %chat_id, caption = %caption_text, "Caption is invalid, using default recipe name");
                                        let default_name = "Recipe";
                                        (default_name.to_string(), None) // Caption was not used
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
                error_logging::log_ocr_error(
                    &e,
                    "extract_text_from_image",
                    Some(chat_id.0),
                    None,
                    None,
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

                bot.edit_message_text(chat_id, success_message_id, &error_message).await?;
                Err(anyhow::anyhow!("OCR processing failed: {:?}", e))
            }
        }
    }
    .await;

    result
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
