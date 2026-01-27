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
use regex;
use std::fs::File;
use std::io::{BufReader, Read};
use tracing::{info, warn};

// Re-export types for easier access from documentation and external usage
pub use crate::circuit_breaker::CircuitBreaker;
use crate::errors::error_logging;
pub use crate::instance_manager::OcrInstanceManager;
pub use crate::observability;
pub use crate::ocr_config::{OcrConfig, RecoveryConfig};
pub use crate::ocr_errors::OcrError;

/// Validate image file path and basic properties using enhanced security validation
pub fn validate_image_path(image_path: &str, config: &crate::ocr_config::OcrConfig) -> Result<()> {
    // Use the comprehensive path validation module
    crate::path_validation::validate_file_path(image_path)
        .map_err(|e| anyhow::anyhow!("Image path validation failed: {}", e))?;

    // Additional OCR-specific validation
    let path = std::path::Path::new(image_path);

    // Check if file exists
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Image path validation failed: file does not exist ({})",
            image_path
        ));
    }

    // Check if it's actually a file (not a directory)
    if !path.is_file() {
        return Err(anyhow::anyhow!(
            "Image path validation failed: path is not a file ({})",
            image_path
        ));
    }

    // Check file size
    match path.metadata() {
        Ok(metadata) => {
            let file_size = metadata.len();
            if file_size > config.max_file_size {
                return Err(anyhow::anyhow!(
                    "Image validation failed: file too large ({} bytes, maximum allowed: {} bytes)",
                    file_size,
                    config.max_file_size
                ));
            }
            if file_size == 0 {
                return Err(anyhow::anyhow!(
                    "Image validation failed: file is empty ({})",
                    image_path
                ));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Image validation failed: cannot read file metadata ({}) - {}",
                image_path,
                e
            ));
        }
    }

    Ok(())
}

/// Enhanced validation with format-specific size limits and progressive validation
pub fn validate_image_with_format_limits(
    image_path: &str,
    config: &crate::ocr_config::OcrConfig,
) -> Result<()> {
    // First, perform comprehensive path validation
    crate::path_validation::validate_file_path(image_path)
        .map_err(|e| anyhow::anyhow!("Image path validation failed: {}", e))?;

    // Additional OCR-specific validation
    let path = std::path::Path::new(image_path);

    // Check if file exists
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Image validation failed: file does not exist ({})",
            image_path
        ));
    }

    // Check if it's actually a file (not a directory)
    if !path.is_file() {
        return Err(anyhow::anyhow!(
            "Image validation failed: path is not a file ({})",
            image_path
        ));
    }

    let file_size = path.metadata()?.len();

    // Quick rejection for extremely large files
    if file_size > config.format_limits.min_quick_reject {
        info!(
            "Quick rejecting file {image_path}: {file_size} bytes exceeds quick reject threshold"
        );
        return Err(anyhow::anyhow!(
            "File too large for processing: {} bytes (exceeds quick reject threshold of {} bytes)",
            file_size,
            config.format_limits.min_quick_reject
        ));
    }

    // Try to detect format and apply format-specific limits
    match File::open(image_path) {
        Ok(file) => {
            let mut reader = BufReader::new(file);
            let mut buffer = vec![0; config.buffer_size];

            match reader.read(&mut buffer) {
                Ok(bytes_read) if bytes_read >= config.min_format_bytes => {
                    buffer.truncate(bytes_read);

                    match image::guess_format(&buffer) {
                        Ok(format) => {
                            let format_limit = match format {
                                image::ImageFormat::Png => {
                                    info!(
                                        "Detected PNG format for {}, applying {}MB limit",
                                        image_path,
                                        config.format_limits.png_max / (1024 * 1024)
                                    );
                                    config.format_limits.png_max
                                }
                                image::ImageFormat::Jpeg => {
                                    info!(
                                        "Detected JPEG format for {}, applying {}MB limit",
                                        image_path,
                                        config.format_limits.jpeg_max / (1024 * 1024)
                                    );
                                    config.format_limits.jpeg_max
                                }
                                image::ImageFormat::Bmp => {
                                    info!(
                                        "Detected BMP format for {}, applying {}MB limit",
                                        image_path,
                                        config.format_limits.bmp_max / (1024 * 1024)
                                    );
                                    config.format_limits.bmp_max
                                }
                                image::ImageFormat::Tiff => {
                                    info!(
                                        "Detected TIFF format for {}, applying {}MB limit",
                                        image_path,
                                        config.format_limits.tiff_max / (1024 * 1024)
                                    );
                                    config.format_limits.tiff_max
                                }
                                _ => {
                                    info!("Detected unsupported format {format:?} for {image_path}, using general limit");
                                    config.max_file_size
                                }
                            };

                            if file_size > format_limit {
                                return Err(anyhow::anyhow!(
                                    "Image file too large for {:?} format: {} bytes (maximum allowed: {} bytes)",
                                    format, file_size, format_limit
                                ));
                            }

                            // Estimate memory usage for processing
                            let estimated_memory_mb = estimate_memory_usage(file_size, &format);
                            info!(
                                "Estimated memory usage for {image_path}: {estimated_memory_mb}MB"
                            );

                            // Check if estimated memory usage exceeds safe limits
                            let max_memory_mb = std::env::var("OCR_MEMORY_LIMIT_MB")
                                .unwrap_or_else(|_| "80".to_string())
                                .parse::<f64>()
                                .unwrap_or(80.0); // 80MB memory limit for OCR processing (conservative for Fly.io 512MB VMs)
                            if estimated_memory_mb > max_memory_mb {
                                return Err(anyhow::anyhow!(
                                    "Estimated memory usage too high: {}MB (maximum allowed: {}MB). File would cause out-of-memory errors.",
                                    estimated_memory_mb, max_memory_mb
                                ));
                            }

                            Ok(())
                        }
                        Err(_) => {
                            // Could not determine format, use general limit
                            info!("Could not determine image format for {image_path}, using general size limit");
                            if file_size > config.max_file_size {
                                return Err(anyhow::anyhow!(
                                    "Image file too large: {} bytes (maximum allowed: {} bytes)",
                                    file_size,
                                    config.max_file_size
                                ));
                            }
                            Ok(())
                        }
                    }
                }
                _ => {
                    // Could not read enough bytes, use general limit
                    info!("Could not read enough bytes for format detection from {image_path}, using general size limit");
                    if file_size > config.max_file_size {
                        return Err(anyhow::anyhow!(
                            "Image file too large: {} bytes (maximum allowed: {} bytes)",
                            file_size,
                            config.max_file_size
                        ));
                    }
                    Ok(())
                }
            }
        }
        Err(e) => Err(anyhow::anyhow!(
            "Cannot open image file for validation: {} - {}",
            image_path,
            e
        )),
    }
}

