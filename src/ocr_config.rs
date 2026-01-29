//! # OCR Configuration Module
//!
//! This module defines configuration structures for OCR processing,
//! including recovery settings, format limits, and processing parameters.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

// Constants for OCR configuration
pub const DEFAULT_LANGUAGES: &str = "eng+fra";
pub const FORMAT_DETECTION_BUFFER_SIZE: usize = 32;
pub const MIN_FORMAT_BYTES: usize = 8;
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB limit for image files

/// Configuration structure for PPOCR (PaddlePaddle OCR) processing
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PpocrConfig {
    /// Whether PPOCR is enabled
    pub enabled: bool,
    /// Path to Python executable for running PPOCR
    pub python_path: PathBuf,
    /// Directory for caching PPOCR models
    pub model_cache_dir: PathBuf,
    /// Language code for OCR (e.g., "en", "fr", "ch")
    pub language: String,
    /// Whether to use GPU acceleration
    pub use_gpu: bool,
    /// Timeout for PPOCR operations in seconds
    pub timeout_seconds: u64,
}

impl Default for PpocrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            python_path: PathBuf::from("python3"),
            model_cache_dir: PathBuf::from("./ppocr_models"),
            language: "en".to_string(),
            use_gpu: false,
            timeout_seconds: 30,
        }
    }
}

impl PpocrConfig {
    /// Validate PPOCR configuration parameters
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        // Validate python_path exists and is not empty
        if self.python_path.as_os_str().is_empty() {
            return Err(crate::errors::AppError::Config(
                "python_path cannot be empty".to_string(),
            ));
        }

        // Validate language is not empty
        if self.language.trim().is_empty() {
            return Err(crate::errors::AppError::Config(
                "language cannot be empty".to_string(),
            ));
        }

        // Validate timeout is reasonable
        if self.timeout_seconds == 0 {
            return Err(crate::errors::AppError::Config(
                "timeout_seconds must be greater than 0".to_string(),
            ));
        }

        // Validate timeout is not too long (max 5 minutes)
        if self.timeout_seconds > 300 {
            return Err(crate::errors::AppError::Config(
                "timeout_seconds cannot exceed 300 seconds (5 minutes)".to_string(),
            ));
        }

        Ok(())
    }
}

/// Recovery configuration for error handling
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries in milliseconds
    pub base_retry_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_retry_delay_ms: u64,
    /// Timeout for OCR operations in seconds
    pub operation_timeout_secs: u64,
    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker reset timeout in seconds
    pub circuit_breaker_reset_secs: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_retry_delay_ms: 1000,  // 1 second
            max_retry_delay_ms: 10000,  // 10 seconds
            operation_timeout_secs: 30, // 30 seconds
            circuit_breaker_threshold: 5,
            circuit_breaker_reset_secs: 60, // 1 minute
        }
    }
}

/// Format-specific file size limits for different image formats
#[derive(Debug, Clone)]
pub struct FormatSizeLimits {
    /// PNG format limit (higher due to better compression)
    pub png_max: u64,
    /// JPEG format limit (moderate due to lossy compression)
    pub jpeg_max: u64,
    /// BMP format limit (lower due to uncompressed nature)
    pub bmp_max: u64,
    /// TIFF format limit (can be large, multi-page support)
    pub tiff_max: u64,
    /// Minimum file size threshold for quick rejection
    pub min_quick_reject: u64,
}

impl Default for FormatSizeLimits {
    fn default() -> Self {
        Self {
            png_max: 15 * 1024 * 1024,          // 15MB for PNG
            jpeg_max: 10 * 1024 * 1024,         // 10MB for JPEG
            bmp_max: 5 * 1024 * 1024,           // 5MB for BMP
            tiff_max: 20 * 1024 * 1024,         // 20MB for TIFF
            min_quick_reject: 50 * 1024 * 1024, // 50MB quick reject
        }
    }
}

/// Configuration structure for OCR processing
#[derive(Debug, Clone)]
pub struct OcrConfig {
    /// OCR language codes (e.g., "eng", "eng+fra", "deu")
    pub languages: String,
    /// Buffer size for format detection in bytes
    pub buffer_size: usize,
    /// Minimum bytes required for format detection
    pub min_format_bytes: usize,
    /// Maximum allowed file size in bytes (general limit)
    pub max_file_size: u64,
    /// Format-specific size limits
    pub format_limits: FormatSizeLimits,
    /// Recovery and error handling configuration
    pub recovery: RecoveryConfig,
    /// PPOCR configuration (optional)
    pub ppocr: Option<PpocrConfig>,
}

