//! Image preprocessing pipeline for OCR optimization
//!
//! This module provides adaptive image preprocessing functionality that prepares
//! images for optimal OCR recognition based on assessed image quality.

use tempfile::NamedTempFile;
use tracing::{info, warn};

use crate::ocr_errors::OcrError;

/// Apply adaptive preprocessing based on assessed image quality.
///
/// This function selects an appropriate preprocessing pipeline based on image quality:
/// - High quality: Minimal preprocessing (scaling only)
/// - Medium quality: Standard preprocessing (scaling + light noise reduction)
/// - Low quality: Full preprocessing pipeline (all steps)
///
/// # Arguments
///
/// * `image` - The input image to preprocess
/// * `quality` - The assessed image quality
///
/// # Returns
///
/// Returns the preprocessed image and strategy description
pub fn apply_adaptive_preprocessing(
    image: &image::DynamicImage,
    quality: &crate::preprocessing::types::ImageQualityResult,
) -> Result<super::types::AdaptivePreprocessingResult, OcrError> {
    match quality.quality {
        crate::preprocessing::types::ImageQuality::High => {
            // High quality images: minimal preprocessing for speed
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                OcrError::Extraction(format!("High quality scaling failed: {:?}", e))
            })?;

            Ok(super::types::AdaptivePreprocessingResult {
                image: scaled_result.image,
                preprocessing_strategy: "high_quality_minimal".to_string(),
            })
        }

        crate::preprocessing::types::ImageQuality::Medium => {
            // Medium quality images: standard preprocessing
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                OcrError::Extraction(format!("Medium quality scaling failed: {:?}", e))
            })?;

            // Light noise reduction (lower sigma for medium quality)
            let denoised_result = crate::preprocessing::filtering::reduce_noise(
                &scaled_result.image,
                0.8,
            )
            .map_err(|e| {
                OcrError::Extraction(format!("Medium quality noise reduction failed: {:?}", e))
            })?;

            // Basic thresholding
            let thresholded_result =
                crate::preprocessing::thresholding::apply_otsu_threshold(&denoised_result.image)
                    .map_err(|e| {
                        OcrError::Extraction(format!("Medium quality thresholding failed: {:?}", e))
                    })?;

            Ok(super::types::AdaptivePreprocessingResult {
                image: thresholded_result.image,
                preprocessing_strategy: "medium_quality_standard".to_string(),
            })
        }

        crate::preprocessing::types::ImageQuality::Low => {
            // Low quality images: full preprocessing pipeline
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                OcrError::Extraction(format!("Low quality scaling failed: {:?}", e))
            })?;

            // Apply CLAHE for contrast enhancement if contrast is low
            let preprocessing_image = if quality.contrast_ratio < 0.3 {
                let clahe_result =
                    crate::preprocessing::filtering::apply_clahe(&scaled_result.image, 3.0, (8, 8))
                        .map_err(|e| {
                            OcrError::Extraction(format!("Low quality CLAHE failed: {:?}", e))
                        })?;
                clahe_result.image
            } else {
                // No CLAHE needed, use scaled image directly
                scaled_result.image
            };

            // Deskewing for low quality images (corrects text rotation)
            let deskew_result = crate::preprocessing::deskewing::deskew_image(&preprocessing_image)
                .map_err(|e| {
                    OcrError::Extraction(format!("Low quality deskewing failed: {:?}", e))
                })?;

            // Strong noise reduction
            let denoised_result =
                crate::preprocessing::filtering::reduce_noise(&deskew_result.image, 1.2).map_err(
                    |e| {
                        OcrError::Extraction(format!("Low quality noise reduction failed: {:?}", e))
                    },
                )?;

            // Thresholding
            let thresholded_result =
                crate::preprocessing::thresholding::apply_otsu_threshold(&denoised_result.image)
                    .map_err(|e| {
                        OcrError::Extraction(format!("Low quality thresholding failed: {:?}", e))
                    })?;

            // Morphological operations for cleanup
            let opened_result = crate::preprocessing::filtering::apply_morphological_operation(
                &thresholded_result.image,
                crate::preprocessing::types::MorphologicalOperation::Opening,
            )
            .map_err(|e| {
                OcrError::Extraction(format!("Low quality morphological opening failed: {:?}", e))
            })?;

            let final_result = crate::preprocessing::filtering::apply_morphological_operation(
                &opened_result.image,
                crate::preprocessing::types::MorphologicalOperation::Closing,
            )
            .map_err(|e| {
                OcrError::Extraction(format!("Low quality morphological closing failed: {:?}", e))
            })?;

            Ok(super::types::AdaptivePreprocessingResult {
                image: final_result.image,
                preprocessing_strategy: if quality.contrast_ratio < 0.3 {
                    "low_quality_full_with_clahe_deskew".to_string()
                } else {
                    "low_quality_full_with_deskew".to_string()
                },
            })
        }
    }
}

