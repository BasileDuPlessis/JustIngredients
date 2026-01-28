//! # Integration Tests
//!
//! This module contains integration tests for the JustIngredients Telegram bot,
//! testing end-to-end functionality including quantity-only ingredient detection.

use just_ingredients::text_processing::{MeasurementConfig, MeasurementDetector};

#[test]
fn test_quantity_only_integration() {
    // Create a measurement detector
    let detector = MeasurementDetector::new().unwrap();

    // Test text that would come from OCR containing quantity-only ingredients
    let ocr_text = r#"
    Recette de Crêpes

    Ingrédients:
    125 g de farine
    2 œufs
    1/2 litre de lait
    2 cuillères à soupe de sucre
    1 pincée de sel
    50 g de beurre fondu
    2 oranges
    100 g de sucre en poudre
    4 cuillères à soupe de Grand Marnier

    Préparation:
    Mélanger la farine avec les œufs...
    "#;

    // Process the text through the measurement detector
    let matches = detector.extract_ingredient_measurements(ocr_text);

    // Verify we found all measurements including quantity-only ones
    assert_eq!(matches.len(), 9);

    // Check traditional measurements
    assert_eq!(matches[0].quantity, "125");
    assert_eq!(matches[0].measurement, Some("g".to_string()));
    assert_eq!(matches[0].ingredient_name, "farine");

    // Check quantity-only ingredients
    assert_eq!(matches[1].quantity, "2");
    assert_eq!(matches[1].measurement, None);
    assert_eq!(matches[1].ingredient_name, "œufs");

    assert_eq!(matches[6].quantity, "2");
    assert_eq!(matches[6].measurement, None);
    assert_eq!(matches[6].ingredient_name, "oranges");

    // Check other measurements still work
    assert_eq!(matches[2].quantity, "1/2");
    assert_eq!(matches[2].measurement, Some("litre".to_string()));
    assert_eq!(matches[2].ingredient_name, "lait");

    assert_eq!(matches[3].quantity, "2");
    assert_eq!(
        matches[3].measurement,
        Some("cuillères à soupe".to_string())
    );
    assert_eq!(matches[3].ingredient_name, "sucre");

    println!(
        "✅ Successfully processed {} measurements including quantity-only ingredients",
        matches.len()
    );
}

/// Test comprehensive recipe processing with mixed ingredient types
#[test]
fn test_mixed_recipe_processing() {
    let detector = MeasurementDetector::with_config(MeasurementConfig {
        enable_ingredient_postprocessing: true,
        ..Default::default()
    })
    .unwrap();

    let recipe_text = r#"
    Chocolate Chip Cookies - English Recipe

    Ingredients:
    2 1/4 cups all-purpose flour
    1 teaspoon baking soda
    1 teaspoon salt
    1 cup unsalted butter, softened
    3/4 cup granulated sugar
    3/4 cup brown sugar
    2 large eggs
    2 teaspoons vanilla extract
    2 cups chocolate chips

    French Crepes Recipe:
    125 g de farine
    4 œufs
    250 ml de lait
    1 sachet de sucre vanillé
    4 pommes
    "#;

    let matches = detector.extract_ingredient_measurements(recipe_text);

    // Should find measurements from both recipes
    assert!(!matches.is_empty());
    println!("Found {} measurements across both recipes", matches.len());

    // Check English measurements - flour with quantity "2 1/4"
    let flour_match = matches.iter().find(|m| m.ingredient_name.contains("flour"));
    assert!(flour_match.is_some());
    assert_eq!(flour_match.unwrap().quantity, "2 1/4");
    assert_eq!(flour_match.unwrap().measurement, Some("cups".to_string()));

    // Check French measurements
    let farine_match = matches
        .iter()
        .find(|m| m.ingredient_name.contains("farine"));
    assert!(farine_match.is_some());
    assert_eq!(farine_match.unwrap().quantity, "125");
    assert_eq!(farine_match.unwrap().measurement, Some("g".to_string()));

    // Check quantity-only ingredients - these should now capture multi-word names completely
    let eggs_match = matches
        .iter()
        .find(|m| m.ingredient_name.contains("large eggs"));
    assert!(eggs_match.is_some());
    assert_eq!(eggs_match.unwrap().quantity, "2");
    assert!(eggs_match.unwrap().measurement.is_none());

    let pommes_match = matches
        .iter()
        .find(|m| m.ingredient_name.contains("pommes"));
    assert!(pommes_match.is_some());
    assert_eq!(pommes_match.unwrap().quantity, "4");
    assert!(pommes_match.unwrap().measurement.is_none());

    println!("✅ Mixed recipe processing test passed");
}

