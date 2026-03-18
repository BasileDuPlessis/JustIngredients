//! # OCR Extraction Module
//!
//! This module handles the core OCR extraction logic, including timeout handling,
//! instance management, and text processing.

use anyhow::Result;
use tempfile::NamedTempFile;
use tracing::{info, warn};

use crate::error_correction::OcrErrorCorrector;
use crate::instance_manager::OcrInstanceManager;
use crate::observability::metrics::record_ocr_cleaning_metrics;
use crate::ocr::types::ConstrainedOcrResult;
use crate::ocr_config::OcrConfig;
use crate::ocr_errors::OcrError;

/// Clean OCR artifacts from extracted text
///
/// Removes leading dashes (-) and dots (.) from each line of OCR text.
/// This improves ingredient parsing accuracy by removing common
/// formatting artifacts from recipe images.
///
/// # Arguments
/// * `text` - Raw OCR extracted text
///
/// # Returns
/// * `(String, u32)` - Tuple of (cleaned text with artifacts removed, number of artifacts removed)
///
/// # Examples
/// ```
/// use just_ingredients::ocr::clean_ocr_artifacts;
/// let raw = "- 2 cups flour\n• 1 tsp salt";
/// let (clean, artifacts) = clean_ocr_artifacts(raw);
/// assert_eq!(clean, "2 cups flour\n1 tsp salt");
/// assert_eq!(artifacts, 2);
/// ```
pub fn clean_ocr_artifacts(text: &str) -> (String, u32) {
    let mut artifacts_removed = 0;

    let cleaned_lines: Vec<String> = text
        .lines()
        .map(|line| {
            let original_line = line;
            let mut cleaned = line.trim_start();

            // Count and remove leading dashes
            let dash_count = cleaned.chars().take_while(|&c| c == '-').count();
            cleaned = cleaned.trim_start_matches('-');
            artifacts_removed += dash_count as u32;

            // Count and remove leading dots (including bullet points •)
            let dot_count = cleaned
                .chars()
                .take_while(|&c| c == '.' || c == '•')
                .count();
            cleaned = cleaned.trim_start_matches('.');
            cleaned = cleaned.trim_start_matches('•');
            artifacts_removed += dot_count as u32;

            // Trim any remaining leading whitespace
            let final_cleaned = cleaned.trim_start();

            // Log what was cleaned for debugging
            if original_line != final_cleaned {
                tracing::trace!(
                    "Cleaned OCR artifact: '{}' -> '{}'",
                    original_line,
                    final_cleaned
                );
            }

            final_cleaned.to_string()
        })
        .collect();

    (cleaned_lines.join("\n"), artifacts_removed)
}

