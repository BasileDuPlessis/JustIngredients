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

/// Page Segmentation Mode for Tesseract OCR
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PageSegMode {
    /// Orientation and script detection (OSD) only
    OsdOnly = 0,
    /// Automatic page segmentation with OSD
    AutoOsd = 1,
    /// Automatic page segmentation, no OSD
    AutoNoOsd = 2,
    /// Fully automatic page segmentation
    #[default]
    Auto = 3,
    /// Assume a single column of text
    SingleColumn = 4,
    /// Assume a single uniform block of vertically aligned text
    SingleBlockVert = 5,
    /// Assume a single uniform block of text
    SingleBlock = 6,
    /// Treat the image as a single text line
    SingleLine = 7,
    /// Treat the image as a single word
    SingleWord = 8,
    /// Treat the image as a single word in a circle
    WordInCircle = 9,
    /// Treat the image as a single character
    SingleChar = 10,
    /// Find as much text as possible in no particular order
    SparseText = 11,
    /// Sparse text with OSD
    SparseTextOsd = 12,
    /// Treat the image as a single text line, bypassing hacks that are Tesseract-specific
    RawLine = 13,
}

impl PageSegMode {
    /// Convert PSM mode to string value for Tesseract
    pub fn as_str(&self) -> &'static str {
        match self {
            PageSegMode::OsdOnly => "0",
            PageSegMode::AutoOsd => "1",
            PageSegMode::AutoNoOsd => "2",
            PageSegMode::Auto => "3",
            PageSegMode::SingleColumn => "4",
            PageSegMode::SingleBlockVert => "5",
            PageSegMode::SingleBlock => "6",
            PageSegMode::SingleLine => "7",
            PageSegMode::SingleWord => "8",
            PageSegMode::WordInCircle => "9",
            PageSegMode::SingleChar => "10",
            PageSegMode::SparseText => "11",
            PageSegMode::SparseTextOsd => "12",
            PageSegMode::RawLine => "13",
        }
    }
}

/// Tesseract model type for different accuracy/speed trade-offs
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ModelType {
    /// Fast model (tessdata_fast) - faster processing, lower accuracy
    #[default]
    Fast,
    /// Best model (tessdata_best) - slower processing, higher accuracy
    Best,
}

impl ModelType {
    /// Get the tessdata directory name for this model type
    pub fn tessdata_dir(&self) -> &'static str {
        match self {
            ModelType::Fast => "tessdata_fast",
            ModelType::Best => "tessdata_best",
        }
    }

    /// Get the expected accuracy improvement over Fast model
    pub fn expected_accuracy_improvement(&self) -> f32 {
        match self {
            ModelType::Fast => 0.0,
            ModelType::Best => 0.05, // 5% improvement expected
        }
    }
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

/// Configuration structure for OCR processing
#[derive(Debug, Clone)]
pub struct OcrConfig {
    /// OCR language codes (e.g., "eng", "eng+fra", "deu")
    pub languages: String,
    /// Tesseract model type (Fast vs Best accuracy)
    pub model_type: ModelType,
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
    /// Default page segmentation mode for OCR
    pub psm_mode: PageSegMode,
    /// Path to custom user words file for improved recognition
    pub user_words_file: Option<String>,
    /// Path to custom user patterns file for improved recognition
    pub user_patterns_file: Option<String>,
    /// Character whitelist to restrict OCR output to recipe-relevant characters
    pub character_whitelist: Option<String>,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            languages: DEFAULT_LANGUAGES.to_string(),
            model_type: ModelType::default(),
            buffer_size: FORMAT_DETECTION_BUFFER_SIZE,
            min_format_bytes: MIN_FORMAT_BYTES,
            max_file_size: MAX_FILE_SIZE,
            format_limits: FormatSizeLimits::default(),
            recovery: RecoveryConfig::default(),
            psm_mode: PageSegMode::default(),
            user_words_file: Some("config/user_words.txt".to_string()),
            user_patterns_file: Some("config/user_patterns.txt".to_string()),
            character_whitelist: Some("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyzÀÂÄÉÈÊËÏÎÔÖÙÛÜŸàâäéèêëïîôöùûüÿ¼½¾⅓⅔⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞/.,-() ".to_string()),
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
    fn test_model_type_enum_values() {
        // Test ModelType enum values and methods
        assert_eq!(ModelType::Fast.tessdata_dir(), "tessdata_fast");
        assert_eq!(ModelType::Best.tessdata_dir(), "tessdata_best");

        assert_eq!(ModelType::Fast.expected_accuracy_improvement(), 0.0);
        assert_eq!(ModelType::Best.expected_accuracy_improvement(), 0.05);
    }