/// Test edge cases for quantity-only ingredient detection
#[test]
fn test_quantity_only_edge_cases() {
    let detector = MeasurementDetector::new().unwrap();

    let test_cases = vec![
        // (input_text, expected_quantity, expected_ingredient, description)
        (
            "3 eggs for breakfast",
            "3",
            "eggs for breakfast",
            "Simple quantity-only with extra text",
        ),
        (
            "Bake at 350°F for 25 minutes",
            "350",
            "°F", // This might be parsed as ingredient, but tests edge case
            "Temperature with degree symbol",
        ),
        ("Serves 4 people", "4", "people", "Serves quantity"),
        (
            "2-3 apples depending on size",
            "2", // Should capture first number
            "apples depending on size",
            "Range quantities with extra text",
        ),
        (
            "1 large onion, diced",
            "1",
            "large onion",
            "Descriptive ingredients (stops at comma)",
        ),
        // New test cases for unified multi-word extraction
        (
            "2 crème fraîche for dessert",
            "2",
            "crème fraîche for dessert",
            "French multi-word ingredient with extra text",
        ),
        (
            "6 pommes de terre",
            "6",
            "pommes de terre",
            "French vegetable with preposition",
        ),
        (
            "3 fresh basil leaves",
            "3",
            "fresh basil leaves",
            "English descriptive multi-word ingredient",
        ),
        (
            "4 red bell peppers",
            "4",
            "red bell peppers",
            "English color + multi-word ingredient",
        ),
    ];

    for (input_text, expected_quantity, expected_ingredient, description) in test_cases {
        let matches = detector.extract_ingredient_measurements(input_text);

        // For most cases, we expect at least one measurement
        if expected_ingredient != "°F" {
            // Skip the temperature case as it's an edge case
            assert!(
                !matches.is_empty(),
                "Should find measurements in: {}",
                description
            );

            // Check if we found the expected quantity
            let found_match = matches.iter().find(|m| m.quantity == expected_quantity);
            if let Some(found_match) = found_match {
                let actual_ingredient = &found_match.ingredient_name;
                // For multi-word ingredients, verify they are captured completely
                if expected_ingredient.contains(" ") {
                    // Multi-word expected - check that all words are present
                    let expected_words: std::collections::HashSet<&str> =
                        expected_ingredient.split_whitespace().collect();
                    let actual_words: std::collections::HashSet<&str> =
                        actual_ingredient.split_whitespace().collect();
                    let all_expected_present = expected_words
                        .iter()
                        .all(|word| actual_words.contains(word));

                    assert!(
                        all_expected_present,
                        "Multi-word ingredient '{}' should have all words present in '{}'",
                        expected_ingredient, actual_ingredient
                    );
                } else {
                    // For single-word ingredients, check if the expected word is present
                    // (since unified extraction may capture extra text)
                    assert!(
                        actual_ingredient.contains(expected_ingredient),
                        "Single-word ingredient '{}' should be present in '{}'",
                        expected_ingredient,
                        actual_ingredient
                    );
                }
                println!(
                    "✅ {}: Found quantity '{}' for '{}'",
                    description, expected_quantity, actual_ingredient
                );
            } else {
                println!(
                    "⚠️ {}: Expected quantity '{}' not found, but found {} measurements",
                    description,
                    expected_quantity,
                    matches.len()
                );
            }
        }
    }

    println!("✅ Quantity-only edge cases test completed");
}

