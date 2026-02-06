//! # OCR Error Correction Tests
//!
//! Comprehensive tests for the OCR error correction system.

use just_ingredients::error_correction::{ErrorCorrectionConfig, OcrErrorCorrector};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_character_corrections() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Test fraction corrections
        assert_eq!(corrector.correct_text("Ye cup flour"), "1/2 cup flour");
        assert_eq!(corrector.correct_text("% cup sugar"), "1/4 cup sugar");
        assert_eq!(corrector.correct_text("Vz cup milk"), "1/3 cup milk");

        // Test common character confusions
        assert_eq!(corrector.correct_text("fl0ur"), "fl0ur"); // Should not correct single characters in words
        assert_eq!(corrector.correct_text("O eggs"), "O eggs"); // O not corrected to avoid false positives
    }

    #[test]
    fn test_unit_expansions() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        assert_eq!(
            corrector.correct_text("2 tbsp butter"),
            "2 tablespoon butter"
        );
        assert_eq!(
            corrector.correct_text("1 tsp vanilla"),
            "1 teaspoon vanilla"
        );
        assert_eq!(corrector.correct_text("3 cups flour"), "3 cups flour"); // Already expanded
        assert_eq!(corrector.correct_text("1 lb beef"), "1 pound beef");
        assert_eq!(corrector.correct_text("500 g sugar"), "500 gram sugar");
        assert_eq!(
            corrector.correct_text("2 kg potatoes"),
            "2 kilogram potatoes"
        );
    }

    #[test]
    fn test_ingredient_corrections() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        assert_eq!(corrector.correct_text("2 cups fiour"), "2 cups flour");
        assert_eq!(corrector.correct_text("1 cup suger"), "1 cup sugar");
        assert_eq!(corrector.correct_text("1/2 tsp sait"), "1/2 teaspoon salt");
        assert_eq!(corrector.correct_text("100 g buter"), "100 gram butter");
        assert_eq!(corrector.correct_text("2 egs"), "2 eggs");
    }

    #[test]
    fn test_french_ingredients() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        assert_eq!(corrector.correct_text("200 g farine"), "200 gram farine");
        assert_eq!(corrector.correct_text("100 g sucre"), "100 gram sucre");
        assert_eq!(corrector.correct_text("1 kg pommes"), "1 kilogram pommes");
        assert_eq!(corrector.correct_text("6 oeufs"), "6 œufs");
    }

    #[test]
    fn test_complex_recipe_correction() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false, // Disable fuzzy matching for this test
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        let input = r#"2 tbsp fiour
1 tsp suger
Ye cup sait
3 egs
1 lb buter"#;

        let expected = r#"2 tablespoon flour