/// Estimate memory usage for image processing based on file size and format
///
/// Calculates expected memory consumption during image decompression and OCR processing.
/// Used for pre-processing validation to prevent out-of-memory errors.
///
/// # Arguments
///
/// * `file_size` - Size of the image file in bytes
/// * `format` - Detected image format
///
/// # Returns
///
/// Returns estimated memory usage in megabytes (MB)
///
/// # Memory Factors by Format
///
/// | Format | Factor | Reason |
/// |--------|--------|--------|
/// | PNG    | 3.0x   | Lossless decompression expands compressed data |
/// | JPEG   | 2.5x   | Lossy decompression with working buffers |
/// | BMP    | 1.2x   | Mostly uncompressed, minimal expansion |
/// | TIFF   | 4.0x   | Complex format with layers and metadata |
///
/// # Examples
///
/// ```rust
/// use just_ingredients::ocr::estimate_memory_usage;
/// use image::ImageFormat;
///
/// // 1MB PNG file
/// let memory_mb = estimate_memory_usage(1024 * 1024, &ImageFormat::Png);
/// assert_eq!(memory_mb, 3.0); // 3MB estimated usage
///
/// // 2MB JPEG file
/// let memory_mb = estimate_memory_usage(2 * 1024 * 1024, &ImageFormat::Jpeg);
/// assert_eq!(memory_mb, 5.0); // 5MB estimated usage
/// ```
///
/// # Usage in Validation
///
/// Used by `validate_image_with_format_limits()` to ensure sufficient memory
/// is available before attempting image processing and OCR operations.
///
/// # Accuracy
///
/// Estimates are conservative and may overestimate actual usage.
/// Better to reject potentially problematic files than risk OOM errors.
pub fn estimate_memory_usage(file_size: u64, format: &image::ImageFormat) -> f64 {
    // Convert file size to MB. Precision loss is acceptable for image files
    // as they rarely exceed sizes where f64 precision becomes an issue.
    #[allow(clippy::cast_precision_loss)]
    let file_size_mb = file_size as f64 / (1024.0 * 1024.0);

    // Memory estimation factors based on format characteristics
    let memory_factor = match format {
        image::ImageFormat::Png => 3.0, // PNG decompression can use 2-4x file size
        image::ImageFormat::Jpeg => 2.5, // JPEG decompression uses ~2-3x
        image::ImageFormat::Bmp => 1.2, // BMP is mostly uncompressed
        image::ImageFormat::Tiff => 4.0, // TIFF can be complex with layers
        _ => 3.0,                       // Default estimation
    };

    file_size_mb * memory_factor
}

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
/// Returns `Result<String, OcrError>` containing the extracted text or an error
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
/// let text = extract_text_from_image("/path/to/ingredients.jpg", &config, &instance_manager, &circuit_breaker).await?;
/// println!("Extracted text: {}", text);
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
/// Returns the extracted text as a `String` on success, or an `OcrError` on failure
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
/// let extracted_text = extract_text_from_image(
///     "/path/to/ingredients.jpg",
///     &config,
///     &instance_manager,
///     &circuit_breaker
/// ).await?;
///
/// println!("Extracted text: {}", extracted_text);
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
///     Ok(text) => println!("Success: {}", text),
///     Err(e) => println!("Error: {:?}", e), // May be circuit breaker error
/// }
/// # Ok(())
/// # }
/// ```
///
/// Correct common OCR errors with fraction characters
///
/// OCR engines often misread fraction symbols (/) as other characters.
/// This function attempts to correct the most common fraction OCR errors.
fn correct_ocr_fraction_errors(text: &str) -> String {
    let mut corrected = text.to_string();

    // Common OCR fraction errors and their corrections
    let corrections = [
        // 1/2 misreads
        ("Ye", "1/2"),

        // 1/4 misreads
        ("%", "1/4"),
    ];

    // Apply corrections with word boundaries to avoid false positives
    for (ocr_error, correction) in corrections.iter() {
        // For single characters, don't require word boundaries
        // For multi-character strings, use word boundaries
        let pattern = if ocr_error.len() == 1 {
            regex::escape(ocr_error)
        } else {
            format!(r"\b{}\b", regex::escape(ocr_error))
        };

        if let Ok(regex) = regex::Regex::new(&pattern) {
            let before = corrected.clone();
            corrected = regex.replace_all(&corrected, *correction).to_string();
            if before != corrected {
                tracing::debug!("OCR correction: '{}' -> '{}' in text: '{}'", ocr_error, correction, before);
            }
        }
    }

    corrected
}