/// Apply image preprocessing for OCR optimization
///
/// This function loads an image, applies OCR-optimized preprocessing (scaling),
/// and saves the result to a temporary file for Tesseract processing.
///
/// # Arguments
///
/// * `image_path` - Path to the original image file
/// * `config` - OCR configuration (unused for now, but for future extensibility)
///
/// # Returns
///
/// Returns a tuple of (temp_file, temp_file_path, preprocessing_duration) on success.
/// The NamedTempFile will be automatically cleaned up when it goes out of scope,
/// but the OCR operation should complete before that happens.
///
/// # Errors
///
/// Returns `OcrError::ImageLoad` if the image cannot be loaded or processed.
/// Returns `OcrError::ProcessingFailed` if preprocessing fails.
pub async fn apply_image_preprocessing(
    image_path: &str,
    _config: &super::OcrConfig,
) -> Result<(NamedTempFile, String, std::time::Duration), OcrError> {
    let preprocessing_start = std::time::Instant::now();

    // Load the original image
    let img = match image::open(image_path) {
        Ok(img) => img,
        Err(e) => {
            // If image loading fails, fall back to using the original image without preprocessing
            warn!(
                "Image preprocessing failed to load image '{}': {}. Using original image for OCR.",
                image_path, e
            );

            // Create a temporary file that points to the original image
            let temp_file = NamedTempFile::new().map_err(|e| {
                OcrError::Extraction(format!(
                    "Failed to create temporary file for fallback: {}",
                    e
                ))
            })?;

            // Copy the original file to the temp location
            std::fs::copy(image_path, temp_file.path()).map_err(|e| {
                OcrError::Extraction(format!("Failed to copy original image for fallback: {}", e))
            })?;

            let temp_path = temp_file.path().to_string_lossy().to_string();
            let preprocessing_duration = preprocessing_start.elapsed();

            info!(
                target: "ocr_preprocessing",
                "Using original image (preprocessing failed): fallback took {:.2}ms",
                preprocessing_duration.as_millis()
            );

            return Ok((temp_file, temp_path, preprocessing_duration));
        }
    };

    // Assess image quality to determine preprocessing strategy
    let quality_result = crate::preprocessing::quality::assess_image_quality(&img)
        .map_err(|e| OcrError::Extraction(format!("Quality assessment failed: {:?}", e)))?;

    // Apply adaptive preprocessing based on image quality
    let processed_image = apply_adaptive_preprocessing(&img, &quality_result)
        .map_err(|e| OcrError::Extraction(format!("Adaptive preprocessing failed: {:?}", e)))?;

    // Create a temporary file for the preprocessed image
    let temp_file = NamedTempFile::with_suffix(".png")
        .map_err(|e| OcrError::Extraction(format!("Failed to create temporary file: {}", e)))?;

    // Save the preprocessed image to the temporary file
    processed_image
        .image
        .save_with_format(temp_file.path(), image::ImageFormat::Png)
        .map_err(|e| OcrError::Extraction(format!("Failed to save preprocessed image: {}", e)))?;

    let temp_path = temp_file.path().to_string_lossy().to_string();
    let preprocessing_duration = preprocessing_start.elapsed();

    info!(
        target: "ocr_preprocessing",
        "Adaptive preprocessing completed in {:.2}ms: quality={:?} (contrast: {:.3}, brightness: {:.3}, sharpness: {:.3}), strategy: {}",
        preprocessing_duration.as_millis(),
        quality_result.quality,
        quality_result.contrast_ratio,
        quality_result.brightness,
        quality_result.sharpness,
        processed_image.preprocessing_strategy
    );

    Ok((temp_file, temp_path, preprocessing_duration))
}