impl RecoveryConfig {
    /// Validate recovery configuration parameters
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        if self.max_retries == 0 {
            return Err(crate::errors::AppError::Config(
                "max_retries must be greater than 0".to_string(),
            ));
        }
        if self.base_retry_delay_ms == 0 {
            return Err(crate::errors::AppError::Config(
                "base_retry_delay_ms must be greater than 0".to_string(),
            ));
        }
        if self.max_retry_delay_ms < self.base_retry_delay_ms {
            return Err(crate::errors::AppError::Config(format!(
                "max_retry_delay_ms ({}) must be >= base_retry_delay_ms ({})",
                self.max_retry_delay_ms, self.base_retry_delay_ms
            )));
        }
        if self.operation_timeout_secs == 0 {
            return Err(crate::errors::AppError::Config(
                "operation_timeout_secs must be greater than 0".to_string(),
            ));
        }
        if self.circuit_breaker_threshold == 0 {
            return Err(crate::errors::AppError::Config(
                "circuit_breaker_threshold must be greater than 0".to_string(),
            ));
        }
        if self.circuit_breaker_reset_secs == 0 {
            return Err(crate::errors::AppError::Config(
                "circuit_breaker_reset_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl FormatSizeLimits {
    /// Validate format size limits
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        if self.png_max == 0 {
            return Err(crate::errors::AppError::Config(
                "png_max must be greater than 0".to_string(),
            ));
        }
        if self.jpeg_max == 0 {
            return Err(crate::errors::AppError::Config(
                "jpeg_max must be greater than 0".to_string(),
            ));
        }
        if self.bmp_max == 0 {
            return Err(crate::errors::AppError::Config(
                "bmp_max must be greater than 0".to_string(),
            ));
        }
        if self.tiff_max == 0 {
            return Err(crate::errors::AppError::Config(
                "tiff_max must be greater than 0".to_string(),
            ));
        }
        if self.min_quick_reject == 0 {
            return Err(crate::errors::AppError::Config(
                "min_quick_reject must be greater than 0".to_string(),
            ));
        }

        // Ensure format limits are reasonable compared to each other
        if self.bmp_max > self.png_max {
            return Err(crate::errors::AppError::Config(format!(
                "bmp_max ({}) should not exceed png_max ({})",
                self.bmp_max, self.png_max
            )));
        }
        if self.jpeg_max > self.png_max {
            return Err(crate::errors::AppError::Config(format!(
                "jpeg_max ({}) should not exceed png_max ({})",
                self.jpeg_max, self.png_max
            )));
        }

        Ok(())
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            languages: DEFAULT_LANGUAGES.to_string(),
            buffer_size: FORMAT_DETECTION_BUFFER_SIZE,
            min_format_bytes: MIN_FORMAT_BYTES,
            max_file_size: MAX_FILE_SIZE,
            format_limits: FormatSizeLimits::default(),
            recovery: RecoveryConfig::default(),
            ppocr: None,
        }
    }
}

impl OcrConfig {
    /// Validate OCR configuration parameters
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        // Validate languages string
        if self.languages.trim().is_empty() {
            return Err(crate::errors::AppError::Config(
                "languages cannot be empty".to_string(),
            ));
        }

        // Validate buffer sizes
        if self.buffer_size == 0 {
            return Err(crate::errors::AppError::Config(
                "buffer_size must be greater than 0".to_string(),
            ));
        }
        if self.min_format_bytes == 0 {
            return Err(crate::errors::AppError::Config(
                "min_format_bytes must be greater than 0".to_string(),
            ));
        }
        if self.min_format_bytes > self.buffer_size {
            return Err(crate::errors::AppError::Config(format!(
                "min_format_bytes ({}) cannot exceed buffer_size ({})",
                self.min_format_bytes, self.buffer_size
            )));
        }

        // Validate file size limits
        if self.max_file_size == 0 {
            return Err(crate::errors::AppError::Config(
                "max_file_size must be greater than 0".to_string(),
            ));
        }

        // Validate nested configurations
        self.format_limits.validate()?;
        self.recovery.validate()?;