pub async fn extract_text_from_image(
    image_path: &str,
    config: &crate::ocr_config::OcrConfig,
    instance_manager: &crate::instance_manager::OcrInstanceManager,
    circuit_breaker: &crate::circuit_breaker::CircuitBreaker,
) -> Result<String, crate::ocr_errors::OcrError> {
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
            Ok((text, ocr_duration)) => {
                let total_duration = start_time.elapsed();
                let total_ms = total_duration.as_millis();

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

                info!("OCR extraction completed successfully on attempt {} in {}ms. Extracted {} characters of text",
                      attempt, total_ms, text.len());
                return Ok(text);
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
async fn perform_ocr_extraction(
    image_path: &str,
    config: &crate::ocr_config::OcrConfig,
    instance_manager: &crate::instance_manager::OcrInstanceManager,
) -> Result<(String, std::time::Duration), crate::ocr_errors::OcrError> {
    // Start timing the actual OCR processing
    let ocr_start_time = std::time::Instant::now();

    // Create a timeout for the operation
    let timeout_duration = tokio::time::Duration::from_secs(config.recovery.operation_timeout_secs);

    let result = tokio::time::timeout(timeout_duration, async {
        // Get or create OCR instance from the manager
        let instance = instance_manager
            .get_instance(config)
            .map_err(|e| crate::ocr_errors::OcrError::Initialization(e.to_string()))?;

        // Perform OCR processing with the reused instance
        let extracted_text = {
            let mut tess = instance
                .lock()
                .expect("Failed to acquire Tesseract instance lock");
            // Set the image for OCR processing
            tess.set_image(image_path).map_err(|e| {
                crate::ocr_errors::OcrError::ImageLoad(format!("Failed to load image for OCR: {e}"))
            })?;

            // Extract text from the image
            tess.get_utf8_text().map_err(|e| {
                crate::ocr_errors::OcrError::Extraction(format!(
                    "Failed to extract text from image: {e}"
                ))
            })?
        };

        // Clean up the extracted text (remove extra whitespace and empty lines)
        let cleaned_text = extracted_text
            .trim()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<&str>>()
            .join("\n");

        // Apply OCR error correction for common fraction misreads
        let corrected_text = correct_ocr_fraction_errors(&cleaned_text);

        Ok(corrected_text)
    })
    .await;

    let ocr_duration = ocr_start_time.elapsed();
    let ocr_ms = ocr_duration.as_millis();

    match result {
        Ok(Ok(text)) => {
            info!(
                "OCR processing completed in {}ms, extracted {} characters",
                ocr_ms,
                text.len()
            );
            Ok((text, ocr_duration))
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

/// Calculate retry delay with exponential backoff
///
/// Implements exponential backoff with jitter to prevent thundering herd problems.
/// Delay increases exponentially with each retry attempt, with random jitter added
/// to distribute retry attempts over time.
///
/// ## Algorithm Overview
///
/// The function calculates retry delays using this formula:
/// ```text
/// delay = min(base_delay * (2^(attempt-1)), max_delay)
/// jitter = random(0, delay/4)
/// final_delay = delay + jitter
/// ```
///
/// ## Exponential Backoff Logic
///
/// - **Base Delay**: Starting delay for first retry (typically 1000ms)
/// - **Exponential Growth**: Delay doubles with each retry attempt
/// - **Maximum Cap**: Prevents excessively long delays
/// - **Jitter Addition**: Random component prevents synchronized retries
///
/// ## Delay Progression Examples
///
/// | Attempt | Base Delay | Exponential | Jitter Range | Final Delay Range |
/// |---------|------------|-------------|--------------|-------------------|
/// | 1       | 1000ms     | 1000ms      | 0-250ms      | 1000-1250ms      |
/// | 2       | 1000ms     | 2000ms      | 0-500ms      | 2000-2500ms      |
/// | 3       | 1000ms     | 4000ms      | 0-1000ms     | 4000-5000ms      |
/// | 4       | 1000ms     | 8000ms      | 0-2000ms     | 8000-10000ms     |
/// | 5+      | 1000ms     | 10000ms*    | 0-2500ms     | 10000-12500ms    |
///
/// *Capped at max_retry_delay_ms
///
/// ## Jitter Implementation
///
/// Jitter is calculated as a random value between 0 and delay/4:
/// - **Purpose**: Distribute retry attempts over time
/// - **Range**: 0 to 25% of the base delay
/// - **Randomness**: Uses `rand::random()` for uniform distribution
/// - **Thread Safety**: Each call generates independent random values
///
/// ## Configuration Parameters
///
/// - `base_retry_delay_ms`: Base delay for first retry (default: 1000ms)
/// - `max_retry_delay_ms`: Maximum delay cap (default: 10000ms)
/// - `attempt`: Current retry attempt number (1-based)
///
/// ## Benefits
///
/// - **Load Distribution**: Prevents server overload during failures
/// - **Thundering Herd Prevention**: Jitter distributes retry attempts
/// - **Configurable**: Adjustable for different environments
/// - **Predictable**: Exponential growth with known bounds
/// - **Resource Protection**: Gives failing services time to recover
///
/// ## Usage in Retry Logic
///
/// ```rust
/// use just_ingredients::ocr_config::RecoveryConfig;
/// use just_ingredients::ocr::calculate_retry_delay;
///
/// let config = RecoveryConfig::default();
/// // Simulate retry logic
/// let mut results = Vec::new();
/// for attempt in 1..=3 {
///     let delay = calculate_retry_delay(attempt, &config);
///     results.push(delay);
///     // In real code: tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
/// }
/// // Results will be different due to jitter, but within expected ranges
/// assert!(results[0] >= 1000 && results[0] <= 1250); // ~1000ms + jitter
/// assert!(results[1] >= 2000 && results[1] <= 2500); // ~2000ms + jitter
/// assert!(results[2] >= 4000 && results[2] <= 5000); // ~4000ms + jitter
/// ```
///
/// ## Performance Considerations
///
/// - **Computation**: Minimal CPU overhead (bit operations + random generation)
/// - **Memory**: No allocations, uses only primitive types
/// - **Thread Safety**: Safe for concurrent use
/// - **Predictability**: Deterministic exponential growth with random jitter
///
/// # Arguments
///
/// * `attempt` - Current retry attempt number (1-based, first retry = 1)
/// * `recovery` - Recovery configuration with delay settings
///
/// # Returns
///
/// Returns delay in milliseconds before next retry attempt
///
/// # Examples
///
/// ```rust
/// use just_ingredients::ocr::{calculate_retry_delay, RecoveryConfig};
///
/// let config = RecoveryConfig::default();
/// // First retry: ~1000-1250ms (1000ms + jitter)
/// let delay1 = calculate_retry_delay(1, &config);
/// // Second retry: ~2000-2500ms (2000ms + jitter)
/// let delay2 = calculate_retry_delay(2, &config);
/// // Third retry: ~4000-5000ms (4000ms + jitter)
/// let delay3 = calculate_retry_delay(3, &config);
/// ```
/// - **Capped**: Prevents excessively long delays
pub fn calculate_retry_delay(attempt: u32, recovery: &crate::ocr_config::RecoveryConfig) -> u64 {
    // Calculate exponential backoff with minimal precision loss
    // For retry delays, precision loss is acceptable as delays are typically small
    #[allow(clippy::cast_precision_loss)]
    let base_delay = recovery.base_retry_delay_ms as f64;

    #[allow(clippy::cast_precision_loss)]
    let exponential_delay = base_delay * (2.0_f64).powf((attempt - 1) as f64);

    #[allow(clippy::cast_precision_loss)]
    let delay = exponential_delay.min(recovery.max_retry_delay_ms as f64) as u64;

    // Add some jitter to prevent thundering herd
    let jitter = (rand::random::<u64>() % (delay / 4)) as u64;
    delay + jitter
}

/// Validate if an image file is supported for OCR processing using `image::guess_format`
///
/// Performs comprehensive validation including:
/// 1. File existence and accessibility checks
/// 2. Format detection using magic bytes
/// 3. File size validation against format-specific limits
/// 4. Memory usage estimation
///
/// # Arguments
///
/// * `file_path` - Path to the image file to validate
/// * `config` - OCR configuration with size limits and buffer settings
///
/// # Returns
///
/// Returns `true` if the image format is supported and passes all validation checks
///
/// # Supported Formats
///
/// | Format | Max Size | Description |
/// |--------|----------|-------------|
/// | PNG    | 15MB     | Lossless compression, best for text |
/// | JPEG   | 10MB     | Lossy compression, good quality/size balance |
/// | BMP    | 5MB      | Uncompressed, fast but large files |
/// | TIFF   | 20MB     | Multi-page support, high quality |
///
/// # Examples
///
/// ```rust,no_run
/// use just_ingredients::ocr::{is_supported_image_format, OcrConfig};
///
/// let config = OcrConfig::default();
/// if is_supported_image_format("/path/to/image.jpg", &config) {
///     println!("Image is supported for OCR processing");
/// } else {
///     println!("Image format not supported or file too large");
/// }
/// ```
///
/// # Validation Process
///
/// 1. Checks if file exists and is readable
/// 2. Reads first 32 bytes (configurable) for format detection
/// 3. Uses `image::guess_format()` to identify format
/// 4. Validates file size against format-specific limits
/// 5. Estimates memory usage for processing
///
/// # Performance
///
/// - Fast format detection using only file header
/// - Minimal I/O (only reads format detection buffer)
/// - No full file loading or OCR processing
pub fn is_supported_image_format(file_path: &str, config: &crate::ocr_config::OcrConfig) -> bool {
    // Enhanced validation first (includes size checks)
    if validate_image_with_format_limits(file_path, config).is_err() {
        return false;
    }

    match File::open(file_path) {
        Ok(file) => {
            let mut reader = BufReader::new(file);
            let mut buffer = vec![0; config.buffer_size]; // Pre-allocate buffer for format detection

            match reader.read(&mut buffer) {
                Ok(bytes_read) if bytes_read >= config.min_format_bytes => {
                    // Truncate buffer to actual bytes read
                    buffer.truncate(bytes_read);

                    info!("Read {bytes_read} bytes from file {file_path} for format detection");

                    match image::guess_format(&buffer) {
                        Ok(format) => {
                            // Tesseract supports: PNG, JPEG/JPG, BMP, TIFF
                            let supported = matches!(
                                format,
                                image::ImageFormat::Png
                                    | image::ImageFormat::Jpeg
                                    | image::ImageFormat::Bmp
                                    | image::ImageFormat::Tiff
                            );

                            if supported {
                                info!("Detected supported image format: {format:?} for file: {file_path}");
                            } else {
                                info!("Detected unsupported image format: {format:?} for file: {file_path}");
                            }

                            supported
                        }
                        Err(e) => {
                            info!("Could not determine image format for file: {file_path} - {e}");
                            false
                        }
                    }
                }
                Ok(bytes_read) => {
                    info!("Could not read enough bytes to determine image format for file: {} (read {} bytes, need at least {})", file_path, bytes_read, config.min_format_bytes);
                    false
                }
                Err(e) => {
                    info!("Error reading image file for format detection: {file_path} - {e}");
                    false
                }
            }
        }
        Err(e) => {
            info!("Could not open image file for format detection: {file_path} - {e}");
            false
        }
    }
}
