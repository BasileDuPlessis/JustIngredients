#[cfg(test)]
mod tests {
    use just_ingredients::text_processing::{MeasurementConfig, MeasurementDetector};

    fn create_detector() -> MeasurementDetector {
        MeasurementDetector::new().unwrap()
    }

    #[test]
    fn test_measurement_detector_creation() {
        let detector = create_detector();
        assert!(!detector.pattern_str().is_empty());
    }

    #[test]
    fn test_basic_measurement_detection() {
        let detector = create_detector();

        // Test basic measurements
        assert!(detector.has_measurements("2 cups flour"));
        assert!(detector.has_measurements("1 tablespoon sugar"));
        assert!(detector.has_measurements("500g butter"));
        assert!(detector.has_measurements("1 kg tomatoes"));
        assert!(detector.has_measurements("250 ml milk"));
    }

    #[test]
    fn test_no_measurement_detection() {
        let detector = create_detector();

        assert!(!detector.has_measurements("some flour"));
        assert!(!detector.has_measurements("add salt"));
        assert!(!detector.has_measurements(""));
    }

    #[test]
    fn test_extract_measurement_lines() {
        let detector = create_detector();
        let text = "2 cups flour\n1 tablespoon sugar\nsome salt\nto taste";

        let lines = detector.extract_measurement_lines(text);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (0, "2 cups flour".to_string()));
        assert_eq!(lines[1], (1, "1 tablespoon sugar".to_string()));
    }

    #[test]
    fn test_find_measurements_with_positions() {
        let detector = create_detector();
        let text = "Mix 2 cups flour with 1 tbsp sugar";

        let matches = detector.extract_ingredient_measurements(text);

        assert_eq!(matches.len(), 2);

        // First match: "2 cups flour"
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");
        assert_eq!(matches[0].line_number, 0);
        assert_eq!(matches[0].start_pos, 4);
        assert_eq!(matches[0].end_pos, 16); // "2 cups flour" ends at position 16

        // Second match: "1 tbsp sugar"
        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("tbsp".to_string()));
        assert_eq!(matches[1].ingredient_name, "sugar");
        assert_eq!(matches[1].line_number, 0);
        assert_eq!(matches[1].start_pos, 22); // "with 1" -> starts after "with "
        assert_eq!(matches[1].end_pos, 34); // "1 tbsp sugar" ends at position 34
    }

    #[test]
    fn test_french_measurements() {
        let detector = create_detector();

        // Test French measurements
        assert!(detector.has_measurements("2 tasses de farine"));
        assert!(detector.has_measurements("1 cuillère à soupe de sucre"));
        assert!(detector.has_measurements("500 g de beurre"));
        assert!(detector.has_measurements("1 kg de tomates"));
    }

    #[test]
    fn test_comprehensive_french_measurements() {
        let detector = create_detector();

        // Test volume measurements
        assert!(detector.has_measurements("2 tasses de lait"));
        assert!(detector.has_measurements("1 cuillère à café de sel"));
        assert!(detector.has_measurements("3 cuillères à soupe d'huile"));
        assert!(detector.has_measurements("250 ml d'eau"));
        assert!(detector.has_measurements("1 litre de jus"));

        // Test weight measurements
        assert!(detector.has_measurements("500 grammes de sucre"));
        assert!(detector.has_measurements("1 kilogramme de pommes"));
        assert!(detector.has_measurements("200 g de chocolat"));

        // Test count measurements (excluding œufs which are ingredients, not measurements)
        assert!(detector.has_measurements("2 tranches de pain"));
        assert!(detector.has_measurements("1 boîte de conserve"));
        assert!(detector.has_measurements("4 morceaux de poulet"));
        assert!(detector.has_measurements("1 sachet de levure"));
        assert!(detector.has_measurements("2 paquets de pâtes"));
        assert!(detector.has_measurements("1 poignée d'amandes"));
        assert!(detector.has_measurements("3 gousses d'ail"));
        assert!(detector.has_measurements("1 brin de persil"));
        assert!(detector.has_measurements("2 feuilles de laurier"));
        assert!(detector.has_measurements("1 bouquet de thym"));
    }

    #[test]
    fn test_abbreviations() {
        let detector = create_detector();

        // Test abbreviations
        assert!(detector.has_measurements("1 tsp salt"));
        assert!(detector.has_measurements("2 tbsp oil"));
        assert!(detector.has_measurements("1 lb beef"));
        assert!(detector.has_measurements("8 oz water"));
    }

    #[test]
    fn test_plural_forms() {
        let detector = create_detector();

        // Test plural forms
        assert!(detector.has_measurements("2 cups"));
        assert!(detector.has_measurements("1 tablespoon"));
        assert!(detector.has_measurements("3 teaspoons"));
        assert!(detector.has_measurements("4 ounces"));
    }

    #[test]
    fn test_decimal_numbers() {
        let detector = create_detector();

        // Test decimal numbers
        assert!(detector.has_measurements("2.5 cups flour"));
        assert!(detector.has_measurements("0.5 kg sugar"));
        assert!(detector.has_measurements("1.25 liters milk"));
    }

    #[test]
    fn test_count_measurements() {
        let detector = create_detector();

        // Test count-based measurements (excluding eggs which are ingredients, not measurements)
        assert!(detector.has_measurements("2 slices bread"));
        assert!(detector.has_measurements("1 can tomatoes"));
        assert!(detector.has_measurements("4 pieces chicken"));
        assert!(detector.has_measurements("3 sachets yeast"));
        assert!(detector.has_measurements("2 paquets pasta"));
    }

    #[test]
    fn test_unique_units_extraction() {
        let detector = create_detector();
        let text = "2 cups flour\n1 cup sugar\n500g butter\n200g flour";

        let units = detector.get_unique_units(text);

        // Should contain the measurement parts
        assert!(units.iter().any(|u| u.contains("cups")));
        assert!(units.iter().any(|u| u.contains("cup")));
        assert!(units.iter().any(|u| u.contains("g")));
    }

    #[test]
    fn test_multi_line_text() {
        let detector = create_detector();
        let text = "Ingredients:\n2 cups flour\n1 tablespoon sugar\n1 teaspoon salt\n\nInstructions:\nMix well";

        let matches = detector.extract_ingredient_measurements(text);

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].line_number, 1); // "2 cups flour"
        assert_eq!(matches[1].line_number, 2); // "1 tablespoon sugar"
        assert_eq!(matches[2].line_number, 3); // "1 teaspoon salt"
    }

    #[test]
    fn test_multi_line_ingredient_integration() {
        let detector = create_detector();
        let text = "Recipe:\n2 cups old-fashioned\nrolled oats\n1 cup sugar\n3 eggs";

        let matches = detector.extract_ingredient_measurements(text);

        assert_eq!(matches.len(), 3);

        // First match: "2 cups old-fashioned rolled oats" (multi-line)
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "old-fashioned rolled oats");
        assert_eq!(matches[0].line_number, 1); // Measurement found on line 1

        // Second match: "1 cup sugar" (single-line)
        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("cup".to_string()));
        assert_eq!(matches[1].ingredient_name, "sugar");
        assert_eq!(matches[1].line_number, 3); // Measurement found on line 3

        // Third match: "3 eggs" (single-line, quantity-only)
        assert_eq!(matches[2].quantity, "3");
        assert_eq!(matches[2].measurement, None);
        assert_eq!(matches[2].ingredient_name, "eggs");
        assert_eq!(matches[2].line_number, 4); // Measurement found on line 4
    }

    #[test]
    fn test_is_measurement_line() {
        let detector = create_detector();

        // Lines that start with measurements
        assert!(detector.is_measurement_line("2 cups flour"));
        assert!(detector.is_measurement_line("1/2 cup sugar"));
        assert!(detector.is_measurement_line("500g butter"));
        assert!(detector.is_measurement_line("6 eggs"));
        assert!(detector.is_measurement_line("1 kg tomatoes"));

        // Lines that don't start with measurements
        assert!(!detector.is_measurement_line("some flour"));
        assert!(!detector.is_measurement_line("add salt"));
        assert!(!detector.is_measurement_line("chopped onions"));
        assert!(!detector.is_measurement_line(""));
        assert!(!detector.is_measurement_line("   2 cups flour")); // leading whitespace
    }

    #[test]
    fn test_is_incomplete_ingredient() {
        let detector = create_detector();

        // Incomplete ingredients (no ending punctuation)
        assert!(detector.is_incomplete_ingredient("old-fashioned rolled"));
        assert!(detector.is_incomplete_ingredient("unsalted butter, cold and"));
        assert!(detector.is_incomplete_ingredient("all-purpose flour"));
        assert!(detector.is_incomplete_ingredient("extra virgin olive oil"));
        assert!(detector.is_incomplete_ingredient("fresh basil"));

        // Complete ingredients (ending punctuation)
        assert!(!detector.is_incomplete_ingredient("flour (all-purpose)"));
        assert!(!detector.is_incomplete_ingredient("sugar."));
        assert!(!detector.is_incomplete_ingredient("salt,"));
        assert!(!detector.is_incomplete_ingredient("butter]"));
        assert!(!detector.is_incomplete_ingredient("cream}"));

        // Edge cases
        assert!(!detector.is_incomplete_ingredient("")); // empty string
        assert!(!detector.is_incomplete_ingredient("   ")); // whitespace only
        assert!(detector.is_incomplete_ingredient("single word"));
    }

    #[test]
    fn test_extract_multi_line_ingredient() {
        let detector = create_detector();

        // Test Case 1 from PRD: Basic multi-line combination
        let lines = ["1 cup old-fashioned rolled", "oats"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "old-fashioned rolled oats");
        assert_eq!(consumed, 2);

        // Test Case 2 from PRD: Multi-line with notes (completes with punctuation)
        let lines = [
            "8 tablespoons unsalted butter, cold and",
            "cubed (See note.)",
        ];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "unsalted butter, cold and cubed (See note.)");
        assert_eq!(consumed, 2);

        // Test Case 3 from PRD: Mixed single and multi-line
        let lines1 = ["2 cups flour"];
        let (ingredient1, consumed1) = detector.extract_multi_line_ingredient(&lines1, 0);
        assert_eq!(ingredient1, "flour");
        assert_eq!(consumed1, 1);

        let lines2 = ["1 cup old-fashioned rolled", "oats"];
        let (ingredient2, consumed2) = detector.extract_multi_line_ingredient(&lines2, 0);
        assert_eq!(ingredient2, "old-fashioned rolled oats");
        assert_eq!(consumed2, 2);

        let lines3 = ["3 eggs"];
        let (ingredient3, consumed3) = detector.extract_multi_line_ingredient(&lines3, 0);
        assert_eq!(ingredient3, "eggs");
        assert_eq!(consumed3, 1);

        // Termination: Empty line
        let lines = ["1 cup old-fashioned rolled", "", "oats"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "old-fashioned rolled");
        assert_eq!(consumed, 1);

        // Termination: Whitespace-only line
        let lines = ["1 cup old-fashioned rolled", "   \t   ", "oats"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "old-fashioned rolled");
        assert_eq!(consumed, 1);

        // Termination: Punctuation-only line
        let lines = ["1 cup old-fashioned rolled", ".", "oats"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "old-fashioned rolled");
        assert_eq!(consumed, 1);

        // Termination: New measurement line
        let lines = ["1 cup old-fashioned rolled", "2 tablespoons sugar"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "old-fashioned rolled");
        assert_eq!(consumed, 1);

        // Single line complete ingredient
        let lines = ["2 cups flour (all-purpose)"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "flour (all-purpose)");
        assert_eq!(consumed, 1);

        // Multi-line that becomes complete mid-way
        let lines = ["8 tablespoons unsalted butter, cold", "and cubed."];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "unsalted butter, cold and cubed.");
        assert_eq!(consumed, 2);

        // Edge case: Very long ingredient (should stop at max limit)
        let lines = [
            "1 cup very long ingredient name that spans",
            "multiple lines and continues for quite",
            "a while with lots of descriptive text",
            "that makes this ingredient extremely",
            "verbose and detailed in its description",
            "requiring many lines to fully express",
            "all the necessary information about",
            "what this ingredient actually represents",
            "in the context of the recipe being parsed",
            "and should eventually be terminated",
            "by the maximum line limit to prevent",
            "runaway processing and memory issues",
        ];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        // Should consume exactly 10 lines (MAX_COMBINE_LINES) and stop
        assert_eq!(consumed, 10);
        // The ingredient should be incomplete (ends without punctuation)
        assert!(detector.is_incomplete_ingredient(&ingredient));

        // Edge case: Out of bounds start index
        let lines = ["1 cup flour"];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 5);
        assert_eq!(ingredient, "");
        assert_eq!(consumed, 0);

        // Edge case: Empty input
        let lines: Vec<&str> = vec![];
        let (ingredient, consumed) = detector.extract_multi_line_ingredient(&lines, 0);
        assert_eq!(ingredient, "");
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_custom_pattern() {
        let pattern = r"\b\d+\s*(?:cups?|tablespoons?)\b";
        let detector = MeasurementDetector::with_pattern(pattern).unwrap();

        assert!(detector.has_measurements("2 cups flour"));
        assert!(detector.has_measurements("1 tablespoon sugar"));
        assert!(!detector.has_measurements("500g butter")); // g not in custom pattern
    }

    #[test]
    fn test_case_insensitive_matching() {
        let detector = create_detector();

        assert!(detector.has_measurements("2 CUPS flour"));
        assert!(detector.has_measurements("1 Tablespoon sugar"));
        assert!(detector.has_measurements("500G butter"));
    }

    #[test]
    fn test_ingredient_name_extraction() {
        let detector = create_detector();

        // Test basic ingredient name extraction
        let matches = detector
            .extract_ingredient_measurements("2 cups flour\n1 tablespoon sugar\n500g butter");

        assert_eq!(matches.len(), 3);

        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");

        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("tablespoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "sugar");

        assert_eq!(matches[2].quantity, "500");
        assert_eq!(matches[2].measurement, Some("g".to_string()));
        assert_eq!(matches[2].ingredient_name, "butter");
    }

    #[test]
    fn test_french_ingredient_name_extraction() {
        let detector = create_detector();

        // Test French ingredient name extraction (with post-processing enabled by default)
        let matches = detector.extract_ingredient_measurements(
            "250 g de farine\n1 litre de lait\n2 tranches de pain",
        );

        assert_eq!(matches.len(), 3);

        assert_eq!(matches[0].quantity, "250");
        assert_eq!(matches[0].measurement, Some("g".to_string()));
        assert_eq!(matches[0].ingredient_name, "farine"); // "de " removed by post-processing

        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("litre".to_string()));
        assert_eq!(matches[1].ingredient_name, "lait"); // "de " removed by post-processing

        assert_eq!(matches[2].quantity, "2");
        assert_eq!(matches[2].measurement, Some("tranches".to_string()));
        assert_eq!(matches[2].ingredient_name, "pain"); // "de " removed by post-processing
    }

    #[test]
    fn test_multi_word_ingredient_names() {
        let detector = create_detector();

        // Test multi-word ingredient names
        let matches = detector.extract_ingredient_measurements(
            "2 cups all-purpose flour\n1 teaspoon baking powder\n500g unsalted butter",
        );

        assert_eq!(matches.len(), 3);

        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "all-purpose flour");

        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "baking powder");

        assert_eq!(matches[2].quantity, "500");
        assert_eq!(matches[2].measurement, Some("g".to_string()));
        assert_eq!(matches[2].ingredient_name, "unsalted butter");
    }

    #[test]
    fn test_measurement_at_end_of_line() {
        let detector = create_detector();

        // Test when measurement is at the end of the line (no ingredient name)
        let matches =
            detector.extract_ingredient_measurements("Add 2 cups\nMix 1 tablespoon\nBake at 350");

        assert_eq!(matches.len(), 2);

        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "");

        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("tablespoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "");
    }

    #[test]
    fn test_regex_pattern_validation() {
        let detector = create_detector();

        // Test that the regex correctly identifies various measurement formats
        let test_cases = vec![
            // Basic volume measurements
            ("1 cup", true),
            ("2 cups", true),
            ("1.5 cups", true),
            ("0.25 cups", true),
            // Weight measurements
            ("500g", true),
            ("1.5kg", true),
            ("250 grams", true),
            ("2 pounds", true),
            // Volume measurements
            ("1 tablespoon", true),
            ("2 teaspoons", true),
            ("1 tsp", true),
            ("2 tbsp", true),
            ("500 ml", true),
            ("1 liter", true),
            // Count measurements (excluding eggs/œufs which are ingredients)
            ("2 slices", true),
            ("1 can", true),
            ("4 pieces", true),
            ("3 sachets", true),
            // French measurements
            ("2 tasses", true),
            ("1 cuillère à soupe", true),
            ("250 g", true),
            // Non-measurements (should not match)
            ("recipe", false),
            ("ingredients", false),
            ("flour", false),
            ("sugar", false),
            ("salt", false),
            ("", false),
            ("123", false), // Just a number, no unit
            ("abc", false),
            ("cupboard", false),      // Contains "cup" but not as measurement
            ("tablespoonful", false), // Contains "tablespoon" but not as measurement
        ];

        for (text, should_match) in test_cases {
            assert_eq!(
                detector.has_measurements(text),
                should_match,
                "Pattern validation failed for: '{}' (expected: {})",
                text,
                should_match
            );
        }
    }

    #[test]
    fn test_regex_capture_groups() {
        let detector = create_detector();

        // Test that the regex captures complete measurement units
        let test_text = "Mix 2 cups flour with 1 tbsp sugar and 500g butter";
        let matches = detector.extract_ingredient_measurements(test_text);

        assert_eq!(matches.len(), 3);

        // Verify each match captures the complete measurement
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("tbsp".to_string()));
        assert_eq!(matches[2].quantity, "500");
        assert_eq!(matches[2].measurement, Some("g".to_string()));

        // Verify positions are correct
        assert_eq!(matches[0].start_pos, 4); // "Mix 2" -> position after "Mix "
        assert_eq!(matches[0].end_pos, 16); // "Mix 2 cups flour" -> ends at position 16
    }

    #[test]
    fn test_regex_boundary_conditions() {
        let detector = create_detector();

        // Test word boundaries and edge cases
        let boundary_tests = vec![
            ("1cup", true),    // No space between number and unit (technically matches pattern)
            ("cup1", false),   // Unit before number
            ("1 cup.", true),  // Period after measurement
            ("(1 cup)", true), // Parentheses around measurement
            ("1 cup,", true),  // Comma after measurement
            ("1 cup;", true),  // Semicolon after measurement
            ("cup of flour", false), // "cup" without number
            ("cups", false),   // Just unit, no number
            ("1", false),      // Just number, no unit
        ];

        for (text, should_match) in boundary_tests {
            assert_eq!(
                detector.has_measurements(text),
                should_match,
                "Boundary test failed for: '{}' (expected: {})",
                text,
                should_match
            );
        }
    }

    #[test]
    fn test_regex_case_insensitivity() {
        let detector = create_detector();

        // Test that the regex is case insensitive
        let case_tests = vec![
            "2 CUPS flour",
            "2 Cups flour",
            "2 cups flour",
            "500G butter",
            "500g butter",
            "1 TBSP sugar",
            "1 tbsp sugar",
            "1 Tablespoon sugar",
        ];

        for text in case_tests {
            assert!(
                detector.has_measurements(text),
                "Case insensitivity test failed for: '{}'",
                text
            );
        }
    }

    #[test]
    fn test_regex_french_accents() {
        let detector = create_detector();

        // Test that French measurements with accents work correctly
        let french_tests = vec![
            "1 cuillère à café",
            "2 cuillères à soupe",
            "1 kilogramme",
            "2 grammes",
            "1 millilitre",
            "2 litres",
            "1 tranche",
            "2 morceaux",
            "1 boîte",
            "2 sachets",
        ];

        for text in french_tests {
            assert!(
                detector.has_measurements(text),
                "French accent test failed for: '{}'",
                text
            );
        }
    }

    #[test]
    fn test_ingredient_name_postprocessing() {
        let config = MeasurementConfig {
            enable_ingredient_postprocessing: true,
            max_ingredient_length: 50,
            ..Default::default()
        };
        let detector = MeasurementDetector::with_config(config).unwrap();

        // Test basic post-processing
        let matches = detector
            .extract_ingredient_measurements("2 cups of flour\n1 tablespoon sugar\n500g butter");

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].ingredient_name, "flour"); // "of " removed
        assert_eq!(matches[1].ingredient_name, "sugar");
        assert_eq!(matches[2].ingredient_name, "butter");
    }

    #[test]
    fn test_french_ingredient_postprocessing() {
        let config = MeasurementConfig {
            enable_ingredient_postprocessing: true,
            ..Default::default()
        };
        let detector = MeasurementDetector::with_config(config).unwrap();

        let matches = detector
            .extract_ingredient_measurements("250 g de farine\n1 litre du lait\n2 tasses d'eau");

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].ingredient_name, "farine"); // "de " removed
        assert_eq!(matches[1].ingredient_name, "lait"); // "du " removed
        assert_eq!(matches[2].ingredient_name, "eau"); // "d'" removed
    }

    #[test]
    fn test_ingredient_length_limit() {
        let config = MeasurementConfig {
            enable_ingredient_postprocessing: true,
            max_ingredient_length: 20,
            ..Default::default()
        };
        let detector = MeasurementDetector::with_config(config).unwrap();

        let matches = detector.extract_ingredient_measurements(
            "2 cups of very-long-ingredient-name-that-should-be-truncated",
        );

        assert_eq!(matches.len(), 1);
        assert!(matches[0].ingredient_name.len() <= 20);
        assert_eq!(matches[0].ingredient_name, "very-long-ingredient"); // "of " removed, then truncated at word boundary
    }

    #[test]
    fn test_postprocessing_disabled() {
        let config = MeasurementConfig {
            enable_ingredient_postprocessing: false,
            ..Default::default()
        };
        let detector = MeasurementDetector::with_config(config).unwrap();

        let matches = detector.extract_ingredient_measurements("2 cups of flour");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ingredient_name, "of flour"); // No post-processing
    }

    #[test]
    fn test_fraction_measurements() {
        let detector = create_detector();

        // Test fraction measurements
        assert!(detector.has_measurements("1/2 cup flour"));
        assert!(detector.has_measurements("3/4 teaspoon salt"));
        assert!(detector.has_measurements("1/4 kg sugar"));
        assert!(detector.has_measurements("2/3 litre milk"));
        assert!(detector.has_measurements("1/8 teaspoon vanilla"));
    }

    #[test]
    fn test_fraction_ingredient_extraction() {
        let detector = create_detector();

        // Test fraction ingredient name extraction
        let matches = detector
            .extract_ingredient_measurements("1/2 cup flour\n3/4 teaspoon salt\n1/4 kg sugar");

        assert_eq!(matches.len(), 3);

        assert_eq!(matches[0].quantity, "1/2");
        assert_eq!(matches[0].measurement, Some("cup".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");

        assert_eq!(matches[1].quantity, "3/4");
        assert_eq!(matches[1].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "salt");

        assert_eq!(matches[2].quantity, "1/4");
        assert_eq!(matches[2].measurement, Some("kg".to_string()));
        assert_eq!(matches[2].ingredient_name, "sugar");
    }

    #[test]
    fn test_unicode_fraction_characters() {
        let detector = create_detector();

        // Test Unicode fraction characters (now supported!)
        assert!(detector.has_measurements("½ cup flour")); // Unicode ½ character
        assert!(detector.has_measurements("⅓ teaspoon salt")); // Unicode ⅓ character
        assert!(detector.has_measurements("¼ kg sugar")); // Unicode ¼ character

        // ASCII fractions still work
        assert!(detector.has_measurements("1/2 cup flour"));
        assert!(detector.has_measurements("1/3 teaspoon salt"));
        assert!(detector.has_measurements("1/4 kg sugar"));
    }

    #[test]
    fn test_mixed_number_quantities() {
        let detector = create_detector();

        // Test mixed number quantities (digit + Unicode fraction)
        assert!(detector.has_measurements("1½ cups flour"));
        assert!(detector.has_measurements("2¼ teaspoons salt"));
        assert!(detector.has_measurements("3¾ kg sugar"));

        // Test extraction
        let matches = detector.extract_ingredient_measurements("1½ cups flour\n2¼ teaspoons salt");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].quantity, "1½");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");

        assert_eq!(matches[1].quantity, "2¼");
        assert_eq!(matches[1].measurement, Some("teaspoons".to_string()));
        assert_eq!(matches[1].ingredient_name, "salt");
    }

    #[test]
    fn test_fraction_corrections() {
        let detector = create_detector();

        // Test common OCR corrections for fractions
        let matches = detector.extract_ingredient_measurements("l/2 cup flour\nO/4 teaspoon salt");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].quantity, "1/2"); // 'l' corrected to '1'
        assert_eq!(matches[0].measurement, Some("cup".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");

        assert_eq!(matches[1].quantity, "0/4"); // 'O' corrected to '0'
        assert_eq!(matches[1].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "salt");
    }

    #[test]
    fn test_get_unique_units() {
        let detector = create_detector();

        // Test with various units
        let text = "2 cups flour\n500g sugar\n1 tablespoon vanilla\n250 ml milk\n3 eggs";
        let units = detector.get_unique_units(text);

        assert_eq!(units.len(), 5); // "2 cups", "500 g", "1 tablespoon", "250 ml", "3"
        assert!(units.contains("2 cups"));
        assert!(units.contains("500 g"));
        assert!(units.contains("1 tablespoon"));
        assert!(units.contains("250 ml"));
        assert!(units.contains("3"));

        // Test empty text
        let empty_units = detector.get_unique_units("");
        assert_eq!(empty_units.len(), 0);

        // Test text with no measurements
        let no_measurements =
            detector.get_unique_units("Just some plain text without measurements");
        assert_eq!(no_measurements.len(), 0);

        // Test duplicate units (should be unique)
        let duplicate_text = "1 cup flour\n2 cups sugar\n3 cups milk";
        let duplicate_units = detector.get_unique_units(duplicate_text);
        assert_eq!(duplicate_units.len(), 3); // "1 cup", "2 cups", "3 cups" - all different
        assert!(duplicate_units.contains("1 cup"));
        assert!(duplicate_units.contains("2 cups"));
        assert!(duplicate_units.contains("3 cups"));
    }

    #[test]
    fn test_multi_word_ingredients_without_measurement() {
        let detector = create_detector();

        let input = "3 large eggs";
        let matches = detector.extract_ingredient_measurements(input);
        println!(
            "Input: '{}', matches: {}, ingredient: '{}'",
            input,
            matches.len(),
            matches[0].ingredient_name
        );

        let test_cases = vec![
            ("2 crème fraîche", "crème fraîche"),           // French dairy
            ("6 pommes de terre", "pommes de terre"),       // French vegetable
            ("3 large eggs", "large eggs"),                 // English descriptive
            ("4 fresh tomatoes", "fresh tomatoes"),         // English descriptive
            ("2 red onions", "red onions"),                 // English color + ingredient
            ("5 green bell peppers", "green bell peppers"), // Multiple adjectives
        ];

        for (input, expected_ingredient) in test_cases {
            let matches = detector.extract_ingredient_measurements(input);
            assert_eq!(
                matches.len(),
                1,
                "Expected exactly one match for: {}",
                input
            );
            assert_eq!(
                matches[0].ingredient_name, expected_ingredient,
                "Ingredient mismatch for: {}",
                input
            );
        }
    }

    #[test]
    fn test_unified_extraction_consistency() {
        let detector = create_detector();

        // Test that the same extraction logic works for both measurement and non-measurement cases
        let consistency_tests = vec![
            // Both should extract "flour" consistently
            ("2 cups flour", "flour"),
            ("3 flour", "flour"),
            // Both should extract "sugar" consistently
            ("1 tablespoon sugar", "sugar"),
            ("4 sugar", "sugar"),
            // Multi-word ingredients should work in both cases
            ("2 cups all-purpose flour", "all-purpose flour"),
            ("5 all-purpose flour", "all-purpose flour"),
        ];

        for (input, expected_ingredient) in consistency_tests {
            let matches = detector.extract_ingredient_measurements(input);
            assert_eq!(
                matches.len(),
                1,
                "Expected exactly one match for: {}",
                input
            );
            assert_eq!(
                matches[0].ingredient_name, expected_ingredient,
                "Consistency test failed for: {}",
                input
            );
        }
    }

    #[test]
    fn test_french_prepositions_postprocessing() {
        let detector = create_detector();

        let test_cases = vec![
            ("2g de chocolat noir", "chocolat noir"), // "de" removed by post-processing
            ("250 ml de lait", "lait"),               // "de" removed by post-processing
            ("1 sachet de levure", "levure"),         // "de" removed by post-processing
            ("3 cuillères à soupe de sucre", "sucre"), // "cuillères à soupe" matched as measurement, "de" removed
            ("1 verre d'eau", "verre d'eau"), // "verre" not a measurement unit, "d'" not removed
            ("2 tranches de pain complet", "pain complet"), // "de" removed by post-processing
        ];

        for (input, expected_ingredient) in test_cases {
            let matches = detector.extract_ingredient_measurements(input);
            assert_eq!(
                matches.len(),
                1,
                "Expected exactly one match for: {}",
                input
            );
            assert_eq!(
                matches[0].ingredient_name, expected_ingredient,
                "French preposition test failed for: {}",
                input
            );
        }
    }

    #[test]
    fn test_mixed_measurement_multi_word() {
        let detector = create_detector();

        let test_cases = vec![
            // With measurements
            (
                "2 cups all-purpose flour",
                "2",
                Some("cups"),
                "all-purpose flour",
            ),
            (
                "500g dark chocolate chips",
                "500",
                Some("g"),
                "dark chocolate chips",
            ),
            ("1 tbsp olive oil", "1", Some("tbsp"), "olive oil"),
            (
                "3 tasses de crème fraîche",
                "3",
                Some("tasses"),
                "crème fraîche",
            ),
            // Without measurements (quantity-only)
            ("3 large eggs", "3", None, "large eggs"),
            ("4 fresh basil leaves", "4", None, "fresh basil leaves"),
            ("2 œufs frais", "2", None, "œufs frais"),
            (
                "6 pommes de terre nouvelles",
                "6",
                None,
                "pommes de terre nouvelles",
            ),
        ];

        for (input, expected_quantity, expected_measurement, expected_ingredient) in test_cases {
            let matches = detector.extract_ingredient_measurements(input);
            assert_eq!(
                matches.len(),
                1,
                "Expected exactly one match for: {}",
                input
            );
            let m = &matches[0];
            assert_eq!(
                m.quantity, expected_quantity,
                "Quantity mismatch for: {}",
                input
            );
            assert_eq!(
                m.measurement.as_deref(),
                expected_measurement,
                "Measurement mismatch for: {}",
                input
            );
            assert_eq!(
                m.ingredient_name, expected_ingredient,
                "Ingredient mismatch for: {}",
                input
            );
        }
    }

    #[test]
    fn test_unified_extraction_edge_cases() {
        let detector = create_detector();

        // Empty and whitespace
        assert_eq!(detector.extract_ingredient_measurements("").len(), 0);
        assert_eq!(detector.extract_ingredient_measurements("   ").len(), 0);

        // Numbers without ingredients
        assert_eq!(detector.extract_ingredient_measurements("42").len(), 0);
        assert_eq!(detector.extract_ingredient_measurements("1/2").len(), 0);

        // Measurements without ingredients
        let cups_matches = detector.extract_ingredient_measurements("2 cups");
        assert_eq!(cups_matches.len(), 1);
        assert_eq!(cups_matches[0].quantity, "2");
        assert_eq!(cups_matches[0].measurement, Some("cups".to_string()));
        assert_eq!(cups_matches[0].ingredient_name, "");

        let grams_matches = detector.extract_ingredient_measurements("500g");
        assert_eq!(grams_matches.len(), 1);
        assert_eq!(grams_matches[0].quantity, "500");
        assert_eq!(grams_matches[0].measurement, Some("g".to_string()));
        assert_eq!(grams_matches[0].ingredient_name, "");

        // Special characters and unicode
        let unicode_test = detector.extract_ingredient_measurements("2 œufs français");
        assert_eq!(unicode_test.len(), 1);
        assert_eq!(unicode_test[0].ingredient_name, "œufs français");

        // Very long ingredient names (should be handled gracefully)
        let long_ingredient = format!("2 {}", "very ".repeat(50) + "long ingredient name");
        let long_test = detector.extract_ingredient_measurements(&long_ingredient);
        assert_eq!(long_test.len(), 1);
        assert!(long_test[0].ingredient_name.len() <= 100); // Should be truncated to max length

        // Boundary detection with commas
        let comma_test = detector.extract_ingredient_measurements("2 cups flour, 1 cup sugar");
        assert_eq!(comma_test.len(), 2); // Should match both ingredients
        assert_eq!(comma_test[0].ingredient_name, "flour");
        assert_eq!(comma_test[1].ingredient_name, "sugar");

        // Mixed case and special formatting
        let mixed_case = detector.extract_ingredient_measurements("2 CUPS All-Purpose Flour");
        assert_eq!(mixed_case.len(), 1);
        assert_eq!(mixed_case[0].ingredient_name, "All-Purpose Flour");
    }

    #[test]
    fn test_unified_extraction_regex_pattern_design() {
        // Test the new unified regex pattern design for Task 1.2
        // This simulates the new pattern that makes measurement optional and captures all remaining text as ingredient

        // Build the new unified pattern (measurement optional, capture all remaining text)
        let config = just_ingredients::text_processing::load_measurement_units_config();
        let mut all_units: Vec<String> = Vec::new();
        all_units.extend(config.measurement_units.volume_units);
        all_units.extend(config.measurement_units.weight_units);
        all_units.extend(config.measurement_units.volume_units_metric);
        all_units.extend(config.measurement_units.us_units);
        all_units.extend(config.measurement_units.french_units);

        let unique_units: std::collections::HashSet<String> = all_units.into_iter().collect();
        let mut sorted_units: Vec<String> = unique_units.into_iter().collect();
        sorted_units.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        let escaped_units: Vec<String> = sorted_units
            .into_iter()
            .map(|unit| regex::escape(&unit))
            .collect();
        let units_pattern = escaped_units.join("|");

        // New unified pattern: measurement is optional, ingredient captures all remaining text
        let new_pattern = format!(
            r"(?i)(?P<quantity>\d+/\d+|\d*\.?\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{})(?:\s|$|[^a-zA-Z]))?\s*(?P<ingredient>.*)",
            units_pattern
        );

        let regex = regex::Regex::new(&new_pattern);
        assert!(
            regex.is_ok(),
            "New unified pattern should compile: {:?}",
            regex.err()
        );
        let regex = regex.unwrap();

        // Test cases for the new unified extraction pattern
        let test_cases = vec![
            // Quantity-only ingredients (should now capture full names)
            ("2 crème fraîche", "2", None, "crème fraîche"),
            ("6 pommes de terre", "6", None, "pommes de terre"),
            ("3 eggs", "3", None, "eggs"),
            ("4 apples", "4", None, "apples"),
            // Traditional measurements (should work the same)
            ("2 cups flour", "2", Some("cups"), "flour"),
            ("500g chocolat noir", "500", Some("g"), "chocolat noir"),
            ("1 tablespoon sugar", "1", Some("tablespoon"), "sugar"),
            // Measurements with prepositions (post-processing will handle "de ")
            ("2g de chocolat", "2", Some("g"), "de chocolat"),
            ("250 ml de lait", "250", Some("ml"), "de lait"),
            // Edge cases
            ("1/2 cup sugar", "1/2", Some("cup"), "sugar"),
            ("½ teaspoon vanilla", "½", Some("teaspoon"), "vanilla"),
        ];

        for (input, expected_quantity, expected_measurement, expected_ingredient) in test_cases {
            let captures = regex
                .captures(input)
                .unwrap_or_else(|| panic!("Pattern should match: {}", input));

            let quantity = captures.name("quantity").map(|m| m.as_str()).unwrap_or("");
            let measurement = captures.name("measurement").map(|m| m.as_str());
            let ingredient = captures
                .name("ingredient")
                .map(|m| m.as_str())
                .unwrap_or("");

            assert_eq!(
                quantity, expected_quantity,
                "Quantity mismatch for input: {}",
                input
            );
            assert_eq!(
                measurement, expected_measurement,
                "Measurement mismatch for input: {}",
                input
            );
            assert_eq!(
                ingredient, expected_ingredient,
                "Ingredient mismatch for input: {}",
                input
            );
        }

        // Test that pattern doesn't match non-measurement text
        assert!(!regex.is_match("some plain text"));
        assert!(!regex.is_match("flour"));
        assert!(!regex.is_match("add salt"));
    }

    #[test]
    fn test_unified_extraction_measurement_detection_accuracy() {
        // Test that the new unified pattern maintains measurement detection accuracy
        let config = just_ingredients::text_processing::load_measurement_units_config();
        let mut all_units: Vec<String> = Vec::new();
        all_units.extend(config.measurement_units.volume_units);
        all_units.extend(config.measurement_units.weight_units);
        all_units.extend(config.measurement_units.volume_units_metric);
        all_units.extend(config.measurement_units.us_units);
        all_units.extend(config.measurement_units.french_units);

        let unique_units: std::collections::HashSet<String> = all_units.into_iter().collect();
        let mut sorted_units: Vec<String> = unique_units.into_iter().collect();
        sorted_units.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        let escaped_units: Vec<String> = sorted_units
            .into_iter()
            .map(|unit| regex::escape(&unit))
            .collect();
        let units_pattern = escaped_units.join("|");

        let new_pattern = format!(
            r"(?i)(?P<quantity>\d+/\d+|\d*\.?\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{})(?:\s|$|[^a-zA-Z]))?\s*(?P<ingredient>.*)",
            units_pattern
        );

        let regex = regex::Regex::new(&new_pattern);
        assert!(
            regex.is_ok(),
            "New unified pattern should compile: {:?}",
            regex.err()
        );
        let regex = regex.unwrap();

        // Test comprehensive measurement detection
        let should_match = vec![
            // English measurements
            "2 cups flour",
            "1 tablespoon sugar",
            "500g butter",
            "1 kg tomatoes",
            "250 ml milk",
            "1 tsp salt",
            "2 tbsp oil",
            "1 lb beef",
            "8 oz water",
            // French measurements
            "2 tasses de farine",
            "1 cuillère à soupe de sucre",
            "500 g de beurre",
            "1 kg de tomates",
            "250 ml de lait",
            "1 sachet de levure",
            // Quantity-only ingredients (no actual measurement units)
            "6 œufs",
            "4 pommes",
            "3 eggs",
            "2 apples",
            // Fractions
            "1/2 cup sugar",
            "3/4 teaspoon salt",
            "½ cup flour",
            "⅓ teaspoon vanilla",
        ];

        let should_not_match = vec![
            "some flour",
            "add salt",
            "flour",
            "sugar",
            "salt",
            "",
            "recipe",
            "ingredients",
            "cupboard",
            "tablespoonful",
            "cup of tea",
            "abc",
        ];

        for text in should_match {
            assert!(regex.is_match(text), "Pattern should match: '{}'", text);
        }

        for text in should_not_match {
            assert!(
                !regex.is_match(text),
                "Pattern should NOT match: '{}'",
                text
            );
        }
    }

    #[test]
    fn test_comma_separated_ingredients_bug() {
        let detector = create_detector();

        // This is the bug case: "150g de farine, 100g de sucre" should produce 2 separate ingredients
        let text = "150g de farine, 100g de sucre";
        let matches = detector.extract_ingredient_measurements(text);

        println!("Input text: '{}'", text);
        println!("Number of matches: {}", matches.len());
        for (i, m) in matches.iter().enumerate() {
            println!(
                "Match {}: quantity='{}', measurement={:?}, ingredient='{}'",
                i, m.quantity, m.measurement, m.ingredient_name
            );
        }

        // Expected: 2 matches
        // Match 0: quantity="150", measurement=Some("g"), ingredient="farine"
        // Match 1: quantity="100", measurement=Some("g"), ingredient="sucre"

        assert_eq!(matches.len(), 2, "Should find 2 separate ingredients");

        // First ingredient
        assert_eq!(matches[0].quantity, "150");
        assert_eq!(matches[0].measurement, Some("g".to_string()));
        assert_eq!(matches[0].ingredient_name, "farine");

        // Second ingredient
        assert_eq!(matches[1].quantity, "100");
        assert_eq!(matches[1].measurement, Some("g".to_string()));
        assert_eq!(matches[1].ingredient_name, "sucre");
    }

    #[test]
    fn test_mixed_single_multi_line_recipe_integration() {
        let detector = create_detector();

        // Realistic OCR-like recipe text with mixed single and multi-line ingredients
        let text = "INGREDIENTS:\n\
                   2 cups all-purpose\n\
                   flour\n\
                   1 teaspoon baking\n\
                   soda\n\
                   1/2 teaspoon salt\n\
                   3/4 cup unsalted\n\
                   butter, softened\n\
                   1 cup granulated sugar\n\
                   2 large eggs\n\
                   1 teaspoon vanilla\n\
                   extract\n\
                   1 cup buttermilk\n\
                   2 tablespoons melted\n\
                   butter";

        let matches = detector.extract_ingredient_measurements(text);

        // Should find 9 ingredients total
        assert_eq!(
            matches.len(),
            9,
            "Should extract 9 ingredients from complex recipe"
        );

        // Verify each ingredient is correctly parsed
        // 1. Multi-line: "2 cups all-purpose flour"
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "all-purpose flour");

        // 2. Multi-line: "1 teaspoon baking soda"
        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[1].ingredient_name, "baking soda");

        // 3. Single-line: "1/2 teaspoon salt"
        assert_eq!(matches[2].quantity, "1/2");
        assert_eq!(matches[2].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[2].ingredient_name, "salt");

        // 4. Multi-line with comma: "3/4 cup unsalted butter, softened"
        assert_eq!(matches[3].quantity, "3/4");
        assert_eq!(matches[3].measurement, Some("cup".to_string()));
        assert_eq!(matches[3].ingredient_name, "unsalted butter, softened");

        // 5. Single-line: "1 cup granulated sugar"
        assert_eq!(matches[4].quantity, "1");
        assert_eq!(matches[4].measurement, Some("cup".to_string()));
        assert_eq!(matches[4].ingredient_name, "granulated sugar");

        // 6. Single-line: "2 large eggs"
        assert_eq!(matches[5].quantity, "2");
        assert_eq!(matches[5].measurement, None);
        assert_eq!(matches[5].ingredient_name, "large eggs");

        // 7. Multi-line: "1 teaspoon vanilla extract"
        assert_eq!(matches[6].quantity, "1");
        assert_eq!(matches[6].measurement, Some("teaspoon".to_string()));
        assert_eq!(matches[6].ingredient_name, "vanilla extract");

        // 8. Single-line: "1 cup buttermilk"
        assert_eq!(matches[7].quantity, "1");
        assert_eq!(matches[7].measurement, Some("cup".to_string()));
        assert_eq!(matches[7].ingredient_name, "buttermilk");

        // 9. Multi-line: "2 tablespoons melted butter"
        assert_eq!(matches[8].quantity, "2");
        assert_eq!(matches[8].measurement, Some("tablespoons".to_string()));
        assert_eq!(matches[8].ingredient_name, "melted butter");
    }

    #[test]
    fn test_ocr_like_text_with_noise_integration() {
        let detector = create_detector();

        // OCR-like text with common OCR errors and noise
        let text = "Recipe from old cookbook\n\
                   \n\
                   2 cups all purpose\n\
                   flour sifted\n\
                   1 tsp baking\n\
                   powder\n\
                   1/2 tsp salt\n\
                   \n\
                   3/4 cup butter\n\
                   softened\n\
                   1 cup brown sugar\n\
                   packed\n\
                   2 eggs\n\
                   room temperature\n\
                   \n\
                   For the topping:\n\
                   1/4 cup flour\n\
                   1 tbsp sugar\n\
                   1/2 tsp cinnamon\n\
                   ground";

        let matches = detector.extract_ingredient_measurements(text);

        // Should find 9 ingredients despite noise and empty lines
        assert_eq!(
            matches.len(),
            9,
            "Should handle OCR noise and extract correct ingredients"
        );

        // Verify key ingredients are parsed correctly
        // Multi-line with OCR error: "2 cups all purpose flour sifted"
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "all purpose flour sifted");

        // Multi-line: "1 tsp baking powder"
        assert_eq!(matches[1].quantity, "1");
        assert_eq!(matches[1].measurement, Some("tsp".to_string()));
        assert_eq!(matches[1].ingredient_name, "baking powder");

        // Multi-line with comma: "3/4 cup butter softened"
        assert_eq!(matches[3].quantity, "3/4");
        assert_eq!(matches[3].measurement, Some("cup".to_string()));
        assert_eq!(matches[3].ingredient_name, "butter softened");

        // Multi-line: "1 cup brown sugar packed"
        assert_eq!(matches[4].quantity, "1");
        assert_eq!(matches[4].measurement, Some("cup".to_string()));
        assert_eq!(matches[4].ingredient_name, "brown sugar packed");

        // Multi-line: "2 eggs room temperature"
        assert_eq!(matches[5].quantity, "2");
        assert_eq!(matches[5].measurement, None);
        assert_eq!(matches[5].ingredient_name, "eggs room temperature");
    }

    #[test]
    fn test_multi_line_accuracy_metrics() {
        let detector = create_detector();

        // Test recipe with known expected outcomes for accuracy calculation
        let text = "Cookie Recipe:\n\
                   2 1/2 cups all-purpose\n\
                   flour\n\
                   1 teaspoon baking\n\
                   soda\n\
                   1 teaspoon salt\n\
                   1 cup butter\n\
                   3/4 cup sugar\n\
                   3/4 cup brown\n\
                   sugar\n\
                   2 eggs\n\
                   2 teaspoons vanilla\n\
                   extract";

        let matches = detector.extract_ingredient_measurements(text);

        // Should extract exactly 8 ingredients
        assert_eq!(
            matches.len(),
            8,
            "Should extract all 8 ingredients accurately"
        );

        // Verify accuracy by checking each ingredient
        let expected_ingredients = [
            (
                "2 1/2",
                Some("cups".to_string()),
                "all-purpose flour".to_string(),
            ),
            ("1", Some("teaspoon".to_string()), "baking soda".to_string()),
            ("1", Some("teaspoon".to_string()), "salt".to_string()),
            ("1", Some("cup".to_string()), "butter".to_string()),
            ("3/4", Some("cup".to_string()), "sugar".to_string()),
            ("3/4", Some("cup".to_string()), "brown sugar".to_string()),
            ("2", None, "eggs".to_string()),
            (
                "2",
                Some("teaspoons".to_string()),
                "vanilla extract".to_string(),
            ),
        ];

        for (i, (expected_qty, expected_unit, expected_name)) in
            expected_ingredients.iter().enumerate()
        {
            assert_eq!(
                &matches[i].quantity, expected_qty,
                "Ingredient {} quantity should be '{}'",
                i, expected_qty
            );
            assert_eq!(
                &matches[i].measurement, expected_unit,
                "Ingredient {} measurement should be {:?}",
                i, expected_unit
            );
            assert_eq!(
                &matches[i].ingredient_name, expected_name,
                "Ingredient {} name should be '{}'",
                i, expected_name
            );
        }

        // Calculate accuracy: all ingredients correctly parsed = 100% accuracy
        let total_ingredients = expected_ingredients.len();
        let correctly_parsed = expected_ingredients
            .iter()
            .enumerate()
            .filter(|(i, (qty, unit, name))| {
                matches[*i].quantity == *qty
                    && matches[*i].measurement == *unit
                    && matches[*i].ingredient_name == *name
            })
            .count();

        let accuracy = (correctly_parsed as f64 / total_ingredients as f64) * 100.0;
        assert!(
            accuracy >= 95.0,
            "Accuracy should be >= 95%, got {:.1}% ({}/{} correct)",
            accuracy,
            correctly_parsed,
            total_ingredients
        );
    }
}