        // Validate PPOCR config if present
        if let Some(ref ppocr_config) = self.ppocr {
            ppocr_config.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(unused_assignments)]
    fn test_recovery_config_validation() {
        let mut config = RecoveryConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test invalid max_retries
        config.max_retries = 0;
        assert!(config.validate().is_err());
        config.max_retries = 3;

        // Test invalid base_retry_delay_ms
        config.base_retry_delay_ms = 0;
        assert!(config.validate().is_err());
        config.base_retry_delay_ms = 1000;

        // Test invalid max_retry_delay_ms < base_retry_delay_ms
        config.max_retry_delay_ms = 500;
        assert!(config.validate().is_err());
        config.max_retry_delay_ms = 10000;

        // Test invalid operation_timeout_secs
        config.operation_timeout_secs = 0;
        assert!(config.validate().is_err());
        config.operation_timeout_secs = 30;

        // Test invalid circuit_breaker_threshold
        config.circuit_breaker_threshold = 0;
        assert!(config.validate().is_err());
        config.circuit_breaker_threshold = 5;

        // Test invalid circuit_breaker_reset_secs
        config.circuit_breaker_reset_secs = 0;
        assert!(config.validate().is_err());
        config.circuit_breaker_reset_secs = 60;
    }

    #[test]
    #[allow(unused_assignments)]
    fn test_format_size_limits_validation() {
        let mut config = FormatSizeLimits::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test invalid png_max
        config.png_max = 0;
        assert!(config.validate().is_err());
        config.png_max = 15 * 1024 * 1024;

        // Test invalid jpeg_max
        config.jpeg_max = 0;
        assert!(config.validate().is_err());
        config.jpeg_max = 10 * 1024 * 1024;

        // Test bmp_max > png_max
        config.bmp_max = 20 * 1024 * 1024;
        assert!(config.validate().is_err());
        config.bmp_max = 5 * 1024 * 1024;

        // Test jpeg_max > png_max
        config.jpeg_max = 20 * 1024 * 1024;
        assert!(config.validate().is_err());
        config.jpeg_max = 10 * 1024 * 1024;
    }

    #[test]
    fn test_ocr_config_validation() {
        let mut config = OcrConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test invalid languages
        config.languages = "".to_string();
        assert!(config.validate().is_err());
        config.languages = DEFAULT_LANGUAGES.to_string();

        // Test invalid buffer_size
        config.buffer_size = 0;
        assert!(config.validate().is_err());
        config.buffer_size = FORMAT_DETECTION_BUFFER_SIZE;

        // Test invalid min_format_bytes
        config.min_format_bytes = 0;
        assert!(config.validate().is_err());
        config.min_format_bytes = MIN_FORMAT_BYTES;

        // Test min_format_bytes > buffer_size
        config.min_format_bytes = FORMAT_DETECTION_BUFFER_SIZE + 1;
        assert!(config.validate().is_err());
        config.min_format_bytes = MIN_FORMAT_BYTES;

        // Test invalid max_file_size
        config.max_file_size = 0;
        assert!(config.validate().is_err());
        config.max_file_size = MAX_FILE_SIZE;
    }

    #[test]
    fn test_ppocr_config_validation() {
        let mut config = PpocrConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test invalid python_path
        config.python_path = PathBuf::new();
        assert!(config.validate().is_err());
        config.python_path = PathBuf::from("python3");

        // Test invalid language
        config.language = "".to_string();
        assert!(config.validate().is_err());
        config.language = "en".to_string();

        // Test invalid timeout_seconds
        config.timeout_seconds = 0;
        assert!(config.validate().is_err());
        config.timeout_seconds = 30;

        // Test timeout_seconds too long
        config.timeout_seconds = 400;
        assert!(config.validate().is_err());
        config.timeout_seconds = 30;
    }

    #[test]
    fn test_ocr_config_with_ppocr_validation() {
        let mut config = OcrConfig::default();

        // Config without PPOCR should pass
        assert!(config.validate().is_ok());

        // Add valid PPOCR config
        config.ppocr = Some(PpocrConfig::default());
        assert!(config.validate().is_ok());

        // Add invalid PPOCR config
        if let Some(ref mut ppocr_config) = config.ppocr {
            ppocr_config.timeout_seconds = 0;
        }
        assert!(config.validate().is_err());
    }
}
