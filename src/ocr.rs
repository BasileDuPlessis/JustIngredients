//! # OCR Processing Module
//!
//! This module provides optical character recognition (OCR) functionality for extracting
//! text from images using the Tesseract OCR engine.
//!
//! ## Features
//!
//! - Text extraction from images using Tesseract OCR
//! - Automatic image format detection and validation
//! - Support for multiple languages (default: English and French)
//! - Comprehensive error handling and logging
//!
//! ## Supported Image Formats
//!
//! - PNG (Portable Network Graphics)
//! - JPEG/JPG (Joint Photographic Experts Group)
//! - BMP (Bitmap)
//! - TIFF/TIF (Tagged Image File Format)
//!
//! ## Dependencies
//!
//! - `leptess`: Rust bindings for Tesseract OCR and Leptonica
//! - `image`: Image format detection and processing
//! - `anyhow`: Error handling
//! - `log`: Logging functionality

use anyhow::Result;
use tempfile::NamedTempFile;
use tracing::{info, warn};

// Submodules
mod confidence;
mod hocr;
mod retry;
mod types;
mod validation;

// Re-export types for easier access from documentation and external usage
pub use crate::circuit_breaker::CircuitBreaker;
use crate::error_correction::OcrErrorCorrector;
use crate::errors::error_logging;
pub use crate::instance_manager::OcrInstanceManager;
pub use crate::observability;
pub use crate::ocr_config::{OcrConfig, RecoveryConfig};
pub use crate::ocr_errors::OcrError;

// Re-export types from submodule
pub use self::types::*;

// Re-export retry utilities from submodule
pub use self::retry::*;

// Re-export confidence utilities from submodule
pub use self::confidence::*;

// Re-export validation utilities from submodule
pub use self::validation::*;

// Re-export HOCR utilities from submodule
pub use self::hocr::*;