/// Helper function to perform OCR extraction with timeout
///
/// This function handles the core OCR processing using Tesseract, including:
/// - OCR instance acquisition from the manager
/// - Image loading and processing
/// - Text extraction and cleanup
/// - Timeout protection
/// - Performance timing and logging
///
/// # Arguments
///
/// * `image_path` - Path to the image file to process
/// * `config` - OCR configuration with timeout and language settings
/// * `instance_manager` - Manager for OCR instance reuse
///
/// # Returns
///
/// Returns `Result<String, OcrError>` with cleaned extracted text or error
///
/// # Processing Details
///
/// 1. Acquires or creates OCR instance for specified language
/// 2. Loads image into Tesseract engine
/// 3. Performs OCR text extraction
/// 4. Cleans extracted text (removes extra whitespace, empty lines)
/// 5. Logs performance metrics
///
/// # Performance
///
/// - Times only the actual OCR processing (excludes validation/retry logic)
/// - Logs processing time in milliseconds
/// - Includes character count in success logs
///
/// # Errors
///
/// - `InitializationError` - Failed to get/create OCR instance
/// - `ImageLoadError` - Could not load the image file
/// - `ExtractionError` - OCR processing failed
/// - `TimeoutError` - Operation exceeded timeout
pub async fn perform_ocr_extraction(
    image_path: &str,
    config: &crate::ocr_config::OcrConfig,
    instance_manager: &crate::instance_manager::OcrInstanceManager,
) -> Result<(String, f32, std::time::Duration), crate::ocr_errors::OcrError> {
    // Start timing the actual OCR processing
    let ocr_start_time = std::time::Instant::now();

    // Create a timeout for the operation
    let timeout_duration = tokio::time::Duration::from_secs(config.recovery.operation_timeout_secs);

    let result = tokio::time::timeout(timeout_duration, async {
        // Apply image preprocessing for OCR optimization
        let (_temp_file, processed_image_path, preprocessing_duration) =
            crate::ocr::apply_image_preprocessing(image_path, config).await?;

        info!(
            "Using preprocessed image for OCR: preprocessing took {:.2}ms",
            preprocessing_duration.as_millis()
        );

        // Get or create OCR instance from the manager
        let instance = instance_manager
            .get_instance(config)
            .map_err(|e| crate::ocr_errors::OcrError::Initialization(e.to_string()))?;

        // Perform OCR processing with the reused instance
        let (extracted_text, tesseract_confidence) = {
            let mut tess = instance
                .lock()
                .expect("Failed to acquire Tesseract instance lock");
            // Set the preprocessed image for OCR processing
            tess.set_image(&processed_image_path).map_err(|e| {
                crate::ocr_errors::OcrError::ImageLoad(format!(
                    "Failed to load preprocessed image for OCR: {e}"
                ))
            })?;

            // Extract text from the image
            let text = tess.get_utf8_text().map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Failed to extract text from preprocessed image: {e}"
                ))
            })?;

            // Extract confidence score from Tesseract
            let confidence = match tess.mean_text_conf() {
                conf if conf >= 0 => conf as f32,
                _ => 75.0, // Default confidence when Tesseract returns invalid confidence
            };

            (text, confidence)
        };

        // Note: The temporary file will be automatically cleaned up when _temp_file goes out of scope

        // Clean up the extracted text (remove extra whitespace and empty lines)
        let cleaned_text = extracted_text
            .trim()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join("\n");

        // Remove OCR artifacts (leading dashes and dots) with metrics
        let cleaning_start = std::time::Instant::now();
        let (artifact_cleaned_text, artifacts_removed) = clean_ocr_artifacts(&cleaned_text);
        let cleaning_duration = cleaning_start.elapsed();

        // Record cleaning metrics
        record_ocr_cleaning_metrics(
            artifacts_removed,
            cleaning_duration,
            cleaned_text.len(),
            artifact_cleaned_text.len(),
        );

        // Log cleaning effectiveness
        if artifacts_removed > 0 {
            info!(
                "OCR cleaning removed {} artifacts in {:.2}ms (text length: {} -> {})",
                artifacts_removed,
                cleaning_duration.as_millis(),
                cleaned_text.len(),
                artifact_cleaned_text.len()
            );
        } else {
            tracing::trace!(
                "OCR cleaning completed in {:.2}ms (no artifacts found)",
                cleaning_duration.as_millis()
            );
        }

        // Apply comprehensive OCR error correction
        let error_corrector = OcrErrorCorrector::new();
        let corrected_text = error_corrector.correct_text(&artifact_cleaned_text);

        Ok((corrected_text, tesseract_confidence))
    })
    .await;

    let ocr_duration = ocr_start_time.elapsed();
    let ocr_ms = ocr_duration.as_millis();

    match result {
        Ok(Ok((text, confidence))) => {
            info!(
                "OCR processing completed in {}ms, extracted {} characters (Tesseract confidence: {:.1}%)",
                ocr_ms,
                text.len(),
                confidence
            );
            Ok((text, confidence, ocr_duration))
        }
        Ok(Err(e)) => {
            warn!("OCR processing failed after {ocr_ms}ms: {e:?}");
            Err(e)
        }
        Err(_) => {
            warn!(
                "OCR processing timed out after {}ms (limit: {}s)",
                ocr_ms, config.recovery.operation_timeout_secs
            );
            Err(crate::ocr_errors::OcrError::Timeout(format!(
                "OCR operation timed out after {} seconds",
                config.recovery.operation_timeout_secs
            )))
        }
    }
}

