//! # OCR Tests Module
//!
//! Comprehensive test suite for OCR processing functionality,
//! including configuration, validation, circuit breaker, and instance management.

#[cfg(test)]
mod tests {
    use just_ingredients::circuit_breaker::CircuitBreaker;
    use just_ingredients::instance_manager::OcrInstanceManager;
    use just_ingredients::ocr::{
        calculate_retry_delay, estimate_memory_usage, extract_text_from_image,
        is_supported_image_format, validate_image_path, validate_image_with_format_limits,
    };
    use just_ingredients::ocr_config::{
        FormatSizeLimits, ModelType, OcrConfig, PageSegMode, RecoveryConfig,
    };
    use just_ingredients::ocr_errors::OcrError;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
        let circuit_breaker = CircuitBreaker::new(RecoveryConfig::default());

        // Create a mock image file for testing
        let mut temp_file = NamedTempFile::new().unwrap();
        // Create a minimal PNG with some text-like content
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        temp_file.write_all(&png_header).unwrap();
        // Add some minimal PNG data (this won't be a valid image but will pass format checks)
        temp_file.write_all(&vec![0u8; 1000]).unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Test PSM modes that are relevant for recipes
        let test_modes = vec![
            (PageSegMode::Auto, "Auto (PSM 3)"),
            (PageSegMode::SingleBlock, "Single Block (PSM 6)"),
            (PageSegMode::SingleLine, "Single Line (PSM 7)"),
            (PageSegMode::SparseText, "Sparse Text (PSM 11)"),
        ];

        for (psm_mode, description) in test_modes {
            let config = OcrConfig {
                psm_mode,
                ..Default::default()
            };

            // Measure time for OCR processing (this will fail due to invalid image,
            // but we're testing that PSM mode configuration works)
            let start = std::time::Instant::now();

            let result = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(extract_text_from_image(
                    &temp_path,
                    &config,
                    &manager,
                    &circuit_breaker,
                ));

            let duration = start.elapsed();

            // The operation should fail due to invalid image, but PSM mode should be configured
            assert!(
                result.is_err(),
                "Expected error for invalid image with PSM {}",
                description
            );

            // Verify reasonable processing time (should be quick even with invalid image)
            // Allow more time since OCR initialization can be slow
            assert!(
                duration.as_millis() < 15000,
                "PSM {} took too long: {:?}",
                description,
                duration
            );
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
}