1 teaspoon sugar
1/2 cup salt
3 eggs
1 pound butter"#;

        let result = corrector.correct_text(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_context_aware_corrections() {
        let config = ErrorCorrectionConfig {
            enable_context_corrections: true,
            enable_fuzzy_matching: false, // Disable fuzzy matching
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Test fraction spacing correction
        assert_eq!(
            corrector.correct_text("2 1 / 2 cups flour"),
            "2 1/2 cups flour"
        );
        assert_eq!(
            corrector.correct_text("1 1 / 4 tsp salt"),
            "1 1/4 teaspoon salt"
        );
    }

    #[test]
    fn test_fuzzy_matching() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: true,
            max_edit_distance: 2, // Allow distance 2 for this test
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Test fuzzy matching for ingredients with small typos
        assert_eq!(corrector.correct_text("2 cups flur"), "2 cups flour"); // "flur" should match "flour" (distance 2)
        assert_eq!(corrector.correct_text("1 cup sugr"), "1 cup sugar"); // "sugr" -> "sugar" (distance 1)
        assert_eq!(corrector.correct_text("1/2 tsp slt"), "1/2 teaspoon salt"); // "slt" -> "salt" (distance 1)
    }

    #[test]
    fn test_fuzzy_matching_disabled() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Fuzzy matching should not apply when disabled
        assert_eq!(corrector.correct_text("2 cups flur"), "2 cups flur"); // No correction
    }

    #[test]
    fn test_max_edit_distance() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: true,
            max_edit_distance: 1, // Only allow 1 edit distance
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Should correct "flur" (1 edit from "flour") but not "flr" (2 edits)
        assert_eq!(corrector.correct_text("2 cups flur"), "2 cups flour");
        assert_eq!(corrector.correct_text("2 cups flr"), "2 cups flr"); // No correction
    }

    #[test]
    fn test_case_insensitive_corrections() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        assert_eq!(
            corrector.correct_text("2 TBSP BUTTER"),
            "2 tablespoon butter"
        );
        assert_eq!(
            corrector.correct_text("1 TSP VANILLA"),
            "1 teaspoon vanilla"
        );
        assert_eq!(corrector.correct_text("2 CUPS FLOUR"), "2 cups flour");
    }

    #[test]
    fn test_no_false_positives() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // These should not be corrected as they are valid words
        assert_eq!(corrector.correct_text("2 cups flour"), "2 cups flour");
        assert_eq!(
            corrector.correct_text("1 teaspoon sugar"),
            "1 teaspoon sugar"
        ); // Already expanded
        assert_eq!(corrector.correct_text("the best"), "the best");
        assert_eq!(corrector.correct_text("running"), "running");
    }

    #[test]
    fn test_multi_line_correction() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        let input = "Ingredients:\n2 tbsp fiour\n1 tsp suger\nYe cup sait\n3 egs";
        let expected = "Ingredients:\n2 tablespoon flour\n1 teaspoon sugar\n1/2 cup salt\n3 eggs";

        let result = corrector.correct_text(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_punctuation_preservation() {
        let corrector = OcrErrorCorrector::new();

        assert_eq!(
            corrector.correct_text("2 tbsp butter,"),
            "2 tablespoon butter,"
        );
        assert_eq!(
            corrector.correct_text("1 tsp vanilla."),
            "1 teaspoon vanilla."
        );
        assert_eq!(
            corrector.correct_text("flour (all-purpose)"),
            "flour (all-purpose)"
        );
    }

    #[test]
    fn test_mixed_corrections() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        let input = "2 tbsp fiour, 1 tsp suger & Ye cup sait";
        let expected = "2 tablespoon flour, 1 teaspoon sugar & 1/2 cup salt";

        let result = corrector.correct_text(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_empty_and_whitespace() {
        let corrector = OcrErrorCorrector::new();

        assert_eq!(corrector.correct_text(""), "");
        assert_eq!(corrector.correct_text("   "), "   ");
        assert_eq!(corrector.correct_text("\n\n"), "\n\n");
    }

    #[test]
    fn test_unicode_fractions() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Unicode fractions should be preserved
        assert_eq!(corrector.correct_text("½ cup flour"), "½ cup flour");
        assert_eq!(corrector.correct_text("¼ tsp salt"), "¼ teaspoon salt");
        assert_eq!(corrector.correct_text("¾ cup sugar"), "¾ cup sugar");
    }

    #[test]
    fn test_performance_large_text() {
        let corrector = OcrErrorCorrector::new();

        // Create a large recipe text
        let mut large_text = String::new();
        for i in 1..100 {
            large_text.push_str(&format!("{} tbsp fiour\n{} tsp suger\n", i, i));
        }

        let start = std::time::Instant::now();
        let _result = corrector.correct_text(&large_text);
        let duration = start.elapsed();

        // Should complete in reasonable time (less than 1 second)
        assert!(duration.as_millis() < 1000);
    }

    #[test]
    fn test_config_options() {
        // Test with all features enabled
        let config_full = ErrorCorrectionConfig {
            enable_fuzzy_matching: true,
            max_edit_distance: 2,
            enable_context_corrections: true,
            min_correction_confidence: 0.8,
        };
        let _corrector_full = OcrErrorCorrector::with_config(config_full);

        // Test with minimal features
        let config_minimal = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            max_edit_distance: 1,
            enable_context_corrections: false,
            min_correction_confidence: 0.5,
        };
        let _corrector_minimal = OcrErrorCorrector::with_config(config_minimal);
    }

    #[test]
    fn test_integration_with_ocr_pipeline() {
        // This test ensures the error corrector integrates properly with the OCR pipeline
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Simulate OCR output with common errors
        let ocr_output = "Ingredlents:\n2 tbsp fiour\n1 tsp suger\nYe cup sait\n3 egs\n1 lb buter";

        let corrected = corrector.correct_text(ocr_output);

        // Should have applied multiple types of corrections
        assert!(corrected.contains("tablespoon")); // Unit expansion
        assert!(corrected.contains("flour")); // Ingredient correction
        assert!(corrected.contains("sugar")); // Ingredient correction
        assert!(corrected.contains("1/2")); // Fraction correction
        assert!(corrected.contains("salt")); // Ingredient correction
        assert!(corrected.contains("eggs")); // Ingredient correction
        assert!(corrected.contains("pound")); // Unit expansion
        assert!(corrected.contains("butter")); // Ingredient correction
    }
}
