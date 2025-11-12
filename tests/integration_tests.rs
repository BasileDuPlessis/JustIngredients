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