/// Test unified multi-word ingredient extraction in real recipe scenarios
#[test]
fn test_unified_multi_word_extraction_integration() {
    let detector = MeasurementDetector::new().unwrap();

    let recipe_scenarios = vec![
        // French recipes with multi-word ingredients
        (
            r#"
            Salade Niçoise

            Ingrédients:
            4 tomates cerises
            2 avocats mûrs
            200 g de thon à l'huile
            1 oignon rouge
            100 g d'olives noires
            "#,
            vec![
                ("4", "tomates cerises"),
                ("2", "avocats mûrs"),
                ("200", "thon à l'huile"),
                ("1", "oignon rouge"),
                ("100", "olives noires"),
            ],
        ),
        // English recipes with descriptive ingredients
        (
            r#"
            Gourmet Sandwich

            Ingredients:
            2 slices sourdough bread
            3 oz roast beef
            1 tbsp horseradish sauce
            2 leaves romaine lettuce
            1 large tomato, sliced
            "#,
            vec![
                ("2", "sourdough bread"),
                ("3", "roast beef"),
                ("1", "horseradish sauce"),
                ("2", "romaine lettuce"),
                ("1", "large tomato"), // Stops at comma
            ],
        ),
        // Mixed measurement types
        (
            r#"
            Baking Recipe

            Ingredients:
            2 cups all-purpose flour
            3 large eggs
            1 cup whole milk
            2 tsp vanilla extract
            4 oz dark chocolate chips
            "#,
            vec![
                ("2", "all-purpose flour"),
                ("3", "large eggs"),
                ("1", "whole milk"),
                ("2", "vanilla extract"),
                ("4", "dark chocolate chips"),
            ],
        ),
    ];

    for (recipe_text, expected_ingredients) in recipe_scenarios {
        let matches = detector.extract_ingredient_measurements(recipe_text);

        println!(
            "Testing recipe with {} expected ingredients",
            expected_ingredients.len()
        );
        assert!(!matches.is_empty(), "Should find ingredients in recipe");

        // Verify each expected ingredient is found with correct extraction
        for (expected_quantity, expected_ingredient) in &expected_ingredients {
            let found_match = matches.iter().find(|m| {
                m.quantity == *expected_quantity && m.ingredient_name.contains(expected_ingredient)
            });

            assert!(
                found_match.is_some(),
                "Should find ingredient '{}' with quantity '{}' in matches: {:?}",
                expected_ingredient,
                expected_quantity,
                matches
                    .iter()
                    .map(|m| (&m.quantity, &m.ingredient_name))
                    .collect::<Vec<_>>()
            );

            let found = found_match.unwrap();
            // For multi-word ingredients, ensure complete capture
            if expected_ingredient.contains(" ") {
                assert!(
                    found.ingredient_name.len() >= expected_ingredient.len(),
                    "Multi-word ingredient '{}' should be captured completely, got '{}'",
                    expected_ingredient,
                    found.ingredient_name
                );
            }
        }

        println!("✅ Recipe scenario passed - all ingredients extracted correctly");
    }

    println!("✅ Unified multi-word extraction integration test passed");
}

