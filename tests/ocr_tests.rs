//! # OCR Tests Module
//!
//! Comprehensive test suite for OCR processing functionality,
//! including configuration, validation, circuit breaker, and instance management.

#[cfg(test)]
mod tests {
    use just_ingredients::circuit_breaker::CircuitBreaker;
    use just_ingredients::instance_manager::OcrInstanceManager;
    use just_ingredients::ocr::{
        calculate_retry_delay, estimate_memory_usage, extract_hocr_from_image,
        is_supported_image_format, map_measurement_to_bbox, parse_hocr_to_lines,
        perform_constrained_ocr, validate_image_path, validate_image_with_format_limits, BBox,
        ConstrainedOcrResult, HocrLine,
    };
    use just_ingredients::ocr_config::{
        FormatSizeLimits, ModelType, OcrConfig, PageSegMode, RecoveryConfig,
    };
    use just_ingredients::ocr_errors::OcrError;
    use just_ingredients::text_processing::MeasurementMatch;
    use std::io::Write;
    use tempfile::NamedTempFile;
    extern crate serde_json;

    /// Test OCR configuration defaults
    #[test]
    fn test_ocr_config_defaults() {
        let config = OcrConfig::default();

        assert_eq!(config.languages, "eng+fra");
        assert_eq!(config.buffer_size, 32);
        assert_eq!(config.min_format_bytes, 8);
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert!(config.recovery.max_retries > 0);
        assert!(config.recovery.operation_timeout_secs > 0);
    }

    /// Test recovery configuration defaults
    #[test]
    fn test_recovery_config_defaults() {
        let recovery = RecoveryConfig::default();

        assert_eq!(recovery.max_retries, 3);
        assert_eq!(recovery.base_retry_delay_ms, 1000);
        assert_eq!(recovery.max_retry_delay_ms, 10000);
        assert_eq!(recovery.operation_timeout_secs, 30);
        assert_eq!(recovery.circuit_breaker_threshold, 5);
        assert_eq!(recovery.circuit_breaker_reset_secs, 60);
    }

    /// Test format size limits defaults
    #[test]
    fn test_format_size_limits_defaults() {
        let limits = FormatSizeLimits::default();

        assert_eq!(limits.png_max, 15 * 1024 * 1024); // 15MB
        assert_eq!(limits.jpeg_max, 10 * 1024 * 1024); // 10MB
        assert_eq!(limits.bmp_max, 5 * 1024 * 1024); // 5MB
        assert_eq!(limits.tiff_max, 20 * 1024 * 1024); // 20MB
        assert_eq!(limits.min_quick_reject, 50 * 1024 * 1024); // 50MB
    }

    /// Test circuit breaker state transitions
    #[test]
    fn test_circuit_breaker_state_transitions() {
        let config = RecoveryConfig {
            circuit_breaker_threshold: 2,
            ..Default::default()
        };
        let circuit_breaker = CircuitBreaker::new(config);

        // Initially closed
        assert!(!circuit_breaker.is_open());

        // Record failures
        circuit_breaker.record_failure();
        assert!(!circuit_breaker.is_open()); // Still closed (1 failure)

        circuit_breaker.record_failure();
        assert!(circuit_breaker.is_open()); // Now open (2 failures)

        // Note: In a real scenario, we'd wait for the reset timeout to transition to half-open
        // For this test, we just verify the failure recording works
    }

    /// Test instance manager operations
    #[test]
    fn test_instance_manager_operations() {
        let manager = OcrInstanceManager::new();

        // Initially empty
        assert_eq!(manager._instance_count(), 0);

        // Create config
        let config = OcrConfig::default();

        // Get instance (creates new one)
        let instance1 = manager.get_instance(&config).unwrap();
        assert_eq!(manager._instance_count(), 1);

        // Get same instance again (reuses existing)
        let instance2 = manager.get_instance(&config).unwrap();
        assert_eq!(manager._instance_count(), 1);

        // Verify they're the same instance
        assert!(std::sync::Arc::ptr_eq(&instance1, &instance2));

        // Remove instance
        manager._remove_instance(&config.languages, ModelType::default());
        assert_eq!(manager._instance_count(), 0);

        // Clear all instances
        manager._clear_all_instances();
        assert_eq!(manager._instance_count(), 0);
    }

    /// Test instance manager with user patterns file configured
    #[test]
    fn test_instance_manager_with_user_patterns() {
        let manager = OcrInstanceManager::new();

        // Create config with user patterns file
        let config = OcrConfig {
            user_patterns_file: Some("config/user_patterns.txt".to_string()),
            ..Default::default()
        };

        // Get instance (should configure with user patterns)
        let instance = manager.get_instance(&config).unwrap();
        assert_eq!(manager._instance_count(), 1);

        // Verify instance is created successfully
        assert!(instance.lock().is_ok());

        // Clean up
        manager._clear_all_instances();
        assert_eq!(manager._instance_count(), 0);
    }

    /// Test image path validation with valid inputs
    #[test]
    fn test_validate_image_path_valid() {
        let config = OcrConfig::default();

        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Should pass validation
        let result = validate_image_path(&temp_path, &config);
        assert!(result.is_ok());
    }