/// Perform constrained OCR on a DynamicImage for specialized text recognition
///
/// This function is optimized for recognizing specific patterns like quantities and measurements
/// by using constrained OCR settings (PSM 8 - Single Word mode with character whitelist).
///
/// # Arguments
///
/// * `image` - The image to process
/// * `instance_manager` - Manager for OCR instance reuse
/// * `config` - OCR configuration with timeout settings
///
/// # Returns
///
/// Returns `ConstrainedOcrResult` with extracted text, confidence, and processing metadata
///
/// # Processing Details
///
/// 1. Saves image to temporary file
/// 2. Configures Tesseract for constrained recognition:
///    - PSM 8 (Single Word) for isolated text
///    - Character whitelist: numbers, fractions, decimal point
/// 3. Performs OCR extraction
/// 4. Returns structured result with metadata
///
/// # Use Cases
///
/// - Quantity recognition in recipe images
/// - Measurement extraction
/// - Specialized numeric text recognition
///
/// # Performance
///
/// - Optimized for short text segments
/// - Higher confidence scores for constrained recognition
/// - Includes processing time tracking
pub async fn perform_constrained_ocr(
    image: &image::DynamicImage,
    instance_manager: &OcrInstanceManager,
    config: &OcrConfig,
) -> Result<ConstrainedOcrResult, OcrError> {
    let start_time = std::time::Instant::now();

    // Create a timeout for the operation
    let timeout_duration = tokio::time::Duration::from_secs(config.recovery.operation_timeout_secs);

    let result = tokio::time::timeout(timeout_duration, async {
        // Save the preprocessed image to a temporary file
        let temp_file = NamedTempFile::with_suffix(".png").map_err(|e| {
            OcrError::Validation(format!("Failed to create temporary file for OCR: {}", e))
        })?;

        image.save(&temp_file).map_err(|e| {
            OcrError::Validation(format!("Failed to save image to temporary file: {}", e))
        })?;

        let temp_path = temp_file
            .path()
            .to_str()
            .ok_or_else(|| OcrError::Validation("Failed to get temporary file path".to_string()))?;

        // Get an OCR instance from the manager
        let instance = instance_manager
            .get_instance(config)
            .map_err(|e| OcrError::Initialization(e.to_string()))?;

        // Perform constrained OCR with optimized settings
        let (extracted_text, confidence) = {
            let mut tess = instance
                .lock()
                .expect("Failed to acquire Tesseract instance lock");

            // Configure Tesseract for constrained OCR on quantities
            // Use PSM 8 (Single Word) for isolated quantity recognition
            tess.set_variable(leptess::Variable::TesseditPagesegMode, "8")
                .map_err(|e| OcrError::Initialization(format!("Failed to set PSM mode: {}", e)))?;

            // Set strict character whitelist for numbers and fractions only
            let char_whitelist = "0123456789/½⅓⅔¼¾⅕⅖⅚⅙⅛⅜⅝⅞.";
            tess.set_variable(leptess::Variable::TesseditCharWhitelist, char_whitelist)
                .map_err(|e| {
                    OcrError::Initialization(format!("Failed to set character whitelist: {}", e))
                })?;

            // Set the image for OCR processing
            tess.set_image(temp_path).map_err(|e| {
                OcrError::ImageLoad(format!("Failed to load image for constrained OCR: {}", e))
            })?;

            // Extract text
            let text = tess.get_utf8_text().map_err(|e| {
                OcrError::Extraction(format!(
                    "Failed to extract text with constrained OCR: {}",
                    e
                ))
            })?;

            // Use default confidence since leptess doesn't expose detailed confidence
            let confidence = 80.0; // Higher default confidence for constrained OCR

            (text, confidence)
        };

        // Clean up the extracted text
        let cleaned_text = extracted_text
            .trim()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join(" ");

        // Remove OCR artifacts (leading dashes and dots)
        let (artifact_cleaned_text, artifacts_removed) = clean_ocr_artifacts(&cleaned_text);

        // Log cleaning for constrained OCR if artifacts were found
        if artifacts_removed > 0 {
            tracing::debug!(
                "Constrained OCR cleaning removed {} artifacts",
                artifacts_removed
            );
        }

        Ok((artifact_cleaned_text, confidence))
    })
    .await;

    let processing_time_ms = start_time.elapsed().as_millis() as u32;

    match result {
        Ok(Ok((text, confidence))) => {
            info!(
                "Constrained OCR completed in {}ms with confidence {:.1}",
                processing_time_ms, confidence
            );

            Ok(ConstrainedOcrResult {
                text,
                confidence,
                psm_mode: "8 (Single Word)".to_string(),
                character_whitelist: "0123456789/½⅓⅔¼¾⅕⅖⅚⅙⅛⅜⅝⅞.".to_string(),
                processing_time_ms,
            })
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(OcrError::Timeout(format!(
            "Constrained OCR operation timed out after {} seconds",
            config.recovery.operation_timeout_secs
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ocr_artifacts_leading_dash() {
        let input = "- 2 cups flour";
        let expected = "2 cups flour";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 1);
    }

    #[test]
    fn test_clean_ocr_artifacts_leading_dot() {
        let input = "• 1 tsp salt";
        let expected = "1 tsp salt";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 1);
    }

    #[test]
    fn test_clean_ocr_artifacts_regular_dot() {
        let input = ". 500ml milk";
        let expected = "500ml milk";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 1);
    }

    #[test]
    fn test_clean_ocr_artifacts_mixed_artifacts() {
        let input = "-. 3 eggs";
        let expected = "3 eggs";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 2);
    }

    #[test]
    fn test_clean_ocr_artifacts_multiple_artifacts_same_line() {
        let input = "-- 2 tbsp oil";
        let expected = "2 tbsp oil";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 2);
    }

    #[test]
    fn test_clean_ocr_artifacts_preserve_internal_artifacts() {
        let input = "100g sugar - for dusting";
        let expected = "100g sugar - for dusting";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 0);
    }

    #[test]
    fn test_clean_ocr_artifacts_preserve_internal_dots() {
        let input = "1.5 kg potatoes";
        let expected = "1.5 kg potatoes";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 0);
    }

    #[test]
    fn test_clean_ocr_artifacts_multiple_lines() {
        let input = "- 2 cups flour\n• 1 tsp salt\n- 3 eggs";
        let expected = "2 cups flour\n1 tsp salt\n3 eggs";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 3);
    }

    #[test]
    fn test_clean_ocr_artifacts_empty_lines() {
        let input = "- 2 cups flour\n\n• 1 tsp salt";
        let expected = "2 cups flour\n\n1 tsp salt";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 2);
    }

    #[test]
    fn test_clean_ocr_artifacts_whitespace_handling() {
        let input = "  - 2 cups flour";
        let expected = "2 cups flour";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 1);
    }

    #[test]
    fn test_clean_ocr_artifacts_only_artifacts() {
        let input = "-";
        let expected = "";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 1);
    }

    #[test]
    fn test_clean_ocr_artifacts_only_whitespace() {
        let input = "   ";
        let expected = "";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 0);
    }

    #[test]
    fn test_clean_ocr_artifacts_empty_input() {
        let input = "";
        let expected = "";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 0);
    }

    #[test]
    fn test_clean_ocr_artifacts_no_artifacts() {
        let input = "2 cups flour\n1 tsp salt\n3 eggs";
        let expected = "2 cups flour\n1 tsp salt\n3 eggs";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 0);
    }

    #[test]
    fn test_clean_ocr_artifacts_complex_recipe() {
        let input = "- 2 cups all-purpose flour\n• 1 teaspoon salt\n- 3 large eggs\n• 1 cup milk\n- 100g butter - softened";
        let expected = "2 cups all-purpose flour\n1 teaspoon salt\n3 large eggs\n1 cup milk\n100g butter - softened";
        let (result, artifacts) = clean_ocr_artifacts(input);
        assert_eq!(result, expected);
        assert_eq!(artifacts, 5);
    }

    #[test]
    fn test_clean_ocr_artifacts_performance() {
        // Test with a larger recipe text to measure performance
        let input = "- 2 cups all-purpose flour\n• 1 teaspoon salt\n- 3 large eggs\n• 1 cup milk\n- 100g butter - softened\n• 2 tbsp sugar\n- 1 tsp vanilla extract\n• 500ml water\n- Some chopped nuts\n• Fresh berries for topping"
            .repeat(10); // Make it larger for performance testing

        let start = std::time::Instant::now();
        let (result, artifacts) = clean_ocr_artifacts(&input);
        let duration = start.elapsed();

        // Verify the result is correct (should have removed all leading artifacts)
        assert!(!result.contains("\n- "));
        assert!(!result.contains("\n• "));
        assert!(!result.contains("\n. "));

        // Performance check: should be well under 1ms for reasonable input
        assert!(
            duration.as_millis() < 1,
            "Performance test failed: took {}ms",
            duration.as_millis()
        );

        // Should have removed artifacts (10 repetitions * 10 artifacts per repetition, minus any edge cases)
        assert!(
            artifacts > 80,
            "Expected to remove many artifacts, but only removed {}",
            artifacts
        );
    }
}
