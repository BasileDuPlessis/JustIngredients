//! Image validation utilities for OCR processing
//!
//! This module provides comprehensive image validation functionality including
//! path validation, format detection, size limits, and memory usage estimation.

use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, Read};
use tracing::info;

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

/// Check if an image file format is supported for OCR processing
///
/// Performs fast format detection using only the file header without loading
/// the entire image. This function is optimized for performance and minimal I/O.
///
/// # Arguments
///
/// * `file_path` - Path to the image file to check
/// * `config` - OCR configuration containing validation parameters
///
/// # Returns
///
/// Returns `true` if the image format is supported, `false` otherwise
///
/// # Supported Formats
///
/// - PNG (Portable Network Graphics)
/// - JPEG/JPG (Joint Photographic Experts Group)
/// - BMP (Bitmap)
/// - TIFF/TIF (Tagged Image File Format)
///
/// # Performance Characteristics
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