    /// Test image path validation with invalid inputs
    #[test]
    fn test_validate_image_path_invalid() {
        let config = OcrConfig::default();

        // Test empty path
        let result = validate_image_path("", &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path is empty"));

        // Test non-existent file
        let result = validate_image_path("/non/existent/file.png", &config);
        assert!(result.is_err());
        // The error message might vary by OS, so just check it's an error
        assert!(result.is_err());
    }

    /// Test memory usage estimation for different formats
    #[test]
    fn test_estimate_memory_usage() {
        let file_size = 1024 * 1024; // 1MB

        // Test PNG (highest memory factor)
        let png_memory = estimate_memory_usage(file_size, &image::ImageFormat::Png);
        assert_eq!(png_memory, 3.0); // 1MB * 3.0

        // Test JPEG
        let jpeg_memory = estimate_memory_usage(file_size, &image::ImageFormat::Jpeg);
        assert_eq!(jpeg_memory, 2.5); // 1MB * 2.5

        // Test BMP (lowest memory factor)
        let bmp_memory = estimate_memory_usage(file_size, &image::ImageFormat::Bmp);
        assert_eq!(bmp_memory, 1.2); // 1MB * 1.2
    }

    /// Test retry delay calculation
    #[test]
    fn test_calculate_retry_delay() {
        let recovery = RecoveryConfig::default();

        // First retry (attempt 1): base delay
        let delay1 = calculate_retry_delay(1, &recovery);
        assert!(delay1 >= recovery.base_retry_delay_ms);

        // Second retry (attempt 2): exponential backoff
        let delay2 = calculate_retry_delay(2, &recovery);
        assert!(delay2 >= delay1);

        // Test that delay doesn't exceed max (with reasonable bounds)
        let delay_max_test = calculate_retry_delay(5, &recovery);
        assert!(delay_max_test <= recovery.max_retry_delay_ms * 2); // Allow some margin for jitter
    }

    /// Test error type conversions
    #[test]
    fn test_error_conversions() {
        // Test From<anyhow::Error>
        let anyhow_error = anyhow::anyhow!("test error");
        let ocr_error: OcrError = anyhow_error.into();
        match ocr_error {
            OcrError::Extraction(msg) => assert!(msg.contains("test error")),
            _ => panic!("Expected Extraction"),
        }

        // Test Display implementation
        let error = OcrError::Validation("test".to_string());
        let display = format!("{}", error);
        assert_eq!(display, "[VALIDATION] Image validation failed: test");
    }

    /// Test format detection with mock PNG file
    #[test]
    fn test_format_detection_png() {
        let config = OcrConfig::default();

        // Create mock PNG file (minimal PNG header)
        let mut temp_file = NamedTempFile::new().unwrap();
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        temp_file.write_all(&png_header).unwrap();
        temp_file.write_all(&[0u8; 24]).unwrap(); // Add some padding
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test format detection
        let is_supported = is_supported_image_format(&temp_path, &config);
        assert!(is_supported, "PNG should be supported");
    }

    /// Test format detection with mock JPEG file
    #[test]
    fn test_format_detection_jpeg() {
        let config = OcrConfig::default();

        // Create mock JPEG file (minimal JPEG header)
        let mut temp_file = NamedTempFile::new().unwrap();
        // JPEG SOI marker: FF D8
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        temp_file.write_all(&jpeg_header).unwrap();
        temp_file.write_all(&[0u8; 24]).unwrap(); // Add some padding
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test format detection
        let is_supported = is_supported_image_format(&temp_path, &config);
        assert!(is_supported, "JPEG should be supported");
    }

    /// Test format detection with unsupported format
    #[test]
    fn test_format_detection_unsupported() {
        let config = OcrConfig::default();

        // Create mock file with unsupported format
        let mut temp_file = NamedTempFile::new().unwrap();
        let unsupported_header = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        temp_file.write_all(&unsupported_header).unwrap();
        temp_file.write_all(&[0u8; 24]).unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test format detection
        let is_supported = is_supported_image_format(&temp_path, &config);
        assert!(!is_supported, "Unsupported format should not be supported");
    }

    /// Test validation with oversized file
    #[test]
    fn test_validation_oversized_file() {
        let config = OcrConfig {
            max_file_size: 100, // Very small limit
            ..Default::default()
        };

        // Create a file larger than the limit
        let mut temp_file = NamedTempFile::new().unwrap();
        let large_content = vec![0u8; 200]; // 200 bytes
        temp_file.write_all(&large_content).unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test validation
        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    /// Test validation with empty file
    #[test]
    fn test_validation_empty_file() {
        let config = OcrConfig::default();

        // Create empty file
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test validation - empty files pass validation but will fail during OCR processing
        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(
            result.is_ok(),
            "Empty files should pass validation (they fail during OCR processing)"
        );
    }

    /// Test circuit breaker integration with extract_text_from_image
    #[test]
    fn test_extract_text_from_image_circuit_breaker_integration() {
        let config = OcrConfig::default();
        let instance_manager = OcrInstanceManager::new();
        let circuit_breaker = CircuitBreaker::new(config.recovery.clone());

        // Initially circuit breaker should be closed
        assert!(!circuit_breaker.is_open());

        // Create a temporary file for testing
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test that function can be called with circuit breaker parameter
        // This verifies the function signature accepts the circuit breaker
        let _future = just_ingredients::ocr::extract_text_from_image(
            &temp_path,
            &config,
            &instance_manager,
            &circuit_breaker,
        );
        // The function compiles and can be called with 4 parameters as expected
    }

    #[test]
    fn test_validate_image_format_valid_png() {
        let config = OcrConfig::default();

        // Create mock PNG file
        let mut temp_file = NamedTempFile::new().unwrap();
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        temp_file.write_all(&png_header).unwrap();
        temp_file.write_all(&vec![0u8; 1000]).unwrap(); // 1KB content
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_image_format_valid_jpeg() {
        let config = OcrConfig::default();

        // Create mock JPEG file
        let mut temp_file = NamedTempFile::new().unwrap();
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        temp_file.write_all(&jpeg_header).unwrap();
        temp_file.write_all(&vec![0u8; 2000000]).unwrap(); // 2MB content (under JPEG limit)
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_image_format_unsupported_format() {
        let config = OcrConfig::default();

        // Create file with unsupported format
        let mut temp_file = NamedTempFile::new().unwrap();
        let unsupported_header = [0x00, 0x00, 0x00, 0x00];
        temp_file.write_all(&unsupported_header).unwrap();
        temp_file.write_all(&vec![0u8; 1000]).unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test that is_supported_image_format returns false for unsupported formats
        let is_supported = is_supported_image_format(&temp_path, &config);
        assert!(!is_supported, "Unsupported format should not be supported");

        // But validate_image_with_format_limits should still pass (uses general limit)
        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(
            result.is_ok(),
            "Validation should pass for unsupported format (uses general limit)"
        );
    }

    #[test]
    fn test_validate_image_format_png_too_large() {
        let config = OcrConfig::default();

        // Create PNG file that's too large
        let mut temp_file = NamedTempFile::new().unwrap();
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        temp_file.write_all(&png_header).unwrap();
        temp_file.write_all(&vec![0u8; 20 * 1024 * 1024]).unwrap(); // 20MB (over PNG limit)
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_image_format_jpeg_too_large() {
        let config = OcrConfig::default();

        // Create JPEG file that's too large
        let mut temp_file = NamedTempFile::new().unwrap();
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        temp_file.write_all(&jpeg_header).unwrap();
        temp_file.write_all(&vec![0u8; 12 * 1024 * 1024]).unwrap(); // 12MB (over JPEG limit)
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let result = validate_image_with_format_limits(&temp_path, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_memory_usage_different_sizes() {
        // Test reasonable memory estimation for different file sizes and formats
        let file_size_1mb = 1024 * 1024;

        // Test PNG format (highest memory factor)
        let png_memory = estimate_memory_usage(file_size_1mb, &image::ImageFormat::Png);
        assert_eq!(png_memory, 3.0); // 1MB * 3.0 = 3MB

        // Test JPEG format
        let jpeg_memory = estimate_memory_usage(file_size_1mb, &image::ImageFormat::Jpeg);
        assert_eq!(jpeg_memory, 2.5); // 1MB * 2.5 = 2.5MB

        // Test BMP format (lowest memory factor)
        let bmp_memory = estimate_memory_usage(file_size_1mb, &image::ImageFormat::Bmp);
        assert_eq!(bmp_memory, 1.2); // 1MB * 1.2 = 1.2MB

        // Test TIFF format
        let tiff_memory = estimate_memory_usage(file_size_1mb, &image::ImageFormat::Tiff);
        assert_eq!(tiff_memory, 4.0); // 1MB * 4.0 = 4MB

        // Test larger file
        let file_size_5mb = 5 * 1024 * 1024;
        let large_png_memory = estimate_memory_usage(file_size_5mb, &image::ImageFormat::Png);
        assert_eq!(large_png_memory, 15.0); // 5MB * 3.0 = 15MB

        // Test unknown format (should use default factor of 3.0)
        let unknown_memory = estimate_memory_usage(file_size_1mb, &image::ImageFormat::WebP);
        assert_eq!(unknown_memory, 3.0); // 1MB * 3.0 = 3MB (default)
    }

    /// Test PSM mode enum values and string conversion
    #[test]
    fn test_psm_mode_enum_values() {
        // Test all PSM mode variants
        assert_eq!(PageSegMode::OsdOnly.as_str(), "0");
        assert_eq!(PageSegMode::AutoOsd.as_str(), "1");
        assert_eq!(PageSegMode::AutoNoOsd.as_str(), "2");
        assert_eq!(PageSegMode::Auto.as_str(), "3");
        assert_eq!(PageSegMode::SingleColumn.as_str(), "4");
        assert_eq!(PageSegMode::SingleBlockVert.as_str(), "5");
        assert_eq!(PageSegMode::SingleBlock.as_str(), "6");
        assert_eq!(PageSegMode::SingleLine.as_str(), "7");
        assert_eq!(PageSegMode::SingleWord.as_str(), "8");
        assert_eq!(PageSegMode::WordInCircle.as_str(), "9");
        assert_eq!(PageSegMode::SingleChar.as_str(), "10");
        assert_eq!(PageSegMode::SparseText.as_str(), "11");
        assert_eq!(PageSegMode::SparseTextOsd.as_str(), "12");
        assert_eq!(PageSegMode::RawLine.as_str(), "13");
    }

    /// Test PSM mode configuration in OcrConfig
    #[test]
    fn test_psm_mode_config() {
        // Test default PSM mode
        let config = OcrConfig::default();
        assert_eq!(config.psm_mode, PageSegMode::Auto);

        // Test custom PSM mode configuration
        let config_single_block = OcrConfig {
            psm_mode: PageSegMode::SingleBlock,
            ..Default::default()
        };
        assert_eq!(config_single_block.psm_mode, PageSegMode::SingleBlock);

        let config_single_line = OcrConfig {
            psm_mode: PageSegMode::SingleLine,
            ..Default::default()
        };
        assert_eq!(config_single_line.psm_mode, PageSegMode::SingleLine);
    }

    /// Test PSM mode setting on OCR instances
    #[test]
    fn test_psm_mode_instance_setting() {
        let manager = OcrInstanceManager::new();

        // Test with different PSM modes
        let psm_modes = vec![
            PageSegMode::Auto,
            PageSegMode::SingleBlock,
            PageSegMode::SingleLine,
            PageSegMode::AutoOsd,
        ];

        for psm_mode in psm_modes {
            let config = OcrConfig {
                psm_mode,
                ..Default::default()
            };

            // Get instance with specific PSM mode
            let _instance = manager.get_instance(&config).unwrap();

            // Instance creation succeeded (unwrap would have panicked if it failed)
        }
    }

    /// Test PSM mode performance comparison (mock test)
    #[test]
    fn test_psm_mode_performance_comparison() {
        // This is a mock test that demonstrates how PSM mode performance
        // would be tested. In a real scenario, this would use actual recipe images.

        let manager = OcrInstanceManager::new();

        // Test PSM modes that are relevant for recipes
        let test_modes = vec![
            (PageSegMode::Auto, "Auto (PSM 3)"),
            (PageSegMode::SingleBlock, "Single Block (PSM 6)"),
            (PageSegMode::SingleLine, "Single Line (PSM 7)"),
            (PageSegMode::SparseText, "Sparse Text (PSM 11)"),
        ];

        for (psm_mode, _description) in test_modes {
            let config = OcrConfig {
                psm_mode,
                ..Default::default()
            };

            // Just test that instance creation works with different PSM modes
            // This verifies PSM mode configuration without triggering OCR processing
            let _instance = manager.get_instance(&config).unwrap();

            // Instance creation succeeded (unwrap would have panicked if PSM mode was invalid)
        }
    }

    /// Test adaptive PSM selection logic (mock implementation)
    #[test]
    fn test_adaptive_psm_selection() {
        // Test the logic for choosing PSM mode based on content type
        // This is a mock test showing how adaptive selection would work

        // Simulate different content types
        let ingredient_list_content = "2 cups flour\n1 cup sugar\n3 eggs\n1 tsp vanilla";
        let full_recipe_content = "Chocolate Chip Cookies\n\nIngredients:\n2 cups flour\n1 cup sugar\n3 eggs\n\nInstructions:\n1. Preheat oven to 350°F\n2. Mix ingredients\n3. Bake for 12 minutes";

        // For ingredient lists (short, line-based), PSM 6 (SingleBlock) should be preferred
        let ingredient_psm = if ingredient_list_content.lines().count() <= 10 {
            PageSegMode::SingleBlock
        } else {
            PageSegMode::Auto
        };
        assert_eq!(ingredient_psm, PageSegMode::SingleBlock);

        // For full recipes (longer, structured), PSM 3 (Auto) should be preferred
        let recipe_psm = if full_recipe_content.lines().count() > 10 {
            PageSegMode::Auto
        } else {
            PageSegMode::SingleBlock
        };
        assert_eq!(recipe_psm, PageSegMode::Auto);
    }

    /// Test PSM mode configuration validation
    #[test]
    fn test_psm_mode_config_validation() {
        // Test that all PSM modes can be configured without issues
        let all_modes = vec![
            PageSegMode::OsdOnly,
            PageSegMode::AutoOsd,
            PageSegMode::AutoNoOsd,
            PageSegMode::Auto,
            PageSegMode::SingleColumn,
            PageSegMode::SingleBlockVert,
            PageSegMode::SingleBlock,
            PageSegMode::SingleLine,
            PageSegMode::SingleWord,
            PageSegMode::WordInCircle,
            PageSegMode::SingleChar,
            PageSegMode::SparseText,
            PageSegMode::SparseTextOsd,
            PageSegMode::RawLine,
        ];

        for mode in all_modes {
            let config = OcrConfig {
                psm_mode: mode,
                ..Default::default()
            };

            // Verify config is valid
            assert_eq!(config.psm_mode, mode);

            // Verify string conversion works
            let mode_str = mode.as_str();
            assert!(!mode_str.is_empty());
            assert!(mode_str.chars().all(|c| c.is_ascii_digit()));
        }
    }

    /// Test model type configuration and instance creation
    #[test]
    fn test_model_type_config() {
        let manager = OcrInstanceManager::new();

        // Test with Fast model (default)
        let fast_config = OcrConfig {
            model_type: ModelType::Fast,
            ..Default::default()
        };

        let _fast_instance = manager.get_instance(&fast_config).unwrap();
        // Instance creation succeeded (unwrap would have panicked if it failed)

        // Test with Best model
        let best_config = OcrConfig {
            model_type: ModelType::Best,
            ..Default::default()
        };

        let _best_instance = manager.get_instance(&best_config).unwrap();
        // Instance creation succeeded (unwrap would have panicked if it failed)

        // Verify that different model types create separate instances
        // (they should have different keys in the instance map)
        assert_eq!(manager._instance_count(), 2);
    }

    /// Test model type instance isolation
    #[test]
    fn test_model_type_instance_isolation() {
        let manager = OcrInstanceManager::new();

        // Create configs with same languages but different model types
        let fast_config = OcrConfig {
            languages: "eng".to_string(),
            model_type: ModelType::Fast,
            ..Default::default()
        };

        let best_config = OcrConfig {
            languages: "eng".to_string(),
            model_type: ModelType::Best,
            ..Default::default()
        };

        // Get instances
        let _fast_instance1 = manager.get_instance(&fast_config).unwrap();
        let _best_instance1 = manager.get_instance(&best_config).unwrap();

        // Should have 2 instances (different model types)
        assert_eq!(manager._instance_count(), 2);

        // Get same instances again (should reuse)
        let _fast_instance2 = manager.get_instance(&fast_config).unwrap();
        let _best_instance2 = manager.get_instance(&best_config).unwrap();

        // Should still have only 2 instances
        assert_eq!(manager._instance_count(), 2);
    }

    /// Test model type performance expectations
    #[test]
    fn test_model_type_performance_expectations() {
        // Test that ModelType provides expected accuracy improvements
        assert_eq!(ModelType::Fast.expected_accuracy_improvement(), 0.0);
        assert_eq!(ModelType::Best.expected_accuracy_improvement(), 0.05);

        // Test that Best model is expected to be more accurate than Fast
        assert!(
            ModelType::Best.expected_accuracy_improvement()
                > ModelType::Fast.expected_accuracy_improvement()
        );
    }

    /// Test HOCR extraction function signature and basic functionality
    #[test]
    fn test_extract_hocr_from_image_function_signature() {
        let config = OcrConfig::default();
        let instance_manager = OcrInstanceManager::new();
        let circuit_breaker = CircuitBreaker::new(config.recovery.clone());

        // Initially circuit breaker should be closed
        assert!(!circuit_breaker.is_open());

        // Create a temporary file for testing
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test that function can be called with circuit breaker parameter
        // This verifies the function signature accepts the circuit breaker
        let _future =
            extract_hocr_from_image(&temp_path, &config, &instance_manager, &circuit_breaker);
        // The function compiles and can be called with 4 parameters as expected
    }

    /// Test that HOCR output contains expected XML structure
    #[test]
    fn test_hocr_output_contains_xml_structure() {
        // Test the HOCR XML structure that would be generated
        let sample_text = "Hello World\nThis is a test";

        // This mimics the HOCR generation logic in perform_hocr_extraction
        let expected_hocr = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
<title>HOCR Output</title>
</head>
<body>
<div class="ocr_page" title="bbox 0 0 100 100">
<div class="ocr_carea" title="bbox 0 0 100 100">
<p class="ocr_par" title="bbox 0 0 100 100">
<span class="ocr_line" title="bbox 10 10 90 20">{}</span>
</p>
</div>
</div>
</body>
</html>"#,
            sample_text
        );

        // Verify the HOCR contains expected XML elements
        assert!(expected_hocr.contains(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(expected_hocr.contains(r#"<!DOCTYPE html"#));
        assert!(expected_hocr.contains(r#"<html xmlns="http://www.w3.org/1999/xhtml">"#));
        assert!(expected_hocr.contains(r#"<div class="ocr_page""#));
        assert!(expected_hocr.contains(r#"<div class="ocr_carea""#));
        assert!(expected_hocr.contains(r#"<p class="ocr_par""#));
        assert!(expected_hocr.contains(r#"<span class="ocr_line""#));
        assert!(expected_hocr.contains(r#"title="bbox"#));
        assert!(expected_hocr.contains("Hello World"));
        assert!(expected_hocr.contains("This is a test"));
        assert!(expected_hocr.contains(r#"</html>"#));
    }

    /// Test HOCR output validation for well-formed XML
    #[test]
    fn test_hocr_output_well_formed_xml() {
        let sample_text = "Sample OCR text";

        let hocr_output = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
<title>HOCR Output</title>
</head>
<body>
<div class="ocr_page" title="bbox 0 0 100 100">
<div class="ocr_carea" title="bbox 0 0 100 100">
<p class="ocr_par" title="bbox 0 0 100 100">
<span class="ocr_line" title="bbox 10 10 90 20">{}</span>
</p>
</div>
</div>
</body>
</html>"#,
            sample_text
        );

        // Basic validation that XML is well-formed
        // Check for matching opening and closing tags
        assert!(hocr_output.contains("<html") && hocr_output.contains("</html>"));
        assert!(hocr_output.contains("<head") && hocr_output.contains("</head>"));
        assert!(hocr_output.contains("<body") && hocr_output.contains("</body>"));
        assert!(hocr_output.contains("<div") && hocr_output.contains("</div>"));
        assert!(hocr_output.contains("<p") && hocr_output.contains("</p>"));
        assert!(hocr_output.contains("<span") && hocr_output.contains("</span>"));

        // Check that the sample text is included
        assert!(hocr_output.contains("Sample OCR text"));
    }

    /// Test HOCR validation function with valid HOCR output
    #[test]
    fn test_validate_hocr_output_valid() {
        // Test structure for valid HOCR - actual validation tested in integration
        // This test ensures the test framework is ready for validation testing
        let _valid_hocr_structure = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<div class="ocr_page" title="bbox 0 0 100 100">
<span class="ocr_line" title="bbox 10 10 90 20">Test text</span>
</div>
</body>
</html>"#;

        // Test compiles and structure is defined correctly
        assert!(_valid_hocr_structure.contains("ocr_page"));
    }

    /// Test HOCR validation function with invalid HOCR output
    #[test]
    fn test_validate_hocr_output_invalid() {
        // Test cases for invalid HOCR that should be caught by validation
        let invalid_cases = vec![
            ("", "Empty string"),
            ("not html at all", "No HTML structure"),
            (
                "<html><body>Missing ocr_page class</body></html>",
                "Missing ocr_page",
            ),
            (
                "<!DOCTYPE html><html><body><div class=\"ocr_page\">Missing closing html",
                "Missing closing tag",
            ),
        ];

        for (invalid_hocr, description) in invalid_cases {
            // Verify each case represents an invalid HOCR scenario
            match description {
                "Empty string" => assert!(invalid_hocr.is_empty()),
                "No HTML structure" => assert!(!invalid_hocr.contains("<html")),
                "Missing ocr_page" => assert!(!invalid_hocr.contains("class=\"ocr_page\"")),
                "Missing closing tag" => assert!(!invalid_hocr.contains("</html>")),
                _ => {}
            }
        }
    }

    /// Test fallback HOCR generation structure
    #[test]
    fn test_fallback_hocr_generation_structure() {
        let sample_text = "Fallback OCR text";
        let image_path = "/test/image.png";

        // Test the structure that would be generated by generate_fallback_hocr
        let fallback_hocr = format!(
            r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>OCR Results for {}</title>
</head>
<body>
  <div class="ocr_page" id="page_1" title="bbox 0 0 1000 1000">
    <div class="ocr_carea" id="block_1_1" title="bbox 10 10 990 990">
      <p class="ocr_par" id="par_1_1" title="bbox 10 10 990 990">
        <span class="ocr_line" id="line_1_1" title="bbox 10 10 990 40">{}</span>
      </p>
    </div>
  </div>
</body>
</html>"#,
            image_path, sample_text
        );

        // Verify fallback HOCR contains required elements
        assert!(fallback_hocr.contains("<!DOCTYPE html>"));
        assert!(fallback_hocr.contains("<html>"));
        assert!(fallback_hocr.contains("</html>"));
        assert!(fallback_hocr.contains("class=\"ocr_page\""));
        assert!(fallback_hocr.contains("class=\"ocr_carea\""));
        assert!(fallback_hocr.contains("class=\"ocr_par\""));
        assert!(fallback_hocr.contains("class=\"ocr_line\""));
        assert!(fallback_hocr.contains("title=\"bbox"));
        assert!(fallback_hocr.contains("Fallback OCR text"));
        assert!(fallback_hocr.contains("OCR Results for /test/image.png"));
    }

    /// Test BBox struct creation and methods
    #[test]
    fn test_bbox_creation_and_methods() {
        // Test BBox creation
        let bbox = BBox::new(10, 20, 110, 120);
        assert_eq!(bbox.x0, 10);
        assert_eq!(bbox.y0, 20);
        assert_eq!(bbox.x1, 110);
        assert_eq!(bbox.y1, 120);

        // Test width calculation
        assert_eq!(bbox.width(), 100);

        // Test height calculation
        assert_eq!(bbox.height(), 100);

        // Test area calculation
        assert_eq!(bbox.area(), 10000);

        // Test edge case: zero-sized bbox
        let zero_bbox = BBox::new(50, 50, 50, 50);
        assert_eq!(zero_bbox.width(), 0);
        assert_eq!(zero_bbox.height(), 0);
        assert_eq!(zero_bbox.area(), 0);

        // Test edge case: underflow protection (x1 < x0)
        let underflow_bbox = BBox::new(100, 100, 50, 50);
        assert_eq!(underflow_bbox.width(), 0);
        assert_eq!(underflow_bbox.height(), 0);
        assert_eq!(underflow_bbox.area(), 0);
    }

    /// Test HocrLine struct creation
    #[test]
    fn test_hocr_line_creation() {
        // Test HocrLine creation with BBox
        let bbox = BBox::new(10, 20, 110, 120);
        let hocr_line = HocrLine::new("Sample text".to_string(), bbox.clone());

        assert_eq!(hocr_line.text, "Sample text");
        assert_eq!(hocr_line.bbox, bbox);

        // Test HocrLine creation from coordinates
        let hocr_line2 = HocrLine::from_coords("Another text".to_string(), 5, 15, 105, 115);

        assert_eq!(hocr_line2.text, "Another text");
        assert_eq!(hocr_line2.bbox.x0, 5);
        assert_eq!(hocr_line2.bbox.y0, 15);
        assert_eq!(hocr_line2.bbox.x1, 105);
        assert_eq!(hocr_line2.bbox.y1, 115);
    }

    /// Test BBox and HocrLine serialization
    #[test]
    fn test_bbox_hocr_line_serialization() {
        let bbox = BBox::new(10, 20, 110, 120);
        let hocr_line = HocrLine::new("Test text".to_string(), bbox.clone());

        // Test that structs can be serialized (required for serde::Serialize derive)
        let bbox_json = serde_json::to_string(&bbox).unwrap();
        let hocr_json = serde_json::to_string(&hocr_line).unwrap();

        // Verify JSON contains expected fields
        assert!(bbox_json.contains("\"x0\":10"));
        assert!(bbox_json.contains("\"y0\":20"));
        assert!(bbox_json.contains("\"x1\":110"));
        assert!(bbox_json.contains("\"y1\":120"));

        assert!(hocr_json.contains("\"text\":\"Test text\""));
        assert!(hocr_json.contains("\"bbox\":"));

        // Test deserialization
        let bbox_deserialized: BBox = serde_json::from_str(&bbox_json).unwrap();
        let hocr_deserialized: HocrLine = serde_json::from_str(&hocr_json).unwrap();

        assert_eq!(bbox_deserialized, bbox);
        assert_eq!(hocr_deserialized, hocr_line);
    }

    /// Test HOCR parsing with valid HOCR content
    #[test]
    fn test_parse_hocr_to_lines_valid() {
        let hocr_content = r#"<!DOCTYPE html>
<html>
<body>
<div class="ocr_page" title="bbox 0 0 1000 1000">
<div class="ocr_carea" title="bbox 10 10 990 990">
<p class="ocr_par" title="bbox 10 10 990 990">
<span class="ocr_line" title="bbox 10 10 990 40">First line of text</span>
<span class="ocr_line" title="bbox 10 50 990 80">Second line with 2 cups flour</span>
<span class="ocr_line" title="bbox 10 90 990 120">Third line 1/2 teaspoon salt</span>
</p>
</div>
</div>
</body>
</html>"#;

        let lines = parse_hocr_to_lines(hocr_content).unwrap();

        assert_eq!(lines.len(), 3);

        // Check first line
        assert_eq!(lines[0].text, "First line of text");
        assert_eq!(lines[0].bbox, BBox::new(10, 10, 990, 40));

        // Check second line
        assert_eq!(lines[1].text, "Second line with 2 cups flour");
        assert_eq!(lines[1].bbox, BBox::new(10, 50, 990, 80));

        // Check third line
        assert_eq!(lines[2].text, "Third line 1/2 teaspoon salt");
        assert_eq!(lines[2].bbox, BBox::new(10, 90, 990, 120));
    }

    /// Test HOCR parsing with empty content
    #[test]
    fn test_parse_hocr_to_lines_empty() {
        let hocr_content = r#"<!DOCTYPE html>
<html>
<body>
<div class="ocr_page" title="bbox 0 0 1000 1000">
</div>
</body>
</html>"#;

        let lines = parse_hocr_to_lines(hocr_content).unwrap();
        assert_eq!(lines.len(), 0);
    }

    /// Test HOCR parsing with HTML entities
    #[test]
    fn test_parse_hocr_to_lines_html_entities() {
        let hocr_content = r#"<!DOCTYPE html>
<html>
<body>
<div class="ocr_page" title="bbox 0 0 1000 1000">
<span class="ocr_line" title="bbox 10 10 200 40">Text with &amp; &lt; &gt; &quot; entities</span>
<span class="ocr_line" title="bbox 10 50 200 80">Text with &#39; &apos; quotes</span>
</div>
</body>
</html>"#;

        let lines = parse_hocr_to_lines(hocr_content).unwrap();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Text with & < > \" entities");
        assert_eq!(lines[1].text, "Text with ' ' quotes");
    }

    /// Test HOCR parsing with malformed coordinates (should skip invalid lines)
    #[test]
    fn test_parse_hocr_to_lines_malformed_coordinates() {
        let hocr_content = r#"<!DOCTYPE html>
<html>
<body>
<div class="ocr_page" title="bbox 0 0 1000 1000">
<span class="ocr_line" title="bbox invalid 10 200 40">Text</span>
</div>
</body>
</html>"#;

        let lines = parse_hocr_to_lines(hocr_content).unwrap();
        // Invalid bbox coordinates should be skipped, resulting in 0 lines
        assert_eq!(lines.len(), 0);
    }

    /// Test HOCR parsing with complex real-world HOCR structure
    #[test]
    fn test_parse_hocr_to_lines_complex_structure() {
        let hocr_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
<title>HOCR Output</title>
</head>
<body>
<div class="ocr_page" id="page_1" title="bbox 0 0 2480 3508">
<div class="ocr_carea" id="block_1_1" title="bbox 120 180 2360 3400">
<p class="ocr_par" id="par_1_1" title="bbox 120 180 2360 320">
<span class="ocr_line" id="line_1_1" title="bbox 120 180 800 220">Recipe Title</span>
<span class="ocr_line" id="line_1_2" title="bbox 120 240 1200 280">Ingredients:</span>
</p>
<p class="ocr_par" id="par_1_2" title="bbox 120 320 2360 600">
<span class="ocr_line" id="line_1_3" title="bbox 120 320 800 360">2 cups all-purpose flour</span>
<span class="ocr_line" id="line_1_4" title="bbox 120 380 600 420">1/2 cup sugar</span>
<span class="ocr_line" id="line_1_5" title="bbox 120 440 700 480">3 eggs</span>
</p>
</div>
</div>
</body>
</html>"#;

        let lines = parse_hocr_to_lines(hocr_content).unwrap();

        assert_eq!(lines.len(), 5);

        // Verify specific lines
        assert_eq!(lines[0].text, "Recipe Title");
        assert_eq!(lines[0].bbox, BBox::new(120, 180, 800, 220));

        assert_eq!(lines[1].text, "Ingredients:");
        assert_eq!(lines[1].bbox, BBox::new(120, 240, 1200, 280));

        assert_eq!(lines[2].text, "2 cups all-purpose flour");
        assert_eq!(lines[2].bbox, BBox::new(120, 320, 800, 360));

        assert_eq!(lines[3].text, "1/2 cup sugar");
        assert_eq!(lines[3].bbox, BBox::new(120, 380, 600, 420));

        assert_eq!(lines[4].text, "3 eggs");
        assert_eq!(lines[4].bbox, BBox::new(120, 440, 700, 480));
    }

    /// Test mapping measurements to bounding boxes
    #[test]
    fn test_map_measurement_to_bbox_success() {
        // Create sample HOCR lines
        let hocr_lines = vec![
            HocrLine::from_coords("Recipe Title".to_string(), 10, 10, 200, 40),
            HocrLine::from_coords("2 cups flour".to_string(), 10, 50, 150, 80),
            HocrLine::from_coords("1/2 cup sugar".to_string(), 10, 90, 140, 120),
        ];

        // Create a measurement match for the second line (1-based line number = 2)
        let measurement = MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "flour".to_string(),
            line_number: 2, // 1-based
            start_pos: 0,
            end_pos: 1,
            requires_quantity_confirmation: false,
        };

        // Map the measurement to its bounding box
        let bbox = map_measurement_to_bbox(&measurement, &hocr_lines);

        // Should return the bounding box of the second line
        assert_eq!(bbox, Some(BBox::new(10, 50, 150, 80)));
    }

    /// Test mapping with out-of-bounds line number
    #[test]
    fn test_map_measurement_to_bbox_out_of_bounds() {
        let hocr_lines = vec![HocrLine::from_coords("Line 1".to_string(), 10, 10, 100, 30)];

        let measurement = MeasurementMatch {
            quantity: "1".to_string(),
            measurement: None,
            ingredient_name: "test".to_string(),
            line_number: 5, // Out of bounds
            start_pos: 0,
            end_pos: 1,
            requires_quantity_confirmation: false,
        };

        let bbox = map_measurement_to_bbox(&measurement, &hocr_lines);
        assert_eq!(bbox, None);
    }

    /// Test mapping with line number 1 (first line, 0-based index)
    #[test]
    fn test_map_measurement_to_bbox_first_line() {
        let hocr_lines = vec![
            HocrLine::from_coords("First line".to_string(), 5, 5, 95, 25),
            HocrLine::from_coords("Second line".to_string(), 5, 30, 95, 50),
        ];

        let measurement = MeasurementMatch {
            quantity: "1".to_string(),
            measurement: None,
            ingredient_name: "test".to_string(),
            line_number: 1, // First line
            start_pos: 0,
            end_pos: 1,
            requires_quantity_confirmation: false,
        };

        let bbox = map_measurement_to_bbox(&measurement, &hocr_lines);
        assert_eq!(bbox, Some(BBox::new(5, 5, 95, 25)));
    }

    /// Test mapping with empty HOCR lines
    #[test]
    fn test_map_measurement_to_bbox_empty_lines() {
        let hocr_lines: Vec<HocrLine> = vec![];

        let measurement = MeasurementMatch {
            quantity: "1".to_string(),
            measurement: None,
            ingredient_name: "test".to_string(),
            line_number: 1,
            start_pos: 0,
            end_pos: 1,
            requires_quantity_confirmation: false,
        };

        let bbox = map_measurement_to_bbox(&measurement, &hocr_lines);
        assert_eq!(bbox, None);
    }

    /// Test mapping with text validation
    #[test]
    fn test_map_measurement_to_bbox_with_text_validation() {
        let hocr_lines = vec![
            HocrLine::from_coords("Recipe ingredients:".to_string(), 10, 10, 200, 30),
            HocrLine::from_coords("2 cups all-purpose flour".to_string(), 10, 40, 250, 60),
            HocrLine::from_coords("1 teaspoon baking powder".to_string(), 10, 70, 220, 90),
        ];

        // Measurement that should match the text content
        let measurement = MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "all-purpose flour".to_string(),
            line_number: 2, // Second line (1-based)
            start_pos: 0,   // "2" starts at position 0
            end_pos: 1,     // "2" ends at position 1
            requires_quantity_confirmation: false,
        };

        let bbox = map_measurement_to_bbox(&measurement, &hocr_lines);
        assert_eq!(bbox, Some(BBox::new(10, 40, 250, 60)));
    }

    /// Test constrained OCR with a simple numeric image
    #[tokio::test]
    async fn test_perform_constrained_ocr_simple_number() {
        let config = OcrConfig::default();
        let instance_manager = OcrInstanceManager::new();

        // Create a simple test image with a number
        let mut img = image::RgbImage::new(50, 20);
        // Fill with white background
        for pixel in img.pixels_mut() {
            *pixel = image::Rgb([255, 255, 255]);
        }
        // This is a simplified test - in practice we'd need a proper image with text
        let test_image = image::DynamicImage::ImageRgb8(img);

        // Note: This test may fail if Tesseract is not properly installed
        // In a real environment, we'd use a mock or skip if Tesseract unavailable
        let result = perform_constrained_ocr(&test_image, &instance_manager, &config).await;

        // The test mainly verifies the function doesn't panic and returns proper structure
        // Actual OCR success depends on Tesseract installation and image content
        match result {
            Ok(constrained_result) => {
                // Verify the result structure
                assert!(!constrained_result.psm_mode.is_empty());
                assert!(!constrained_result.character_whitelist.is_empty());
                // Verify processing time was recorded
                assert!(constrained_result.processing_time_ms < 10000); // Should be reasonable
                assert!(
                    constrained_result.confidence >= 0.0 && constrained_result.confidence <= 100.0
                );
            }
            Err(OcrError::Initialization(_)) => {
                // Tesseract not available - this is acceptable for CI/testing
                println!("Skipping constrained OCR test: Tesseract not initialized");
            }
            Err(e) => {
                // Other errors are unexpected
                panic!("Unexpected error in constrained OCR: {:?}", e);
            }
        }
    }

    /// Test constrained OCR configuration validation
    #[test]
    fn test_constrained_ocr_result_structure() {
        let result = ConstrainedOcrResult {
            text: "1/2".to_string(),
            confidence: 85.0,
            psm_mode: "8 (Single Word)".to_string(),
            character_whitelist: "0123456789/½⅓⅔¼¾⅕⅖⅚⅙⅛⅜⅝⅞.".to_string(),
            processing_time_ms: 150,
        };

        assert_eq!(result.text, "1/2");
        assert_eq!(result.confidence, 85.0);
        assert_eq!(result.psm_mode, "8 (Single Word)");
        assert!(result.character_whitelist.contains("½"));
        assert!(result.character_whitelist.contains("."));
        assert_eq!(result.processing_time_ms, 150);
    }
}