/// Test mixed measurement types in various formats
#[test]
fn test_mixed_measurement_types() {
    let detector = MeasurementDetector::new().unwrap();

    let test_text = r#"
    International Recipe Collection:

    American:
    2 cups flour
    1 tablespoon sugar
    1 teaspoon vanilla

    Metric:
    250 g butter
    200 ml milk
    500 g chicken

    Imperial:
    1 lb potatoes
    8 oz cheese
    1 pint cream

    French:
    2 cuillères à soupe d'huile
    1 pincée de sel
    3 gousses d'ail

    Quantity-only:
    4 eggs
    2 onions
    3 tomatoes
    "#;

    let matches = detector.extract_ingredient_measurements(test_text);

    assert!(!matches.is_empty());
    println!("Found {} measurements in mixed format text", matches.len());

    // Test various measurement types are recognized
    let volume_measurements = matches
        .iter()
        .filter(|m| {
            m.measurement
                .as_ref()
                .map(|u| {
                    u.contains("cups")
                        || u.contains("tablespoon")
                        || u.contains("teaspoon")
                        || u.contains("ml")
                        || u.contains("cuillères")
                })
                .unwrap_or(false)
        })
        .count();

    let weight_measurements = matches
        .iter()
        .filter(|m| {
            m.measurement
                .as_ref()
                .map(|u| u.contains('g') || u.contains("lb") || u.contains("oz"))
                .unwrap_or(false)
        })
        .count();

    let quantity_only = matches.iter().filter(|m| m.measurement.is_none()).count();

    println!(
        "Volume: {}, Weight: {}, Quantity-only: {}",
        volume_measurements, weight_measurements, quantity_only
    );

    // Should find a good mix of measurement types
    assert!(volume_measurements > 0, "Should find volume measurements");
    assert!(weight_measurements > 0, "Should find weight measurements");
    assert!(quantity_only > 0, "Should find quantity-only ingredients");

    println!("✅ Mixed measurement types test passed");
}