    #[test]
    fn test_model_type_default() {
        // Test that default is Fast for backward compatibility
        assert_eq!(ModelType::default(), ModelType::Fast);
    }

    #[test]
    fn test_ocr_config_with_model_type() {
        // Test OcrConfig with different model types
        let fast_config = OcrConfig {
            model_type: ModelType::Fast,
            ..Default::default()
        };
        assert_eq!(fast_config.model_type, ModelType::Fast);

        let best_config = OcrConfig {
            model_type: ModelType::Best,
            ..Default::default()
        };
        assert_eq!(best_config.model_type, ModelType::Best);

        // Test that default config uses Fast model
        let default_config = OcrConfig::default();
        assert_eq!(default_config.model_type, ModelType::Fast);
    }

    #[test]
    fn test_ocr_config_user_words_file() {
        // Test OcrConfig with custom user words file
        let config_with_custom_words = OcrConfig {
            user_words_file: Some("custom/path/to/words.txt".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config_with_custom_words.user_words_file,
            Some("custom/path/to/words.txt".to_string())
        );

        // Test default config includes user words file
        let default_config = OcrConfig::default();
        assert_eq!(
            default_config.user_words_file,
            Some("config/user_words.txt".to_string())
        );

        // Test config without user words file
        let config_without_words = OcrConfig {
            user_words_file: None,
            ..Default::default()
        };
        assert_eq!(config_without_words.user_words_file, None);
    }

    #[test]
    fn test_ocr_config_user_patterns_file() {
        // Test OcrConfig with custom user patterns file
        let config_with_custom_patterns = OcrConfig {
            user_patterns_file: Some("custom/path/to/patterns.txt".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config_with_custom_patterns.user_patterns_file,
            Some("custom/path/to/patterns.txt".to_string())
        );

        // Test default config includes user patterns file
        let default_config = OcrConfig::default();
        assert_eq!(
            default_config.user_patterns_file,
            Some("config/user_patterns.txt".to_string())
        );

        // Test config without user patterns file
        let config_without_patterns = OcrConfig {
            user_patterns_file: None,
            ..Default::default()
        };
        assert_eq!(config_without_patterns.user_patterns_file, None);
    }

    #[test]
    fn test_ocr_config_character_whitelist() {
        // Test OcrConfig with custom character whitelist
        let custom_whitelist = "0123456789ABCDEF".to_string();
        let config_with_custom_whitelist = OcrConfig {
            character_whitelist: Some(custom_whitelist.clone()),
            ..Default::default()
        };
        assert_eq!(
            config_with_custom_whitelist.character_whitelist,
            Some(custom_whitelist)
        );

        // Test default config includes character whitelist
        let default_config = OcrConfig::default();
        assert!(default_config.character_whitelist.is_some());
        let default_whitelist = default_config
            .character_whitelist
            .as_ref()
            .expect("character_whitelist is Some as asserted above");
        // Should contain basic alphanumeric characters
        assert!(default_whitelist.contains("0123456789"));
        assert!(default_whitelist.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
        assert!(default_whitelist.contains("abcdefghijklmnopqrstuvwxyz"));
        // Should contain accented characters for French
        assert!(default_whitelist.contains("ÀÂÄÉÈÊË"));
        // Should contain fractions
        assert!(default_whitelist.contains("¼½¾⅓⅔⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞"));
        // Should contain common punctuation
        assert!(default_whitelist.contains(".,-() "));

        // Test config without character whitelist
        let config_without_whitelist = OcrConfig {
            character_whitelist: None,
            ..Default::default()
        };
        assert_eq!(config_without_whitelist.character_whitelist, None);
    }
}
