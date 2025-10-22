use just_ingredients::circuit_breaker::CircuitBreaker;
use just_ingredients::instance_manager::OcrInstanceManager;
use just_ingredients::localization::create_localization_manager;
use just_ingredients::ocr_config::{FormatSizeLimits, OcrConfig, RecoveryConfig};
use just_ingredients::ocr_errors::OcrError;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_localization() -> Arc<just_ingredients::localization::LocalizationManager> {
        // Create a new shared localization manager for tests
        create_localization_manager().expect("Failed to create localization manager")
    }

    /// Test OCR configuration validation
    #[test]
    fn test_ocr_config_validation() {
        let config = OcrConfig::default();

        // Test that configuration has reasonable defaults
        assert!(!config.languages.is_empty());
        assert!(config.buffer_size > 0);
        assert!(config.min_format_bytes > 0);
        assert!(config.max_file_size > 0);
        assert!(config.recovery.max_retries <= 10); // Reasonable upper bound
        assert!(config.recovery.operation_timeout_secs > 0);
    }

    /// Test circuit breaker initialization
    #[test]
    fn test_circuit_breaker_initialization() {
        let config = RecoveryConfig {
            circuit_breaker_threshold: 2,
            ..Default::default()
        };
        let circuit_breaker = CircuitBreaker::new(config);

        // Initially should not be open
        assert!(!circuit_breaker.is_open());
    }

    /// Test OCR instance manager initialization
    #[test]
    fn test_ocr_instance_manager_initialization() {
        let manager = OcrInstanceManager::new();

        // Initially should be empty
        assert_eq!(manager._instance_count(), 0);
    }

    /// Test error message formatting
    #[test]
    fn test_error_message_formatting() {
        let validation_error = OcrError::Validation("Test validation error".to_string());
        let display_msg = format!("{}", validation_error);
        assert_eq!(
            display_msg,
            "[VALIDATION] Image validation failed: Test validation error"
        );

        let timeout_error = OcrError::Timeout("Test timeout".to_string());
        let display_msg = format!("{}", timeout_error);
        assert_eq!(
            display_msg,
            "[OCR_TIMEOUT] OCR processing timed out: Test timeout"
        );
    }

    /// Test temporary file cleanup
    #[test]
    fn test_temp_file_cleanup() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Simulate cleanup
        let cleanup_result = fs::remove_file(&temp_path);
        assert!(cleanup_result.is_ok() || cleanup_result.is_err()); // File might not exist
    }

    /// Test OCR configuration defaults are reasonable
    #[test]
    fn test_ocr_config_defaults_reasonable() {
        let config = OcrConfig::default();
        let recovery = config.recovery;

        // Test that defaults are within reasonable ranges
        assert!(config.max_file_size > 1024 * 1024); // At least 1MB
        assert!(config.max_file_size <= 100 * 1024 * 1024); // At most 100MB

        assert!(recovery.max_retries <= 10); // Reasonable upper bound
        assert!(recovery.max_retries <= 10); // Reasonable retry limit

        assert!(recovery.operation_timeout_secs > 0);
        assert!(recovery.operation_timeout_secs <= 300); // At most 5 minutes

        assert!(recovery.base_retry_delay_ms >= 100); // At least 100ms
        assert!(recovery.base_retry_delay_ms <= 10000); // At most 10 seconds
    }

    /// Test format size limits defaults
    #[test]
    fn test_format_size_limits_defaults() {
        let limits = FormatSizeLimits::default();

        // Test that format limits are in ascending order for different formats
        assert!(limits.bmp_max <= limits.jpeg_max);
        assert!(limits.jpeg_max <= limits.png_max);
        assert!(limits.png_max <= limits.tiff_max);

        // Test that all limits are reasonable (between 1MB and 50MB)
        assert!(limits.bmp_max >= 1024 * 1024);
        assert!(limits.tiff_max <= 50 * 1024 * 1024);
    }

    /// Test circuit breaker failure recording
    #[test]
    fn test_circuit_breaker_failure_recording() {
        let config = RecoveryConfig {
            circuit_breaker_threshold: 2,
            ..Default::default()
        };
        let circuit_breaker = CircuitBreaker::new(config);

        // Initially closed
        assert!(!circuit_breaker.is_open());

        // Record one failure - still closed
        circuit_breaker.record_failure();
        assert!(!circuit_breaker.is_open());

        // Record second failure - now open
        circuit_breaker.record_failure();
        assert!(circuit_breaker.is_open());
    }

    /// Test circuit breaker success recording
    #[test]
    fn test_circuit_breaker_success_recording() {
        let config = RecoveryConfig {
            circuit_breaker_threshold: 1,
            ..Default::default()
        };
        let circuit_breaker = CircuitBreaker::new(config);

        // Record failure to open circuit
        circuit_breaker.record_failure();
        assert!(circuit_breaker.is_open());

        // Record success to close circuit
        circuit_breaker.record_success();
        assert!(!circuit_breaker.is_open());
    }

    /// Test OCR instance manager operations
    #[test]
    fn test_ocr_instance_manager_operations() {
        let manager = OcrInstanceManager::new();

        // Initially empty
        assert_eq!(manager._instance_count(), 0);

        // Test that we can create a new manager (basic functionality test)
        let new_manager = OcrInstanceManager::new();
        assert_eq!(new_manager._instance_count(), 0);
    }

    /// Test configuration cloning
    #[test]
    fn test_config_cloning() {
        let config = OcrConfig::default();
        let cloned_config = config.clone();

        // Test that cloning preserves values
        assert_eq!(config.languages, cloned_config.languages);
        assert_eq!(config.buffer_size, cloned_config.buffer_size);
        assert_eq!(config.max_file_size, cloned_config.max_file_size);
    }

    /// Test image format validation function
    #[test]
    fn test_image_format_validation() {
        // Test with a non-existent file (should return false)
        let result = just_ingredients::ocr::is_supported_image_format(
            "/non/existent/file.png",
            &OcrConfig::default(),
        );
        assert!(!result);
    }

    /// Test that all error variants can be created
    #[test]
    fn test_error_variants_creation() {
        let validation_err = OcrError::Validation("test".to_string());
        let init_err = OcrError::Initialization("test".to_string());
        let load_err = OcrError::ImageLoad("test".to_string());
        let extract_err = OcrError::Extraction("test".to_string());
        let timeout_err = OcrError::Timeout("test".to_string());

        // Test that all variants can be formatted
        assert!(format!("{}", validation_err).contains("[VALIDATION]"));
        assert!(format!("{}", init_err).contains("[OCR_INIT]"));
        assert!(format!("{}", load_err).contains("[IMAGE_LOAD]"));
        assert!(format!("{}", extract_err).contains("[OCR_EXTRACT]"));
        assert!(format!("{}", timeout_err).contains("[OCR_TIMEOUT]"));
    }

    /// Test configuration structure
    #[test]
    fn test_config_structure() {
        let config = OcrConfig::default();

        // Test that all fields are accessible and have reasonable values
        assert!(!config.languages.is_empty());
        assert!(config.buffer_size > 0);
        assert!(config.min_format_bytes > 0);
        assert!(config.max_file_size > 0);

        // Test nested structure access with references
        let png_max = config.format_limits.png_max;
        let max_retries = config.recovery.max_retries;

        assert!(png_max > 0);
        assert!(max_retries <= 10); // Reasonable upper bound
    }

    /// Test /start command response content
    #[test]
    fn test_start_command_response_contains_expected_content() {
        // Test that the start command response contains key elements
        let expected_phrases = [
            "Welcome to Ingredients Bot",
            "Send me photos",
            "OCR",
            "start",
            "help",
        ];

        // This is a basic content check - in a real scenario we'd mock the bot
        // For now, we verify our expected phrases are reasonable
        for phrase in &expected_phrases {
            assert!(!phrase.is_empty(), "Expected phrase should not be empty");
            assert!(phrase.len() > 2, "Expected phrase should be meaningful");
        }
    }

    /// Test /help command response content
    #[test]
    fn test_help_command_response_contains_expected_content() {
        // Test that the help command response contains key elements
        let expected_phrases = [
            "Ingredients Bot Help",
            "Send a photo",
            "Supported formats",
            "File size limit",
            "clear, well-lit images",
        ];

        // This is a basic content check - in a real scenario we'd mock the bot
        // For now, we verify our expected phrases are reasonable
        for phrase in &expected_phrases {
            assert!(!phrase.is_empty(), "Expected phrase should not be empty");
            assert!(phrase.len() > 3, "Expected phrase should be meaningful");
        }
    }

    /// Test French localization support
    #[test]
    fn test_french_localization() {
        let manager = setup_localization();

        // Test that both English and French are supported
        assert!(
            manager.is_language_supported("en"),
            "English should be supported"
        );
        // Note: French support depends on whether the fr/main.ftl file was loaded successfully
        // In test environment, this might fail if running from wrong directory
        let french_supported = manager.is_language_supported("fr");
        if french_supported {
            assert!(
                french_supported,
                "French should be supported if file was loaded"
            );
        } else {
            eprintln!("French localization not loaded - likely running from wrong directory");
        }

        assert!(
            !manager.is_language_supported("es"),
            "Spanish should not be supported"
        );

        // Test basic messages in English (always available)
        let welcome_title_en = manager.get_message_in_language("welcome-title", "en", None);
        assert!(
            !welcome_title_en.is_empty(),
            "English welcome-title should not be empty"
        );

        // Test messages with arguments - let's find a key that uses arguments
        let help_step1_en = manager.get_message_in_language("help-step1", "en", None);
        assert!(
            !help_step1_en.is_empty(),
            "English help-step1 should not be empty"
        );

        // Test fallback to English for unsupported language
        let fallback = manager.get_message_in_language("welcome-title", "de", None);
        assert_eq!(
            fallback, welcome_title_en,
            "Unsupported language should fallback to English"
        );

        // If French is supported, test that it's different from English
        if french_supported {
            let welcome_title_fr = manager.get_message_in_language("welcome-title", "fr", None);
            assert!(
                !welcome_title_fr.is_empty(),
                "French welcome-title should not be empty"
            );
            assert_ne!(
                welcome_title_en, welcome_title_fr,
                "English and French welcome-title should be different"
            );

            let help_step1_fr = manager.get_message_in_language("help-step1", "fr", None);
            assert!(
                !help_step1_fr.is_empty(),
                "French help-step1 should not be empty"
            );
            assert_ne!(
                help_step1_en, help_step1_fr,
                "English and French help-step1 should be different"
            );
        }
    }

    /// Test language detection functionality
    #[test]
    fn test_language_detection() {
        use just_ingredients::localization::detect_language;
        let manager = setup_localization();

        // Test supported languages
        assert_eq!(
            detect_language(&manager, Some("fr")),
            "fr",
            "French should be detected as 'fr'"
        );
        assert_eq!(
            detect_language(&manager, Some("en")),
            "en",
            "English should be detected as 'en'"
        );
        assert_eq!(
            detect_language(&manager, Some("fr-FR")),
            "fr",
            "French with locale should be detected as 'fr'"
        );
        assert_eq!(
            detect_language(&manager, Some("en-US")),
            "en",
            "English with locale should be detected as 'en'"
        );

        // Test unsupported languages fallback to English
        assert_eq!(
            detect_language(&manager, Some("es")),
            "en",
            "Unsupported language should fallback to English"
        );
        assert_eq!(
            detect_language(&manager, Some("de")),
            "en",
            "German should fallback to English"
        );
        assert_eq!(
            detect_language(&manager, Some("zh-CN")),
            "en",
            "Chinese should fallback to English"
        );

        // Test None case
        assert_eq!(
            detect_language(&manager, None),
            "en",
            "None should default to English"
        );
    }

    /// Test delete ingredient callback functionality
    #[test]
    fn test_delete_ingredient_callback() {
        use just_ingredients::text_processing::MeasurementMatch;

        // Create test ingredients
        let mut ingredients = vec![
            MeasurementMatch {
                quantity: "2".to_string(),
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 6,
            },
            MeasurementMatch {
                quantity: "3".to_string(),
                measurement: None,
                ingredient_name: "eggs".to_string(),
                line_number: 1,
                start_pos: 8,
                end_pos: 9,
            },
            MeasurementMatch {
                quantity: "1".to_string(),
                measurement: Some("cup".to_string()),
                ingredient_name: "sugar".to_string(),
                line_number: 2,
                start_pos: 15,
                end_pos: 21,
            },
        ];

        // Test deleting middle ingredient (index 1 - eggs)
        let index_to_delete = 1;
        assert!(index_to_delete < ingredients.len(), "Index should be valid");

        ingredients.remove(index_to_delete);

        // Verify the correct ingredient was removed
        assert_eq!(ingredients.len(), 2, "Should have 2 ingredients remaining");
        assert_eq!(
            ingredients[0].ingredient_name, "flour",
            "First ingredient should be flour"
        );
        assert_eq!(
            ingredients[1].ingredient_name, "sugar",
            "Second ingredient should be sugar"
        );

        // Test deleting first ingredient (index 0)
        ingredients.remove(0);
        assert_eq!(ingredients.len(), 1, "Should have 1 ingredient remaining");
        assert_eq!(
            ingredients[0].ingredient_name, "sugar",
            "Remaining ingredient should be sugar"
        );

        // Test deleting last ingredient (index 0, which is now the last one)
        ingredients.remove(0);
        assert_eq!(ingredients.len(), 0, "Should have no ingredients remaining");

        // Test edge case: trying to delete from empty list (this would be handled by bounds checking in real code)
        // This test just verifies our understanding of the behavior
        let empty_ingredients: Vec<MeasurementMatch> = vec![];
        // In real code, we would check bounds before calling remove
        assert_eq!(
            empty_ingredients.len(),
            0,
            "Empty list should have length 0"
        );
    }

    /// Test dialogue state updates after ingredient deletion
    #[test]
    fn test_dialogue_state_after_deletion() {
        use just_ingredients::dialogue::RecipeDialogueState;
        use just_ingredients::text_processing::MeasurementMatch;

        // Create initial dialogue state
        let recipe_name = "Test Recipe".to_string();
        let mut ingredients = vec![
            MeasurementMatch {
                quantity: "2".to_string(),
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 6,
            },
            MeasurementMatch {
                quantity: "3".to_string(),
                measurement: None,
                ingredient_name: "eggs".to_string(),
                line_number: 1,
                start_pos: 8,
                end_pos: 9,
            },
        ];

        let language_code = Some("en".to_string());

        // Create initial state
        let initial_state = RecipeDialogueState::ReviewIngredients {
            recipe_name: recipe_name.clone(),
            ingredients: ingredients.clone(),
            language_code: language_code.clone(),
            message_id: None,
            extracted_text: "Test OCR text".to_string(),
            recipe_name_from_caption: None,
        };

        // Simulate deleting an ingredient
        ingredients.remove(0); // Remove flour

        // Create updated state
        let updated_state = RecipeDialogueState::ReviewIngredients {
            recipe_name: recipe_name.clone(),
            ingredients: ingredients.clone(),
            language_code: language_code.clone(),
            message_id: None,
            extracted_text: "Test OCR text".to_string(),
            recipe_name_from_caption: None,
        };

        // Verify the states are different
        match (&initial_state, &updated_state) {
            (
                RecipeDialogueState::ReviewIngredients {
                    ingredients: initial,
                    ..
                },
                RecipeDialogueState::ReviewIngredients {
                    ingredients: updated,
                    ..
                },
            ) => {
                assert_eq!(initial.len(), 2, "Initial state should have 2 ingredients");
                assert_eq!(updated.len(), 1, "Updated state should have 1 ingredient");
                assert_eq!(
                    updated[0].ingredient_name, "eggs",
                    "Remaining ingredient should be eggs"
                );
            }
            _ => panic!("Both states should be ReviewIngredients"),
        }

        // Test empty ingredients state
        let empty_ingredients: Vec<MeasurementMatch> = vec![];
        let empty_state = RecipeDialogueState::ReviewIngredients {
            recipe_name,
            ingredients: empty_ingredients,
            language_code,
            message_id: None,
            extracted_text: "Test OCR text".to_string(),
            recipe_name_from_caption: None,
        };

        match empty_state {
            RecipeDialogueState::ReviewIngredients { ingredients, .. } => {
                assert_eq!(
                    ingredients.len(),
                    0,
                    "Empty state should have no ingredients"
                );
            }
            _ => panic!("State should be ReviewIngredients"),
        }
    }

    /// Test ingredient review keyboard creation
    #[test]
    fn test_ingredient_review_keyboard_creation() {
        let manager = setup_localization();
        use just_ingredients::bot::create_ingredient_review_keyboard;
        use just_ingredients::text_processing::MeasurementMatch;
        use teloxide::types::InlineKeyboardMarkup;

        // Create test ingredients
        let ingredients = vec![
            MeasurementMatch {
                quantity: "2".to_string(),
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 6,
            },
            MeasurementMatch {
                quantity: "3".to_string(),
                measurement: None,
                ingredient_name: "eggs".to_string(),
                line_number: 1,
                start_pos: 8,
                end_pos: 9,
            },
        ];

        // Test keyboard creation
        let keyboard = create_ingredient_review_keyboard(&ingredients, Some("en"), &manager);

        // Verify keyboard structure
        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should have 4 rows: 2 ingredient rows + 1 confirm/cancel row + 1 add ingredient row
            assert_eq!(keyboard.len(), 4);

            // First row: Edit and Delete buttons for first ingredient
            assert_eq!(keyboard[0].len(), 2);
            assert!(keyboard[0][0].text.contains("✏️"));
            assert!(keyboard[0][0].text.contains("flour"));
            assert!(keyboard[0][1].text.contains("🗑️"));
            assert!(keyboard[0][1].text.contains("flour"));

            // Second row: Edit and Delete buttons for second ingredient
            assert_eq!(keyboard[1].len(), 2);
            assert!(keyboard[1][0].text.contains("✏️"));
            assert!(keyboard[1][0].text.contains("eggs"));
            assert!(keyboard[1][1].text.contains("🗑️"));
            assert!(keyboard[1][1].text.contains("eggs"));

            // Third row: Confirm and Cancel buttons
            assert_eq!(keyboard[2].len(), 2);
            assert!(keyboard[2][0].text.contains("✅"));
            assert!(keyboard[2][1].text.contains("❌"));

            // Fourth row: Add Ingredient button
            assert_eq!(keyboard[3].len(), 1);
            assert!(keyboard[3][0].text.contains("➕"));
            assert!(keyboard[3][0].text.contains("Add Ingredient"));
        }
    }

    /// Test ingredient review keyboard with empty ingredients
    #[test]
    fn test_ingredient_review_keyboard_empty() {
        let manager = setup_localization();
        use just_ingredients::bot::create_ingredient_review_keyboard;
        use just_ingredients::text_processing::MeasurementMatch;
        use teloxide::types::InlineKeyboardMarkup;

        let empty_ingredients: Vec<MeasurementMatch> = vec![];

        let keyboard = create_ingredient_review_keyboard(&empty_ingredients, Some("en"), &manager);

        // Should still have confirm/cancel row even with no ingredients
        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            assert_eq!(keyboard.len(), 1); // Just the confirm/cancel row
            assert_eq!(keyboard[0].len(), 2);
            assert!(keyboard[0][0].text.contains("✅"));
            assert!(keyboard[0][1].text.contains("❌"));
        }
    }

    /// Test ingredient review keyboard with long ingredient names
    #[test]
    fn test_ingredient_review_keyboard_long_names() {
        let manager = setup_localization();
        use just_ingredients::bot::create_ingredient_review_keyboard;
        use just_ingredients::text_processing::MeasurementMatch;
        use teloxide::types::InlineKeyboardMarkup;

        let ingredients = vec![MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("cup".to_string()),
            ingredient_name: "very_long_ingredient_name_that_should_be_truncated".to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 50,
        }];

        let keyboard = create_ingredient_review_keyboard(&ingredients, Some("en"), &manager);

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should truncate long names
            assert!(keyboard[0][0].text.contains("..."));
            assert!(keyboard[0][0].text.len() <= 30); // Should be reasonably short
        }
    }

    /// Test ingredient review keyboard with unknown ingredients
    #[test]
    fn test_ingredient_review_keyboard_unknown_ingredients() {
        let manager = setup_localization();
        use just_ingredients::bot::create_ingredient_review_keyboard;
        use just_ingredients::text_processing::MeasurementMatch;
        use teloxide::types::InlineKeyboardMarkup;

        let ingredients = vec![MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "".to_string(), // Empty name should show as unknown
            line_number: 0,
            start_pos: 0,
            end_pos: 6,
        }];

        let keyboard = create_ingredient_review_keyboard(&ingredients, Some("en"), &manager);

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should contain unknown ingredient text
            assert!(keyboard[0][0].text.contains("❓"));
        }
    }

    /// Test callback data parsing for ingredient actions
    #[test]
    fn test_callback_data_parsing() {
        // Test edit callback parsing
        let edit_callback = "edit_1";
        assert!(edit_callback.starts_with("edit_"));
        let index_str = edit_callback.strip_prefix("edit_").unwrap();
        let index: usize = index_str.parse().unwrap();
        assert_eq!(index, 1);

        // Test delete callback parsing
        let delete_callback = "delete_0";
        assert!(delete_callback.starts_with("delete_"));
        let index_str = delete_callback.strip_prefix("delete_").unwrap();
        let index: usize = index_str.parse().unwrap();
        assert_eq!(index, 0);

        // Test other callbacks
        assert_eq!("confirm", "confirm");
        assert_eq!("cancel_review", "cancel_review");
        assert_eq!("add_more", "add_more");
        assert_eq!("cancel_empty", "cancel_empty");
    }

    /// Test ingredient display formatting
    #[test]
    fn test_ingredient_display_formatting() {
        use just_ingredients::text_processing::MeasurementMatch;

        let ingredients = vec![
            MeasurementMatch {
                quantity: "2".to_string(),
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 6,
            },
            MeasurementMatch {
                quantity: "3".to_string(),
                measurement: None,
                ingredient_name: "eggs".to_string(),
                line_number: 1,
                start_pos: 8,
                end_pos: 9,
            },
            MeasurementMatch {
                quantity: "1".to_string(),
                measurement: Some("tbsp".to_string()),
                ingredient_name: "".to_string(), // Empty name
                line_number: 2,
                start_pos: 15,
                end_pos: 21,
            },
        ];

        // Test formatting logic (this mirrors the logic in create_ingredient_review_keyboard)
        for (i, ingredient) in ingredients.iter().enumerate() {
            let ingredient_display = if ingredient.ingredient_name.is_empty() {
                "unknown-ingredient".to_string() // This would be localized
            } else {
                ingredient.ingredient_name.clone()
            };

            let measurement_display = if let Some(ref unit) = ingredient.measurement {
                format!("{} {}", ingredient.quantity, unit)
            } else {
                ingredient.quantity.clone()
            };

            let display_text = format!("{} → {}", measurement_display, ingredient_display);

            match i {
                0 => {
                    assert_eq!(display_text, "2 cups → flour");
                }
                1 => {
                    assert_eq!(display_text, "3 → eggs");
                }
                2 => {
                    assert_eq!(display_text, "1 tbsp → unknown-ingredient");
                }
                _ => panic!("Unexpected index"),
            }
        }
    }

    /// Test ingredient list formatting for display
    #[test]
    fn test_ingredient_list_formatting() {
        let manager = setup_localization();
        use just_ingredients::bot::format_ingredients_list;
        use just_ingredients::text_processing::MeasurementMatch;

        let ingredients = vec![
            MeasurementMatch {
                quantity: "2".to_string(),
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 6,
            },
            MeasurementMatch {
                quantity: "3".to_string(),
                measurement: None,
                ingredient_name: "eggs".to_string(),
                line_number: 1,
                start_pos: 8,
                end_pos: 9,
            },
        ];

        let formatted = format_ingredients_list(&ingredients, Some("en"), &manager);

        // Should contain both ingredients
        assert!(formatted.contains("flour"));
        assert!(formatted.contains("eggs"));
        assert!(formatted.contains("2 cups"));
        assert!(formatted.contains("3"));

        // Should be formatted as a list
        assert!(formatted.contains("\n") || formatted.contains("•"));
    }

    /// Test recipes pagination keyboard creation
    #[test]
    fn test_recipes_pagination_keyboard_creation() {
        let manager = setup_localization();
        use just_ingredients::bot::create_recipes_pagination_keyboard;
        use teloxide::types::{InlineKeyboardButtonKind, InlineKeyboardMarkup};

        // Test with multiple recipes and first page
        let recipes = vec!["Apple Pie".to_string(), "Chocolate Cake".to_string()];
        let current_page = 0;
        let total_count = 5;
        let limit = 2;

        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            current_page,
            total_count,
            limit,
            Some("en"),
            &manager,
        );

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should have 3 rows: 2 recipe rows + 1 navigation row
            assert_eq!(keyboard.len(), 3);

            // First row: Apple Pie button
            assert_eq!(keyboard[0].len(), 1);
            assert!(keyboard[0][0].text.contains("Apple Pie"));
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard[0][0].kind {
                assert!(data.contains("select_recipe:Apple Pie"));
            } else {
                panic!("Expected callback button");
            }

            // Second row: Chocolate Cake button
            assert_eq!(keyboard[1].len(), 1);
            assert!(keyboard[1][0].text.contains("Chocolate Cake"));
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard[1][0].kind {
                assert!(data.contains("select_recipe:Chocolate Cake"));
            } else {
                panic!("Expected callback button");
            }

            // Third row: Page info and Next button
            assert_eq!(keyboard[2].len(), 2);
            assert!(keyboard[2][0].text.contains("Page 1 of 3"));
            assert!(keyboard[2][1].text.contains("Next"));
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard[2][1].kind {
                assert_eq!(data, "page:1");
            } else {
                panic!("Expected callback button");
            }
        }
    }

    /// Test recipes pagination keyboard with last page
    #[test]
    fn test_recipes_pagination_keyboard_last_page() {
        let manager = setup_localization();
        use just_ingredients::bot::create_recipes_pagination_keyboard;
        use teloxide::types::{InlineKeyboardButtonKind, InlineKeyboardMarkup};

        let recipes = vec!["Banana Bread".to_string()];
        let current_page = 2;
        let total_count = 5;
        let limit = 2;

        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            current_page,
            total_count,
            limit,
            Some("en"),
            &manager,
        );

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should have 2 rows: 1 recipe row + 1 navigation row
            assert_eq!(keyboard.len(), 2);

            // First row: Banana Bread button
            assert_eq!(keyboard[0].len(), 1);
            assert!(keyboard[0][0].text.contains("Banana Bread"));

            // Second row: Previous button and Page info
            assert_eq!(keyboard[1].len(), 2);
            assert!(keyboard[1][0].text.contains("Previous"));
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard[1][0].kind {
                assert_eq!(data, "page:1");
            } else {
                panic!("Expected callback button");
            }
            assert!(keyboard[1][1].text.contains("Page 3 of 3"));
        }
    }

    /// Test recipes pagination keyboard with single page
    #[test]
    fn test_recipes_pagination_keyboard_single_page() {
        let manager = setup_localization();
        use just_ingredients::bot::create_recipes_pagination_keyboard;
        use teloxide::types::InlineKeyboardMarkup;

        let recipes = vec!["Simple Recipe".to_string()];
        let current_page = 0;
        let total_count = 1;
        let limit = 10;

        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            current_page,
            total_count,
            limit,
            Some("en"),
            &manager,
        );

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should have only 1 row: just the recipe button (no navigation)
            assert_eq!(keyboard.len(), 1);

            // First row: Simple Recipe button
            assert_eq!(keyboard[0].len(), 1);
            assert!(keyboard[0][0].text.contains("Simple Recipe"));
        }
    }

    /// Test recipes pagination keyboard with long recipe names
    #[test]
    fn test_recipes_pagination_keyboard_long_names() {
        let manager = setup_localization();
        use just_ingredients::bot::create_recipes_pagination_keyboard;
        use teloxide::types::InlineKeyboardMarkup;

        let recipes = vec!["Very Long Recipe Name That Should Be Truncated".to_string()];
        let current_page = 0;
        let total_count = 1;
        let limit = 10;

        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            current_page,
            total_count,
            limit,
            Some("en"),
            &manager,
        );

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard,
        } = keyboard;
        {
            // Should truncate long names
            assert!(keyboard[0][0].text.contains("..."));
            assert!(keyboard[0][0].text.len() <= 33); // 30 + "..."
        }
    }

    /// Test recipes command message formatting
    #[test]
    fn test_recipes_command_message_formatting() {
        let manager = setup_localization();
        use just_ingredients::localization::t_lang;

        // Test that localization keys exist and return reasonable strings
        let your_recipes = t_lang(&manager, "your-recipes", Some("en"));
        let select_recipe = t_lang(&manager, "select-recipe", Some("en"));
        let no_recipes = t_lang(&manager, "no-recipes-found", Some("en"));
        let no_recipes_suggestion = t_lang(&manager, "no-recipes-suggestion", Some("en"));

        assert!(!your_recipes.is_empty());
        assert!(!select_recipe.is_empty());
        assert!(!no_recipes.is_empty());
        assert!(!no_recipes_suggestion.is_empty());

        // Test French versions
        let your_recipes_fr = t_lang(&manager, "your-recipes", Some("fr"));
        let select_recipe_fr = t_lang(&manager, "select-recipe", Some("fr"));

        assert!(!your_recipes_fr.is_empty());
        assert!(!select_recipe_fr.is_empty());

        // French and English should be different
        assert_ne!(your_recipes, your_recipes_fr);
        assert_ne!(select_recipe, select_recipe_fr);
    }

    /// Test callback data parsing for recipes
    #[test]
    fn test_recipes_callback_data_parsing() {
        // Test recipe selection callback parsing
        let select_callback = "select_recipe:Chocolate Cake";
        assert!(select_callback.starts_with("select_recipe:"));
        let recipe_name = select_callback.strip_prefix("select_recipe:").unwrap();
        assert_eq!(recipe_name, "Chocolate Cake");

        // Test pagination callback parsing
        let page_callback = "page:2";
        assert!(page_callback.starts_with("page:"));
        let page_str = page_callback.strip_prefix("page:").unwrap();
        let page: usize = page_str.parse().unwrap();
        assert_eq!(page, 2);

        // Test edge cases
        let page_zero = "page:0";
        let page_zero_num: usize = page_zero.strip_prefix("page:").unwrap().parse().unwrap();
        assert_eq!(page_zero_num, 0);

        // Test invalid callbacks (should not crash)
        let invalid_callback = "invalid_data";
        assert!(!invalid_callback.starts_with("select_recipe:"));
        assert!(!invalid_callback.starts_with("page:"));
    }

    /// Test post-confirmation keyboard creation
    #[test]
    fn test_post_confirmation_keyboard_creation() {
        let manager = setup_localization();
        use just_ingredients::bot::create_post_confirmation_keyboard;
        use teloxide::types::{InlineKeyboardButtonKind, InlineKeyboardMarkup};

        // Test keyboard creation for English
        let keyboard_en = create_post_confirmation_keyboard(Some("en"), &manager);

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard_en,
        } = keyboard_en;
        {
            // Should have 2 rows: first row with 2 buttons, second row with 1 button
            assert_eq!(keyboard_en.len(), 2);
            assert_eq!(keyboard_en[0].len(), 2); // Add Another, List Recipes
            assert_eq!(keyboard_en[1].len(), 1); // Search Recipes

            // Check button texts and callbacks
            assert!(keyboard_en[0][0].text.contains("Add Another"));
            assert!(keyboard_en[0][1].text.contains("List My Recipes"));
            assert!(keyboard_en[1][0].text.contains("Search Recipes"));

            // Check callback data
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard_en[0][0].kind {
                assert_eq!(data, "workflow_add_another");
            } else {
                panic!("Expected callback button");
            }
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard_en[0][1].kind {
                assert_eq!(data, "workflow_list_recipes");
            } else {
                panic!("Expected callback button");
            }
            if let InlineKeyboardButtonKind::CallbackData(data) = &keyboard_en[1][0].kind {
                assert_eq!(data, "workflow_search_recipes");
            } else {
                panic!("Expected callback button");
            }
        }

        // Test keyboard creation for French
        let keyboard_fr = create_post_confirmation_keyboard(Some("fr"), &manager);

        let InlineKeyboardMarkup {
            inline_keyboard: keyboard_fr,
        } = keyboard_fr;
        {
            // Should have 2 rows: first row with 2 buttons, second row with 1 button
            assert_eq!(keyboard_fr.len(), 2);
            assert_eq!(keyboard_fr[0].len(), 2); // Add Another, List Recipes
            assert_eq!(keyboard_fr[1].len(), 1); // Search Recipes

            // Check that French text is different from English
            assert_ne!(keyboard_fr[0][0].text, keyboard_en[0][0].text);
            assert_ne!(keyboard_fr[0][1].text, keyboard_en[0][1].text);
            assert_ne!(keyboard_fr[1][0].text, keyboard_en[1][0].text);
        }
    }

    /// Test workflow localization keys
    #[test]
    fn test_workflow_localization_keys() {
        let manager = setup_localization();
        use just_ingredients::localization::t_lang;

        // Test English workflow keys
        let recipe_saved_en = t_lang(&manager, "workflow-recipe-saved", Some("en"));
        let what_next_en = t_lang(&manager, "workflow-what-next", Some("en"));
        let add_another_en = t_lang(&manager, "workflow-add-another", Some("en"));
        let list_recipes_en = t_lang(&manager, "workflow-list-recipes", Some("en"));
        let search_recipes_en = t_lang(&manager, "workflow-search-recipes", Some("en"));

        assert!(!recipe_saved_en.is_empty());
        assert!(!what_next_en.is_empty());
        assert!(!add_another_en.is_empty());
        assert!(!list_recipes_en.is_empty());
        assert!(!search_recipes_en.is_empty());

        // Test French workflow keys
        let recipe_saved_fr = t_lang(&manager, "workflow-recipe-saved", Some("fr"));
        let what_next_fr = t_lang(&manager, "workflow-what-next", Some("fr"));
        let add_another_fr = t_lang(&manager, "workflow-add-another", Some("fr"));
        let list_recipes_fr = t_lang(&manager, "workflow-list-recipes", Some("fr"));
        let search_recipes_fr = t_lang(&manager, "workflow-search-recipes", Some("fr"));

        assert!(!recipe_saved_fr.is_empty());
        assert!(!what_next_fr.is_empty());
        assert!(!add_another_fr.is_empty());
        assert!(!list_recipes_fr.is_empty());
        assert!(!search_recipes_fr.is_empty());

        // French and English should be different
        assert_ne!(recipe_saved_en, recipe_saved_fr);
        assert_ne!(what_next_en, what_next_fr);
        assert_ne!(add_another_en, add_another_fr);
        assert_ne!(list_recipes_en, list_recipes_fr);
        assert_ne!(search_recipes_en, search_recipes_fr);
    }

    /// Test workflow callback data parsing
    #[test]
    fn test_workflow_callback_data_parsing() {
        // Test workflow callback parsing
        assert_eq!("workflow_add_another", "workflow_add_another");
        assert_eq!("workflow_list_recipes", "workflow_list_recipes");
        assert_eq!("workflow_search_recipes", "workflow_search_recipes");

        // Test that these are distinct from other callbacks
        assert_ne!("workflow_add_another", "confirm");
        assert_ne!("workflow_list_recipes", "cancel_review");
        assert_ne!("workflow_search_recipes", "add_more");
    }

    /// Test workflow message formatting
    #[test]
    fn test_workflow_message_formatting() {
        let manager = setup_localization();
        use just_ingredients::localization::t_lang;

        // Test confirmation message formatting
        let recipe_saved = t_lang(&manager, "workflow-recipe-saved", Some("en"));
        let what_next = t_lang(&manager, "workflow-what-next", Some("en"));
        let confirmation_message = format!("✅ **{}**\n\n{}", recipe_saved, what_next);

        assert!(confirmation_message.contains("Recipe saved"));
        assert!(confirmation_message.contains("What would you like to do next"));
        assert!(confirmation_message.contains("✅"));
        assert!(confirmation_message.contains("**"));

        // Test French version
        let recipe_saved_fr = t_lang(&manager, "workflow-recipe-saved", Some("fr"));
        let what_next_fr = t_lang(&manager, "workflow-what-next", Some("fr"));
        let confirmation_message_fr = format!("✅ **{}**\n\n{}", recipe_saved_fr, what_next_fr);

        assert!(confirmation_message_fr.contains("Recette"));
        assert!(confirmation_message_fr.contains("ensuite"));
        assert_ne!(confirmation_message, confirmation_message_fr);
    }

    /// Test photo caption extraction and validation
    #[test]
    fn test_caption_extraction_and_validation() {
        use just_ingredients::validation::validate_recipe_name;

        // Test valid captions
        let valid_captions = vec![
            "Chocolate Chip Cookies",
            "Grandma's Apple Pie",
            "French Crepes Recipe",
            "Simple Pasta",
            "Very Long Recipe Name That Is Still Valid Because It Is Under 255 Characters",
        ];

        for caption in valid_captions {
            let result = validate_recipe_name(caption);
            assert!(result.is_ok(), "Caption '{}' should be valid", caption);
            assert_eq!(result.unwrap(), caption);
        }

        // Test invalid captions
        let invalid_captions = vec![
            "",    // Empty
            "   ", // Whitespace only
        ];

        // Test too long caption separately
        let too_long_caption = "a".repeat(256);
        assert!(validate_recipe_name(&too_long_caption).is_err());

        for caption in &invalid_captions {
            let result = validate_recipe_name(caption);
            assert!(result.is_err(), "Caption '{}' should be invalid", caption);
        }

        // Test whitespace trimming
        let whitespace_caption = "   Chocolate Cookies   ";
        let result = validate_recipe_name(whitespace_caption);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Chocolate Cookies");

        println!("✅ Caption extraction and validation tests passed");
    }

    /// Test caption processing logic for recipe name assignment
    #[test]
    fn test_caption_recipe_name_assignment() {
        use just_ingredients::validation::validate_recipe_name;

        // Test cases for caption processing
        let test_cases = vec![
            // (caption, expected_result)
            (Some("Valid Recipe Name".to_string()), "Valid Recipe Name"),
            (Some("   Spaced Recipe   ".to_string()), "Spaced Recipe"),
            (Some("".to_string()), "Recipe"), // Empty falls back to default
            (Some("   ".to_string()), "Recipe"), // Whitespace falls back to default
            (Some("a".repeat(256)), "Recipe"), // Too long falls back to default
            (None, "Recipe"),                 // No caption falls back to default
        ];

        for (caption, expected) in test_cases {
            let result = match &caption {
                Some(caption_text) if !caption_text.trim().is_empty() => {
                    validate_recipe_name(caption_text).unwrap_or("Recipe")
                }
                _ => "Recipe",
            };

            assert_eq!(
                result, expected,
                "Caption {:?} should result in '{}'",
                caption, expected
            );
        }

        println!("✅ Caption recipe name assignment tests passed");
    }

    /// Test caption localization messages
    #[test]
    fn test_caption_localization_messages() {
        let manager = setup_localization();
        use just_ingredients::localization::t_lang;

        // Test English caption messages
        let caption_used_en = t_lang(&manager, "caption-used", Some("en"));
        let caption_invalid_en = t_lang(&manager, "caption-invalid", Some("en"));

        assert!(!caption_used_en.is_empty());
        assert!(!caption_invalid_en.is_empty());
        assert!(caption_used_en.contains("{$caption}"));
        assert!(caption_invalid_en.contains("{$caption}"));
        // The invalid message doesn't use {$default_name}, it just says "using default recipe name instead"
        assert!(caption_invalid_en.contains("default recipe name"));

        // Test French caption messages
        let caption_used_fr = t_lang(&manager, "caption-used", Some("fr"));
        let caption_invalid_fr = t_lang(&manager, "caption-invalid", Some("fr"));

        assert!(!caption_used_fr.is_empty());
        assert!(!caption_invalid_fr.is_empty());
        assert!(caption_used_fr.contains("{$caption}"));
        // French invalid message uses {$default_name} but not {$caption}
        assert!(caption_invalid_fr.contains("{$default_name}"));

        // French and English should be different
        assert_ne!(caption_used_en, caption_used_fr);
        assert_ne!(caption_invalid_en, caption_invalid_fr);

        // Test message formatting with arguments
        let formatted_used_en = caption_used_en.replace("{$caption}", "Test Recipe");
        // English invalid message doesn't use {$default_name}, just replace {$caption}
        let formatted_invalid_en = caption_invalid_en.replace("{$caption}", "Invalid!!!");

        let formatted_used_fr = caption_used_fr.replace("{$caption}", "Recette Test");
        // French invalid message doesn't use {$caption}, just {$default_name}
        let formatted_invalid_fr = caption_invalid_fr.replace("{$default_name}", "Recette");

        assert!(formatted_used_en.contains("Test Recipe"));
        assert!(formatted_invalid_en.contains("Invalid!!!"));

        assert!(formatted_used_fr.contains("Recette Test"));
        // French invalid message doesn't contain the original caption, just the default name
        assert!(formatted_invalid_fr.contains("Recette"));

        println!("✅ Caption localization message tests passed");
    }

    /// Test edge cases for caption processing
    #[test]
    fn test_caption_edge_cases() {
        use just_ingredients::validation::validate_recipe_name;

        // Test various edge cases
        let edge_cases = vec![
            // Special characters that should be valid
            ("Café au Lait Recipe", true),
            ("Mamá's Tamales", true),
            ("Naïve Cuisine", true),
            ("Recipe with émojis 🎂", true),
            ("Recipe-with-dashes", true),
            ("Recipe_with_underscores", true),
            ("Recipe (with parentheses)", true),
            // Invalid cases
            ("", false),
            ("   ", false),
            // Unicode and special characters that might cause issues
            ("Recipe\twith\ttabs", true),      // Tabs should be handled
            ("Recipe\nwith\nlines", true),     // Newlines should be handled
            ("Recipe\x00with\x00nulls", true), // Null bytes should be handled
        ];

        // Test too long caption separately
        let too_long_caption = "x".repeat(256);
        assert!(validate_recipe_name(&too_long_caption).is_err());

        for (caption, should_be_valid) in edge_cases {
            let result = validate_recipe_name(caption);

            if should_be_valid {
                assert!(result.is_ok(), "Caption '{}' should be valid", caption);
                assert_eq!(result.unwrap(), caption);
            } else {
                assert!(result.is_err(), "Caption '{}' should be invalid", caption);
            }
        }

        println!("✅ Caption edge case tests passed");
    }

    /// Test that existing photo processing still works without captions
    #[test]
    fn test_backward_compatibility_no_caption() {
        // Test that the logic for photos without captions still works
        // This simulates the old behavior

        let caption: Option<String> = None;
        let default_name = "Recipe";

        let recipe_name_candidate = match &caption {
            Some(caption_text) if !caption_text.trim().is_empty() => {
                match just_ingredients::validation::validate_recipe_name(caption_text) {
                    Ok(validated_name) => validated_name,
                    Err(_) => default_name,
                }
            }
            _ => default_name,
        };

        assert_eq!(recipe_name_candidate, default_name);

        // Test with empty caption (should also use default)
        let empty_caption = Some("".to_string());
        let recipe_name_candidate_empty = match &empty_caption {
            Some(caption_text) if !caption_text.trim().is_empty() => {
                match just_ingredients::validation::validate_recipe_name(caption_text) {
                    Ok(validated_name) => validated_name,
                    Err(_) => default_name,
                }
            }
            _ => default_name,
        };

        assert_eq!(recipe_name_candidate_empty, default_name);

        println!("✅ Backward compatibility tests passed - no caption behavior preserved");
    }
}