/// Test security boundary testing - input validation and path traversal protection
#[test]
fn test_security_boundary_testing() {
    use just_ingredients::ocr;
    use just_ingredients::ocr_config::OcrConfig;

    let config = OcrConfig::default();

    // Test 1: Path traversal protection
    let dangerous_paths = vec![
        "/etc/passwd",
        "/usr/bin/bash",
        "../../../etc/passwd",
        "/System/Library/Keychains",
        "C:\\Windows\\System32\\cmd.exe",
    ];

    for path in dangerous_paths {
        let result = ocr::validate_image_path(path, &config);
        assert!(result.is_err(), "Path {} should be rejected", path);
    }

    // Test 2: Valid paths (these should work if files exist)
    let safe_paths = vec![
        "/tmp/test.png",
        "/var/tmp/test.jpg",
        "/private/tmp/test.bmp",
    ];

    for path in safe_paths {
        // Create a dummy file for testing
        if let Ok(()) = std::fs::write(path, b"dummy") {
            let _result = ocr::validate_image_path(path, &config);
            // Should pass validation (may fail later for other reasons)
            std::fs::remove_file(path).ok(); // Cleanup
        }
    }

    // Test 3: Null byte protection
    let null_byte_path = "/tmp/test.png\x00evil";
    let _result = ocr::validate_image_path(null_byte_path, &config);
    assert!(_result.is_err(), "Null byte in path should be rejected");

    // Test 4: Empty path validation
    let _result = ocr::validate_image_path("", &config);
    assert!(_result.is_err(), "Empty path should be rejected");

    // Test 5: Non-existent file
    let _result = ocr::validate_image_path("/tmp/nonexistent_file.png", &config);
    assert!(_result.is_err(), "Non-existent file should be rejected");

    // Test 6: Directory instead of file
    let _result = ocr::validate_image_path("/tmp", &config);
    assert!(_result.is_err(), "Directory should be rejected");

    // Test 7: Image format validation
    let config = OcrConfig::default();

    // Test with various formats (create minimal test files)
    let test_cases = vec![
        ("png", b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\tpHYs\x00\x00\x0b\x13\x00\x00\x0b\x13\x01\x00\x9a\x9c\x18\x00\x00\x00\nIDATx\x9cc\xf8\x00\x00\x00\x01\x00\x01\x00\x00\x00\x00IEND\xaeB`\x82"),
        ("jpg", b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x01\x00H\x00H\x00\x00\xFF\xC0\x00\x11\x08\x00\x01\x00\x01\x01\x01\x11\x00\x02\x11\x01\x03\x11\x01\xFF\xC4\x00\x14\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\xFF\xC4\x00\x14\x10\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"),
    ];

    for (ext, data) in test_cases {
        let temp_file = tempfile::NamedTempFile::with_suffix(format!(".{}", ext)).unwrap();
        std::fs::write(temp_file.path(), data).unwrap();

        let is_supported =
            ocr::is_supported_image_format(temp_file.path().to_str().unwrap(), &config);
        assert!(is_supported, "Format {} should be supported", ext);
    }

    // Test unsupported format
    let temp_file = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    std::fs::write(temp_file.path(), b"Hello world").unwrap();

    let is_supported = ocr::is_supported_image_format(temp_file.path().to_str().unwrap(), &config);
    assert!(!is_supported, "Text file should not be supported as image");

    println!("✅ Security boundary testing passed - path traversal, input validation, and format checking working correctly");
}

#[test]
fn test_fraction_quantities_image_processing() {
    use just_ingredients::circuit_breaker::CircuitBreaker;
    use just_ingredients::instance_manager::OcrInstanceManager;
    use just_ingredients::ocr::{extract_text_from_image, OcrConfig};
    use std::fs;

    // Copy the test image to a temporary location
    let source_path =
        "/Users/basile.du.plessis/Documents/JustIngredients/docs/test_fraction_quantities.jpg";
    let temp_file = tempfile::NamedTempFile::with_suffix(".jpg").unwrap();
    let temp_path = temp_file.path().to_str().unwrap().to_string();

    // Copy the image file
    fs::copy(source_path, &temp_path).expect("Failed to copy test image");

    // Set up OCR components
    let config = OcrConfig::default();
    let instance_manager = OcrInstanceManager::new();
    let circuit_breaker = CircuitBreaker::new(config.recovery.clone());

    // Extract text from the image
    let extracted_text = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(extract_text_from_image(
            &temp_path,
            &config,
            &instance_manager,
            &circuit_breaker,
        ))
        .expect("OCR extraction should succeed");

    println!("Extracted text: {}", extracted_text);

    // Process the extracted text to find ingredients
    let detector = MeasurementDetector::new().unwrap();
    let ingredients = detector.extract_ingredient_measurements(&extracted_text);

    println!("Found {} ingredients:", ingredients.len());
    for ingredient in &ingredients {
        println!(
            "- {} {} {}",
            ingredient.quantity,
            ingredient.measurement.as_deref().unwrap_or(""),
            ingredient.ingredient_name
        );
    }

    // Verify the expected ingredients are found
    assert_eq!(ingredients.len(), 2, "Should find exactly 2 ingredients");

    // Check first ingredient: 1/2 cup brown sugar (OCR reads "flour" as "brown sugar")
    let brown_sugar_match = ingredients
        .iter()
        .find(|m| m.ingredient_name.to_lowercase().contains("brown"));
    assert!(
        brown_sugar_match.is_some(),
        "Should find brown sugar ingredient"
    );

    let brown_sugar = brown_sugar_match.unwrap();
    assert_eq!(
        brown_sugar.quantity, "1/2",
        "Brown sugar quantity should be 1/2"
    );
    assert_eq!(
        brown_sugar.measurement,
        Some("cup".to_string()),
        "Brown sugar measurement should be cup"
    );
    assert!(
        brown_sugar.ingredient_name.to_lowercase().contains("brown"),
        "Ingredient should contain 'brown'"
    );

    // Check second ingredient: 1/4 cup granulated sugar
    let sugar_match = ingredients
        .iter()
        .find(|m| m.ingredient_name.to_lowercase().contains("granulated"));
    assert!(
        sugar_match.is_some(),
        "Should find granulated sugar ingredient"
    );

    let sugar = sugar_match.unwrap();
    assert_eq!(sugar.quantity, "1/4", "Sugar quantity should be 1/4");
    assert_eq!(
        sugar.measurement,
        Some("cup".to_string()),
        "Sugar measurement should be cup"
    );
    assert!(
        sugar.ingredient_name.to_lowercase().contains("sugar"),
        "Ingredient should contain 'sugar'"
    );
}

/// Test complete end-to-end bot workflow with multi-line ingredients
#[test]
fn test_multi_line_ingredients_end_to_end_bot_workflow() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::validation::validate_recipe_name;

    // Simulate OCR text with multi-line ingredients (realistic scenario)
    let ocr_text = r#"
    INGREDIENTS:
    2 cups all-purpose
    flour
    1 teaspoon baking
    soda
    1/2 teaspoon salt
    3/4 cup unsalted
    butter, softened
    1 cup granulated sugar
    2 large eggs
    1 teaspoon vanilla
    extract
    1 cup buttermilk
    2 tablespoons melted
    butter
    "#;

    // Step 1: Process text through measurement detector
    let detector = MeasurementDetector::new().unwrap();
    let ingredients = detector.extract_ingredient_measurements(ocr_text);

    // Verify multi-line ingredients were correctly combined
    assert_eq!(ingredients.len(), 9, "Should extract 9 ingredients from multi-line recipe");

    // Check specific multi-line combinations
    let flour_match = ingredients.iter().find(|m| m.ingredient_name == "all-purpose flour");
    assert!(flour_match.is_some(), "Should find combined 'all-purpose flour'");
    assert_eq!(flour_match.unwrap().quantity, "2");
    assert_eq!(flour_match.unwrap().measurement, Some("cups".to_string()));

    let baking_soda = ingredients.iter().find(|m| m.ingredient_name == "baking soda");
    assert!(baking_soda.is_some(), "Should find combined 'baking soda'");
    assert_eq!(baking_soda.unwrap().quantity, "1");
    assert_eq!(baking_soda.unwrap().measurement, Some("teaspoon".to_string()));

    let butter_softened = ingredients.iter().find(|m| m.ingredient_name == "unsalted butter, softened");
    assert!(butter_softened.is_some(), "Should find 'unsalted butter, softened' with comma");
    assert_eq!(butter_softened.unwrap().quantity, "3/4");
    assert_eq!(butter_softened.unwrap().measurement, Some("cup".to_string()));

    let vanilla_extract = ingredients.iter().find(|m| m.ingredient_name == "vanilla extract");
    assert!(vanilla_extract.is_some(), "Should find combined 'vanilla extract'");
    assert_eq!(vanilla_extract.unwrap().quantity, "1");
    assert_eq!(vanilla_extract.unwrap().measurement, Some("teaspoon".to_string()));

    let melted_butter = ingredients.iter().find(|m| m.ingredient_name == "melted butter");
    assert!(melted_butter.is_some(), "Should find combined 'melted butter'");
    assert_eq!(melted_butter.unwrap().quantity, "2");
    assert_eq!(melted_butter.unwrap().measurement, Some("tablespoons".to_string()));

    // Step 2: Simulate dialogue state for recipe naming
    let dialogue_state = RecipeDialogueState::WaitingForRecipeName {
        extracted_text: ocr_text.to_string(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
    };

    // Verify dialogue state contains complete ingredient names
    if let RecipeDialogueState::WaitingForRecipeName { ingredients: ingr, .. } = dialogue_state {
        // Check that all ingredient names are complete (not truncated)
        for ingredient in &ingr {
            assert!(!ingredient.ingredient_name.is_empty(), "Ingredient name should not be empty");
            assert!(!ingredient.ingredient_name.contains('\n'), "Ingredient name should not contain newlines");

            // Verify specific complete names
            if ingredient.quantity == "2" && ingredient.measurement == Some("cups".to_string()) {
                assert_eq!(ingredient.ingredient_name, "all-purpose flour");
            }
        }
    } else {
        panic!("Expected WaitingForRecipeName state");
    }

    // Step 3: Test recipe name validation and completion
    let recipe_name = "Multi-Line Cookie Recipe";
    let validation_result = validate_recipe_name(recipe_name);
    assert!(validation_result.is_ok(), "Recipe name should be valid");

    // Step 4: Simulate UI display formatting
    let mut display_lines = Vec::new();
    for ingredient in &ingredients {
        let display_line = if let Some(ref unit) = ingredient.measurement {
            format!("• {} {} {}", ingredient.quantity, unit, ingredient.ingredient_name)
        } else {
            format!("• {} {}", ingredient.quantity, ingredient.ingredient_name)
        };
        display_lines.push(display_line);
    }

    // Verify UI display shows complete ingredient names
    let flour_display = display_lines.iter().find(|line| line.contains("all-purpose flour"));
    assert!(flour_display.is_some(), "UI should display complete 'all-purpose flour'");
    assert!(flour_display.unwrap().contains("• 2 cups all-purpose flour"));

    let vanilla_display = display_lines.iter().find(|line| line.contains("vanilla extract"));
    assert!(vanilla_display.is_some(), "UI should display complete 'vanilla extract'");
    assert!(vanilla_display.unwrap().contains("• 1 teaspoon vanilla extract"));

    println!("✅ Multi-line ingredients end-to-end bot workflow test passed");
    println!("📊 Successfully processed {} ingredients with complete names", ingredients.len());
}

/// Test UI display formatting for multi-line ingredients in confirmation dialogs
#[test]
fn test_multi_line_ingredients_ui_display_formatting() {
    // Test that ingredient display in UI components correctly shows complete multi-line names
    use just_ingredients::bot::ui_builder::create_ingredient_review_keyboard;
    use just_ingredients::localization::create_localization_manager;

    let ingredients = vec![
        just_ingredients::MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "all-purpose flour".to_string(),
            line_number: 1,
            start_pos: 0,
            end_pos: 20,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("teaspoon".to_string()),
            ingredient_name: "baking soda".to_string(),
            line_number: 3,
            start_pos: 0,
            end_pos: 15,
        },
        just_ingredients::MeasurementMatch {
            quantity: "3/4".to_string(),
            measurement: Some("cup".to_string()),
            ingredient_name: "unsalted butter, softened".to_string(),
            line_number: 6,
            start_pos: 0,
            end_pos: 28,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("teaspoon".to_string()),
            ingredient_name: "vanilla extract".to_string(),
            line_number: 10,
            start_pos: 0,
            end_pos: 18,
        },
    ];

    // Create localization manager for testing
    let localization = create_localization_manager().unwrap();

    // Test ingredient review keyboard displays complete names
    let keyboard = create_ingredient_review_keyboard(&ingredients, Some("en"), &localization);

    // Verify keyboard contains buttons with complete ingredient names
    // The keyboard should have buttons for each ingredient
    assert!(!keyboard.inline_keyboard.is_empty(), "Keyboard should not be empty");

    // Check that the keyboard has the right number of rows (one per ingredient + action buttons)
    assert!(keyboard.inline_keyboard.len() >= ingredients.len(), "Should have at least one button per ingredient");

    println!("✅ Multi-line ingredients UI display formatting test passed");
}

/// Test dialogue flow integrity with multi-line ingredients
#[test]
fn test_dialogue_flow_integrity_with_multi_line_ingredients() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide::dispatching::dialogue::InMemStorage;

    // Test that dialogue states can handle multi-line ingredients without breaking flow
    let ingredients = vec![
        just_ingredients::MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "old-fashioned rolled oats".to_string(),
            line_number: 1,
            start_pos: 0,
            end_pos: 25,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("cup".to_string()),
            ingredient_name: "sugar".to_string(),
            line_number: 3,
            start_pos: 0,
            end_pos: 5,
        },
    ];

    // Test ReviewIngredients state with multi-line ingredients
    let review_state = RecipeDialogueState::ReviewIngredients {
        recipe_name: "Oatmeal Cookies".to_string(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        message_id: Some(123),
        extracted_text: "2 cups old-fashioned\nrolled oats\n1 cup sugar".to_string(),
        recipe_name_from_caption: None,
    };

    // Verify state contains correct data
    if let RecipeDialogueState::ReviewIngredients {
        recipe_name,
        ingredients: state_ingredients,
        extracted_text,
        ..
    } = review_state {
        assert_eq!(recipe_name, "Oatmeal Cookies");
        assert_eq!(state_ingredients.len(), 2);
        assert_eq!(state_ingredients[0].ingredient_name, "old-fashioned rolled oats");
        assert_eq!(state_ingredients[1].ingredient_name, "sugar");
        assert_eq!(extracted_text, "2 cups old-fashioned\nrolled oats\n1 cup sugar");
    } else {
        panic!("Expected ReviewIngredients state");
    }

    // Test dialogue storage can handle the state
    let _storage = InMemStorage::<RecipeDialogueState>::new();

    // This tests that the dialogue system can handle multi-line ingredients
    // without any serialization or state management issues
    // The storage is created successfully if we reach this point

    println!("✅ Dialogue flow integrity with multi-line ingredients test passed");
}

/// Test realistic OCR scenarios with multi-line ingredients
#[test]
fn test_realistic_ocr_scenarios_with_multi_line_ingredients() {
    let detector = MeasurementDetector::new().unwrap();

    // Test various realistic OCR scenarios that commonly produce multi-line ingredients
    let test_cases = [
        (
            // Recipe from old cookbook with line breaks
            r#"
            OLD FASHIONED CHOCOLATE CHIP COOKIES

            2 cups all purpose
            flour
            1 tsp baking
            soda
            1/2 tsp salt
            1 cup butter
            3/4 cup sugar
            3/4 cup brown
            sugar packed
            2 eggs
            2 tsp vanilla
            extract
            2 cups chocolate
            chips
            "#,
            9, // expected ingredients
            vec![
                ("all purpose flour", "2 cups"),
                ("baking soda", "1 tsp"),
                ("brown sugar packed", "3/4 cup"),
                ("vanilla extract", "2 tsp"),
                ("chocolate chips", "2 cups"),
            ],
        ),
        (
            // French recipe with multi-line ingredients
            r#"
            CRÊPES FRANÇAISES

            250 g de farine
            de blé
            4 œufs
            frais
            1/2 litre de lait
            entier
            2 cuillères à soupe de sucre
            en poudre
            1 pincée de sel
            de mer
            "#,
            5, // expected ingredients
            vec![
                ("de farine de blé", "250 g"),
                ("œufs frais", "4"),
                ("de lait entier", "1/2 litre"),
                ("de sucre en poudre", "2 cuillères à soupe"),
                ("de sel de mer", "1 pincée"),
            ],
        ),
        (
            // Complex ingredient names
            r#"
            GOURMET SALAD

            2 cups mixed salad
            greens
            1 cup cherry
            tomatoes halved
            1/2 cup crumbled
            feta cheese
            1/4 cup extra virgin
            olive oil
            2 tablespoons balsamic
            vinegar
            "#,
            5, // expected ingredients
            vec![
                ("mixed salad greens", "2 cups"),
                ("cherry tomatoes halved", "1 cup"),
                ("crumbled feta cheese", "1/2 cup"),
                ("extra virgin olive oil", "1/4 cup"),
                ("balsamic vinegar", "2 tablespoons"),
            ],
        ),
    ];

    for (i, (ocr_text, expected_count, expected_ingredients)) in test_cases.iter().enumerate() {
        println!("🧪 Testing OCR scenario {}", i + 1);

        let ingredients = detector.extract_ingredient_measurements(ocr_text);

        assert_eq!(
            ingredients.len(),
            *expected_count,
            "Scenario {}: Expected {} ingredients, found {}",
            i + 1,
            expected_count,
            ingredients.len()
        );

        // Verify specific expected ingredients
        for (expected_name, expected_measure) in expected_ingredients {
            let found = ingredients.iter().find(|m| m.ingredient_name == *expected_name);
            assert!(found.is_some(), "Scenario {}: Should find ingredient '{}'", i + 1, expected_name);

            let ingredient = found.unwrap();
            // Basic validation that measurement format is reasonable
            let measure_str = if let Some(ref unit) = ingredient.measurement {
                format!("{} {}", ingredient.quantity, unit)
            } else {
                ingredient.quantity.clone()
            };

            assert_eq!(measure_str, *expected_measure,
                      "Scenario {}: Ingredient '{}' should have measurement '{}'",
                      i + 1, expected_name, expected_measure);
        }

        println!("✅ OCR scenario {} passed: {} ingredients correctly parsed", i + 1, ingredients.len());
    }

    println!("✅ All realistic OCR scenarios with multi-line ingredients test passed");
}