/// Extract text from an image file using OCR with comprehensive error handling and retry logic
///
/// This function implements a robust OCR processing pipeline with the following algorithm:
///
/// ## Processing Algorithm
///
/// ```text
/// 1. Circuit Breaker Check
///    - Check if circuit breaker is open (service unavailable)
///    - Return early if open to prevent cascading failures
///
/// 2. Input Validation
///    - Validate image format and size limits
///    - Pre-calculate memory requirements
///
/// 3. Retry Loop (up to max_retries + 1 attempts)
///    For each attempt:
///      a. Perform OCR extraction with timeout
///      b. On success: Record success, update metrics, return result
///      c. On failure: Calculate delay, wait, retry
///      d. After max attempts: Record failure, return error
///
/// 4. Circuit Breaker Updates
///    - Record success/failure to track system health
///    - Update circuit breaker state based on thresholds
/// ```
///
/// ## Circuit Breaker Integration
///
/// The circuit breaker prevents system overload during OCR failures:
/// - **Open State**: When failure threshold exceeded, rejects requests fast
/// - **Closed State**: Normal operation, allows all requests
/// - **Half-Open State**: Testing recovery after timeout
///
/// ## Retry Strategy
///
/// Implements exponential backoff with jitter to prevent thundering herd:
/// - **Base Delay**: Configurable starting delay (default: 1000ms)
/// - **Exponential Growth**: Delay doubles each retry (2^(attempt-1))
/// - **Maximum Cap**: Prevents excessively long delays (default: 10000ms)
/// - **Jitter**: Random variation prevents synchronized retries
///
/// ## Performance Monitoring
///
/// Comprehensive metrics collection:
/// - **Timing**: Total duration and OCR-specific processing time
/// - **Success Rates**: Attempt counts and failure patterns
/// - **Resource Usage**: Memory estimates and file size tracking
/// - **Circuit State**: Breaker state changes and threshold tracking
///
/// ## Error Recovery Flow
///
/// ```text
/// OCR Failure → Check Retry Count → Max Retries Exceeded?
///     ├── Yes → Record Circuit Failure → Return Error
///     └── No → Calculate Delay → Wait → Retry
///
/// Circuit Open → Fast Fail → Return Service Unavailable
/// ```
///
/// # Arguments
///
/// * `image_path` - Path to the image file to process (must be absolute path)
/// * `config` - OCR configuration including language settings, timeouts, and recovery options
/// * `instance_manager` - Manager for OCR instance reuse to improve performance
/// * `circuit_breaker` - Circuit breaker for fault tolerance and cascading failure prevention
///
/// # Returns
///
/// Returns `Result<(String, OcrConfidence), OcrError>` containing the extracted text and confidence score, or an error
///
/// The confidence score provides quality assessment of the OCR result with the following components:
/// - **Overall Score** (0.0-1.0): Weighted combination of quality metrics
/// - **Text Quality Score**: Based on text length, content, and noise ratio
/// - **Pattern Score**: Presence of expected recipe content (measurements, ingredients)
/// - **Processing Score**: Based on OCR processing time
/// - **Flags**: Specific issues detected (too short, no measurements, slow processing, etc.)
///
/// # Examples
///
/// ```rust,no_run
/// use just_ingredients::ocr::{extract_text_from_image, OcrConfig, OcrInstanceManager, CircuitBreaker};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = OcrConfig::default();
/// let instance_manager = OcrInstanceManager::new();
/// let circuit_breaker = CircuitBreaker::new(config.recovery.clone());
///
/// // Process an image of ingredients
/// let (text, confidence) = extract_text_from_image("/path/to/ingredients.jpg", &config, &instance_manager, &circuit_breaker).await?;
/// println!("Extracted text: {}", text);
/// println!("Confidence score: {:.2}", confidence.overall_score);
/// # Ok(())
/// # }
/// ```
///
/// # Supported Image Formats
///
/// - PNG (Portable Network Graphics) - up to 15MB
/// - JPEG/JPG (Joint Photographic Experts Group) - up to 10MB
/// - BMP (Bitmap) - up to 5MB
/// - TIFF/Tagged Image File Format) - up to 20MB
///
/// # Performance
///
/// - Includes automatic retry logic (up to 3 attempts by default)
/// - Uses OCR instance reuse for better performance
/// - Circuit breaker protection against cascading failures
/// - Comprehensive timing metrics logged at INFO level
///
/// # Circuit Breaker Protection
///
/// The circuit breaker prevents cascading failures by:
/// - Opening when failure threshold is exceeded (default: 5 failures)
/// - Failing fast when open to protect system resources
/// - Automatically resetting after timeout (default: 60 seconds)
/// - Recording success/failure to track system health
///
/// # Errors
///
/// Returns `OcrError` for various failure conditions:
/// - `ValidationError` - Image format not supported or file too large
/// - `InitializationError` - OCR engine initialization failed
/// - `ImageLoadError` - Could not load the image file
/// - `ExtractionError` - OCR processing failed
/// - `TimeoutError` - Operation exceeded timeout (30s default)
/// - **Extract text from an image using OCR with comprehensive error handling and performance monitoring**
///
/// This function implements a robust OCR extraction pipeline that handles image processing,
/// text recognition, and failure recovery. It includes circuit breaker protection, retry logic,
/// format validation, and detailed performance monitoring.
///
/// ## Algorithm Overview
///
/// The OCR extraction process follows this sequence:
/// 1. **Circuit Breaker Check**: Verify service availability before processing
/// 2. **Input Validation**: Comprehensive format and size validation
/// 3. **Retry Loop**: Exponential backoff retry logic for transient failures
/// 4. **OCR Processing**: Core Tesseract text extraction with timeout protection
/// 5. **Success Handling**: Record metrics and return extracted text
/// 6. **Failure Handling**: Circuit breaker updates and error propagation
///
/// ## Processing Stages
///
/// ### Stage 1: Circuit Breaker Protection
/// **Algorithm**: Prevent system overload during OCR service failures
///
/// **Protection Logic**:
/// ```text
/// if circuit_breaker.is_open():
///     return "OCR service temporarily unavailable"
/// ```
///
/// **Purpose**: Maintain system stability during extended OCR failures
/// **State Tracking**: Updates observability metrics for monitoring
/// **Recovery**: Automatic recovery when service becomes available again
///
/// ### Stage 2: Enhanced Input Validation
/// **Algorithm**: Multi-layer validation of image files before processing
///
/// **Validation Checks**:
/// - **File Existence**: Verify image file is accessible
/// - **Format Detection**: Identify image format from magic bytes
/// - **Size Limits**: Format-specific size constraints
///   - PNG: 15MB maximum
///   - JPEG: 10MB maximum
///   - BMP: 5MB maximum
///   - TIFF: 20MB maximum
/// - **Memory Estimation**: Pre-calculate processing memory requirements
///
/// **Error Handling**: Detailed validation errors with specific failure reasons
///
/// ### Stage 3: Retry Logic with Exponential Backoff
/// **Algorithm**: Intelligent retry strategy for transient OCR failures
///
/// **Retry Configuration**:
/// - **Max Attempts**: Configurable retry count (default: 3)
/// - **Backoff Strategy**: Exponential delay with jitter
/// - **Delay Calculation**: `delay = min(base_delay * 2^(attempt-1), max_delay) + jitter`
/// - **Jitter Range**: Random component (0 to delay/4) to prevent thundering herd
///
/// **Retry Progression**:
/// | Attempt | Base Delay | Range (with jitter) |
/// |---------|------------|---------------------|
/// | 1       | 1000ms     | 1000-1250ms        |
/// | 2       | 2000ms     | 2000-2500ms        |
/// | 3       | 4000ms     | 4000-5000ms        |
///
/// ### Stage 4: Core OCR Processing
/// **Algorithm**: Tesseract-based text extraction with timeout protection
///
/// **Processing Steps**:
/// 1. **Instance Acquisition**: Get or create OCR instance from pool
/// 2. **Image Loading**: Load image into Tesseract engine
/// 3. **Text Extraction**: Perform OCR recognition
/// 4. **Text Cleanup**: Remove extra whitespace and empty lines
/// 5. **Timeout Protection**: 30-second operation timeout
///
/// **Instance Management**:
/// - **Reuse Strategy**: Cached instances by language combination
/// - **Performance Benefit**: Eliminates 100-500ms initialization overhead
/// - **Thread Safety**: Protected by mutex for concurrent access
///
/// ### Stage 5: Success Path Handling
/// **Algorithm**: Comprehensive success processing and metric recording
///
/// **Success Actions**:
/// - **Circuit Breaker**: Record success to improve availability
/// - **Performance Metrics**: Record timing, size, and resource usage
/// - **Observability**: Update monitoring systems with success data
/// - **Logging**: Detailed success information with character counts
///
/// **Metrics Collected**:
/// - Total processing duration
/// - OCR-specific processing time
/// - Image file size
/// - Retry attempt count
/// - Memory usage estimation
/// - Character count of extracted text
///
/// ### Stage 6: Failure Path Handling
/// **Algorithm**: Comprehensive failure processing and system protection
///
/// **Failure Actions**:
/// - **Circuit Breaker**: Record failure to trigger protection if needed
/// - **Retry Logic**: Continue retry loop or fail permanently
/// - **Error Propagation**: Return detailed error information
/// - **Logging**: Comprehensive failure diagnostics
///
/// ## Error Types and Handling
///
/// ### Circuit Breaker Errors
/// - **Trigger**: Service temporarily unavailable due to repeated failures
/// - **User Message**: "OCR service is temporarily unavailable..."
/// - **Recovery**: Automatic when circuit breaker resets
///
/// ### Validation Errors
/// - **File Access**: Cannot read image file
/// - **Format Unsupported**: Image format not supported by Tesseract
/// - **Size Limits**: File exceeds format-specific size limits
/// - **Corruption**: File appears corrupted or invalid
///
/// ### Processing Errors
/// - **Timeout**: Operation exceeded 30-second limit
/// - **OCR Failure**: Tesseract processing failed
/// - **Instance Error**: Could not acquire OCR instance
/// - **Memory Error**: Insufficient memory for processing
///
/// ## Performance Characteristics
///
/// - **Memory Usage**: Pre-calculated estimates prevent OOM conditions
/// - **CPU Usage**: Tesseract processing is CPU-intensive but optimized
/// - **I/O Patterns**: Minimal I/O with format detection buffers
/// - **Concurrent Safety**: Thread-safe with mutex-protected instances
/// - **Scalability**: Instance pooling supports multiple concurrent requests
///
/// ## Observability and Monitoring
///
/// ### Metrics Tracked
/// - **Success Rate**: OCR operation success/failure ratios
/// - **Processing Time**: Total and OCR-specific durations
/// - **Retry Patterns**: Attempt counts and delay distributions
/// - **Resource Usage**: Memory and file size correlations
/// - **Circuit Breaker State**: Protection mechanism status
///
/// ### Logging Levels
/// - **Info**: Successful extractions with performance data
/// - **Warn**: Retries and timeouts with timing information
/// - **Error**: Permanent failures with comprehensive diagnostics
/// - **Debug**: Detailed processing steps and intermediate results
///
/// ## Configuration Integration
///
/// ### OCR Configuration (`OcrConfig`)
/// - **Language Settings**: Tesseract language combinations (eng+fra)
/// - **Timeout Settings**: Operation timeout limits (30 seconds)
/// - **Size Limits**: Format-specific file size constraints
/// - **Buffer Sizes**: Format detection buffer configuration
///
/// ### Recovery Configuration (`RecoveryConfig`)
/// - **Retry Settings**: Maximum attempts and delay parameters
/// - **Circuit Breaker**: Failure thresholds and reset timeouts
/// - **Timeout Protection**: Operation timeout enforcement
///
/// ## Thread Safety and Concurrency
///
/// - **Instance Pooling**: Thread-safe OCR instance management
/// - **Circuit Breaker**: Atomic state updates for concurrent access
/// - **File Access**: Safe concurrent file operations
/// - **Metrics Recording**: Thread-safe observability updates
///
/// ## Integration Points
///
/// - **Circuit Breaker**: Fault tolerance and system protection
/// - **Instance Manager**: OCR instance lifecycle management
/// - **Observability**: Performance monitoring and alerting
/// - **Configuration**: Runtime behavior customization
/// - **Error Handling**: Comprehensive error classification
///
/// # Arguments
///
/// * `image_path` - Absolute path to the image file for OCR processing
/// * `config` - OCR configuration with timeout, language, and size limit settings
/// * `instance_manager` - Manager for OCR instance reuse and lifecycle
/// * `circuit_breaker` - Circuit breaker for fault tolerance and overload protection
///
/// # Returns
///
/// Returns a tuple of `(String, OcrConfidence)` containing the extracted text and confidence score on success, or an `OcrError` on failure
///
/// # Examples
///
/// ```rust,no_run
/// use just_ingredients::ocr::{extract_text_from_image, OcrConfig, OcrInstanceManager, CircuitBreaker};
/// use just_ingredients::ocr_config::RecoveryConfig;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Initialize components
/// let config = OcrConfig::default();
/// let instance_manager = OcrInstanceManager::new();
/// let recovery_config = RecoveryConfig::default();
/// let circuit_breaker = CircuitBreaker::new(recovery_config);
///
/// // Extract text from image
/// let (extracted_text, confidence) = extract_text_from_image(
///     "/path/to/ingredients.jpg",
///     &config,
///     &instance_manager,
///     &circuit_breaker
/// ).await?;
///
/// println!("Extracted text: {}", extracted_text);
/// println!("Confidence: {:?}", confidence);
/// # Ok(())
/// # }
/// ```
///
/// # Error Examples
///
/// ```rust,no_run
/// use just_ingredients::ocr::{extract_text_from_image, OcrConfig, OcrInstanceManager, CircuitBreaker};
///
/// # async fn error_example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = OcrConfig::default();
/// let instance_manager = OcrInstanceManager::new();
/// let recovery_config = just_ingredients::ocr_config::RecoveryConfig::default();
/// let circuit_breaker = CircuitBreaker::new(recovery_config);
///
/// // Circuit breaker open
/// match extract_text_from_image("/large/image.png", &config, &instance_manager, &circuit_breaker).await {
///     Ok((text, confidence)) => println!("Success: {} (confidence: {:?})", text, confidence),
///     Err(e) => println!("Error: {:?}", e), // May be circuit breaker error
/// }
/// # Ok(())
/// # }
/// ```
///
/// Correct common OCR errors with fraction characters
///
/// OCR engines often misread fraction symbols (/) as other characters.
pub async fn extract_text_from_image(
    image_path: &str,
    config: &crate::ocr_config::OcrConfig,
    instance_manager: &crate::instance_manager::OcrInstanceManager,
    circuit_breaker: &crate::circuit_breaker::CircuitBreaker,
) -> Result<(String, OcrConfidence), crate::ocr_errors::OcrError> {
    // Create a tracing span for the OCR operation
    let span = crate::observability::ocr_span("extract_text_from_image");
    let _enter = span.enter();

    // Start timing the entire OCR operation
    let start_time = std::time::Instant::now();

    // Check circuit breaker before processing
    if circuit_breaker.is_open() {
        warn!("Circuit breaker is open, rejecting OCR request for image: {image_path}");
        observability::update_circuit_breaker_state(true);
        return Err(crate::ocr_errors::OcrError::Extraction(
            "OCR service is temporarily unavailable due to repeated failures. Please try again later.".to_string()
        ));
    }
    observability::update_circuit_breaker_state(false);

    // Validate input with enhanced format-specific validation
    validate_image_with_format_limits(image_path, config)
        .map_err(|e| crate::ocr_errors::OcrError::Validation(e.to_string()))?;

    info!("Starting OCR text extraction from image: {image_path}");

    // Implement retry logic with exponential backoff
    let mut attempt = 0;
    let max_attempts = config.recovery.max_retries + 1; // +1 for initial attempt

    loop {
        attempt += 1;

        match perform_ocr_extraction(image_path, config, instance_manager).await {
            Ok((text, tesseract_confidence, ocr_duration)) => {
                let total_duration = start_time.elapsed();
                let total_ms = total_duration.as_millis();

                // Calculate OCR confidence score (now incorporating Tesseract's confidence)
                let confidence = calculate_ocr_confidence_with_tesseract(
                    &text,
                    tesseract_confidence,
                    ocr_duration,
                    config,
                );

                // Record success in circuit breaker
                circuit_breaker.record_success();
                observability::update_circuit_breaker_state(false);

                // Record OCR metrics with enhanced performance data
                let image_size = std::fs::metadata(image_path).map(|m| m.len()).unwrap_or(0);
                let memory_estimate =
                    crate::ocr::estimate_memory_usage(image_size, &image::ImageFormat::Png);
                observability::record_ocr_performance_metrics(
                    observability::OcrPerformanceMetricsParams {
                        success: true,
                        total_duration,
                        ocr_duration,
                        image_size,
                        attempt_count: attempt,
                        memory_estimate_mb: memory_estimate,
                    },
                );

                info!("OCR extraction completed successfully on attempt {} in {}ms. Extracted {} characters of text (Tesseract: {:.1}%, overall: {:.2})",
                      attempt, total_ms, text.len(), tesseract_confidence, confidence.overall_score);

                // Log confidence warnings if needed
                if should_flag_for_review(&confidence, 0.7) {
                    warn!(
                        "OCR result flagged for review: {}",
                        get_confidence_description(&confidence)
                    );
                }

                return Ok((text, confidence));
            }
            Err(err) => {
                if attempt >= max_attempts {
                    let total_duration = start_time.elapsed();

                    // Record failure in circuit breaker
                    circuit_breaker.record_failure();
                    observability::update_circuit_breaker_state(circuit_breaker.is_open());

                    // Record OCR metrics with enhanced performance data
                    let image_size = std::fs::metadata(image_path).map(|m| m.len()).unwrap_or(0);
                    let memory_estimate =
                        crate::ocr::estimate_memory_usage(image_size, &image::ImageFormat::Png);
                    observability::record_ocr_performance_metrics(
                        observability::OcrPerformanceMetricsParams {
                            success: false,
                            total_duration,
                            ocr_duration: std::time::Duration::from_millis(0), // No successful OCR duration on failure
                            image_size,
                            attempt_count: attempt,
                            memory_estimate_mb: memory_estimate,
                        },
                    );

                    error_logging::log_ocr_error(
                        &err,
                        "ocr_extraction_retry",
                        None, // user_id not available in this context
                        Some(image_size),
                        Some(total_duration),
                    );
                    return Err(err);
                }

                let delay_ms = calculate_retry_delay(attempt, &config.recovery);
                warn!("OCR extraction attempt {attempt} failed: {err:?}. Retrying in {delay_ms}ms");

                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
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
/// - `ImageLoadError` - Could not load image into Tesseract
/// - `ExtractionError` - OCR processing failed
/// - `TimeoutError` - Operation exceeded configured timeout
///
/// Applies adaptive preprocessing based on assessed image quality.
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
) -> Result<AdaptivePreprocessingResult, crate::ocr_errors::OcrError> {
    match quality.quality {
        crate::preprocessing::types::ImageQuality::High => {
            // High quality images: minimal preprocessing for speed
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "High quality scaling failed: {:?}",
                    e
                ))
            })?;

            Ok(AdaptivePreprocessingResult {
                image: scaled_result.image,
                preprocessing_strategy: "high_quality_minimal".to_string(),
            })
        }

        crate::preprocessing::types::ImageQuality::Medium => {
            // Medium quality images: standard preprocessing
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Medium quality scaling failed: {:?}",
                    e
                ))
            })?;

            // Light noise reduction (lower sigma for medium quality)
            let denoised_result =
                crate::preprocessing::filtering::reduce_noise(&scaled_result.image, 0.8).map_err(
                    |e| {
                        crate::ocr_errors::OcrError::Extraction(format!(
                            "Medium quality noise reduction failed: {:?}",
                            e
                        ))
                    },
                )?;

            // Basic thresholding
            let thresholded_result =
                crate::preprocessing::thresholding::apply_otsu_threshold(&denoised_result.image)
                    .map_err(|e| {
                        crate::ocr_errors::OcrError::Extraction(format!(
                            "Medium quality thresholding failed: {:?}",
                            e
                        ))
                    })?;

            Ok(AdaptivePreprocessingResult {
                image: thresholded_result.image,
                preprocessing_strategy: "medium_quality_standard".to_string(),
            })
        }

        crate::preprocessing::types::ImageQuality::Low => {
            // Low quality images: full preprocessing pipeline
            let scaler = crate::preprocessing::scaling::ImageScaler::new();
            let scaled_result = scaler.scale_for_ocr(image).map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Low quality scaling failed: {:?}",
                    e
                ))
            })?;

            // Apply CLAHE for contrast enhancement if contrast is low
            let preprocessing_image = if quality.contrast_ratio < 0.3 {
                let clahe_result =
                    crate::preprocessing::filtering::apply_clahe(&scaled_result.image, 3.0, (8, 8))
                        .map_err(|e| {
                            crate::ocr_errors::OcrError::Extraction(format!(
                                "Low quality CLAHE failed: {:?}",
                                e
                            ))
                        })?;
                clahe_result.image
            } else {
                // No CLAHE needed, use scaled image directly
                scaled_result.image
            };

            // Deskewing for low quality images (corrects text rotation)
            let deskew_result = crate::preprocessing::deskewing::deskew_image(&preprocessing_image)
                .map_err(|e| {
                    crate::ocr_errors::OcrError::Extraction(format!(
                        "Low quality deskewing failed: {:?}",
                        e
                    ))
                })?;

            // Strong noise reduction
            let denoised_result =
                crate::preprocessing::filtering::reduce_noise(&deskew_result.image, 1.2).map_err(
                    |e| {
                        crate::ocr_errors::OcrError::Extraction(format!(
                            "Low quality noise reduction failed: {:?}",
                            e
                        ))
                    },
                )?;

            // Thresholding
            let thresholded_result =
                crate::preprocessing::thresholding::apply_otsu_threshold(&denoised_result.image)
                    .map_err(|e| {
                        crate::ocr_errors::OcrError::Extraction(format!(
                            "Low quality thresholding failed: {:?}",
                            e
                        ))
                    })?;

            // Morphological operations for cleanup
            let opened_result = crate::preprocessing::filtering::apply_morphological_operation(
                &thresholded_result.image,
                crate::preprocessing::types::MorphologicalOperation::Opening,
            )
            .map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Low quality morphological opening failed: {:?}",
                    e
                ))
            })?;

            let final_result = crate::preprocessing::filtering::apply_morphological_operation(
                &opened_result.image,
                crate::preprocessing::types::MorphologicalOperation::Closing,
            )
            .map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Low quality morphological closing failed: {:?}",
                    e
                ))
            })?;

            Ok(AdaptivePreprocessingResult {
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
async fn apply_image_preprocessing(
    image_path: &str,
    _config: &crate::ocr_config::OcrConfig,
) -> Result<(NamedTempFile, String, std::time::Duration), crate::ocr_errors::OcrError> {
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
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Failed to create temporary file for fallback: {}",
                    e
                ))
            })?;

            // Copy the original file to the temp location
            std::fs::copy(image_path, temp_file.path()).map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Failed to copy original image for fallback: {}",
                    e
                ))
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
    let quality_result =
        crate::preprocessing::quality::assess_image_quality(&img).map_err(|e| {
            crate::ocr_errors::OcrError::Extraction(format!("Quality assessment failed: {:?}", e))
        })?;

    // Apply adaptive preprocessing based on image quality
    let processed_image = apply_adaptive_preprocessing(&img, &quality_result).map_err(|e| {
        crate::ocr_errors::OcrError::Extraction(format!("Adaptive preprocessing failed: {:?}", e))
    })?;

    // Create a temporary file for the preprocessed image
    let temp_file = NamedTempFile::with_suffix(".png").map_err(|e| {
        crate::ocr_errors::OcrError::Extraction(format!("Failed to create temporary file: {}", e))
    })?;

    // Save the preprocessed image to the temporary file
    processed_image
        .image
        .save_with_format(temp_file.path(), image::ImageFormat::Png)
        .map_err(|e| {
            crate::ocr_errors::OcrError::Extraction(format!(
                "Failed to save preprocessed image: {}",
                e
            ))
        })?;

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

async fn perform_ocr_extraction(
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
            apply_image_preprocessing(image_path, config).await?;

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
            // NOTE: The leptess crate (v0.14) does not expose Tesseract's confidence methods.
            // Using a default confidence score based on successful OCR completion.
            // TODO: Consider using a different Tesseract binding that exposes confidence scores.
            let confidence = 75.0; // Default confidence for successful OCR

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

        // Apply comprehensive OCR error correction
        let error_corrector = OcrErrorCorrector::new();
        let corrected_text = error_corrector.correct_text(&cleaned_text);

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

        Ok((cleaned_text, confidence))
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
