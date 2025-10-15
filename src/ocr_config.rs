//! # OCR Configuration Module
//!
//! This module defines configuration structures for OCR processing,
//! including recovery settings, format limits, and processing parameters.

// Constants for OCR configuration
pub const DEFAULT_LANGUAGES: &str = "eng+fra";
pub const FORMAT_DETECTION_BUFFER_SIZE: usize = 32;
pub const MIN_FORMAT_BYTES: usize = 8;
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB limit for image files

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
}

impl RecoveryConfig {
    /// Validate recovery configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.max_retries == 0 {
            return Err("[CONFIG_RECOVERY] max_retries must be greater than 0".to_string());
        }
        if self.base_retry_delay_ms == 0 {
            return Err("[CONFIG_RECOVERY] base_retry_delay_ms must be greater than 0".to_string());
        }
        if self.max_retry_delay_ms < self.base_retry_delay_ms {
            return Err(format!(
                "[CONFIG_RECOVERY] max_retry_delay_ms ({}) must be >= base_retry_delay_ms ({})",
                self.max_retry_delay_ms, self.base_retry_delay_ms
            ));
        }
        if self.operation_timeout_secs == 0 {
            return Err("[CONFIG_RECOVERY] operation_timeout_secs must be greater than 0".to_string());
        }
        if self.circuit_breaker_threshold == 0 {
            return Err("[CONFIG_RECOVERY] circuit_breaker_threshold must be greater than 0".to_string());
        }
        if self.circuit_breaker_reset_secs == 0 {
            return Err("[CONFIG_RECOVERY] circuit_breaker_reset_secs must be greater than 0".to_string());
        }
        Ok(())
    }
}

impl FormatSizeLimits {
    /// Validate format size limits
    pub fn validate(&self) -> Result<(), String> {
        if self.png_max == 0 {
            return Err("[CONFIG_FORMAT] png_max must be greater than 0".to_string());
        }
        if self.jpeg_max == 0 {
            return Err("[CONFIG_FORMAT] jpeg_max must be greater than 0".to_string());
        }
        if self.bmp_max == 0 {
            return Err("[CONFIG_FORMAT] bmp_max must be greater than 0".to_string());
        }
        if self.tiff_max == 0 {
            return Err("[CONFIG_FORMAT] tiff_max must be greater than 0".to_string());
        }
        if self.min_quick_reject == 0 {
            return Err("[CONFIG_FORMAT] min_quick_reject must be greater than 0".to_string());
        }

        // Ensure format limits are reasonable compared to each other
        if self.bmp_max > self.png_max {
            return Err(format!(
                "[CONFIG_FORMAT] bmp_max ({}) should not exceed png_max ({})",
                self.bmp_max, self.png_max
            ));
        }
        if self.jpeg_max > self.png_max {
            return Err(format!(
                "[CONFIG_FORMAT] jpeg_max ({}) should not exceed png_max ({})",
                self.jpeg_max, self.png_max
            ));
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
        }
    }
}

impl OcrConfig {
    /// Validate OCR configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        // Validate languages string
        if self.languages.trim().is_empty() {
            return Err("[CONFIG_OCR] languages cannot be empty".to_string());
        }

        // Validate buffer sizes
        if self.buffer_size == 0 {
            return Err("[CONFIG_OCR] buffer_size must be greater than 0".to_string());
        }
        if self.min_format_bytes == 0 {
            return Err("[CONFIG_OCR] min_format_bytes must be greater than 0".to_string());
        }
        if self.min_format_bytes > self.buffer_size {
            return Err(format!(
                "[CONFIG_OCR] min_format_bytes ({}) cannot exceed buffer_size ({})",
                self.min_format_bytes, self.buffer_size
            ));
        }

        // Validate file size limits
        if self.max_file_size == 0 {
            return Err("[CONFIG_OCR] max_file_size must be greater than 0".to_string());
        }

        // Validate nested configurations
        self.format_limits.validate()?;
        self.recovery.validate()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
}
