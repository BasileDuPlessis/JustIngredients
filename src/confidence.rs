//! Confidence scoring module for ingredient extraction
//!
//! This module provides functionality to calculate confidence scores for ingredient
//! extractions based on multiple factors including pattern strength, measurement validity,
//! context consistency, and OCR quality.

use crate::text_processing::{ConfidenceLevel, IngredientConfidence, MeasurementMatch};

/// Calculate confidence score for an ingredient extraction
///
/// This function evaluates the quality of an ingredient match by analyzing multiple factors:
/// - Pattern strength: How well the regex pattern matched
/// - Measurement validity: Whether the quantity/unit combination is reasonable
/// - Context consistency: How well the ingredient fits the recipe context
/// - OCR quality: Base confidence from the OCR engine
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
/// * `extracted_text` - The full OCR-extracted text for context analysis
/// * `ocr_base_confidence` - Optional base confidence score from OCR engine (0.0-1.0)
///
/// # Returns
///
/// Returns an `IngredientConfidence` struct with overall score and individual factor scores
///
/// # Examples
///
/// ```
/// use just_ingredients::confidence::calculate_ingredient_confidence;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let measurement = MeasurementMatch {
///     quantity: "2".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 11,
///     confidence: None,
/// };
///
/// let confidence = calculate_ingredient_confidence(&measurement, "2 cups flour", Some(0.95));
/// assert!(confidence.overall_score > 0.7);
/// ```
pub fn calculate_ingredient_confidence(
    measurement: &MeasurementMatch,
    extracted_text: &str,
    ocr_base_confidence: Option<f32>,
) -> IngredientConfidence {
    // Calculate individual confidence factors
    let pattern_strength = calculate_pattern_strength(measurement);
    let measurement_validity = calculate_measurement_validity(measurement);
    let context_consistency = calculate_context_consistency(measurement, extracted_text);
    let ocr_quality = ocr_base_confidence.unwrap_or(0.8); // Default to 0.8 if not provided

    // Calculate weighted overall score
    // Pattern strength: 30%, Measurement validity: 30%, Context: 20%, OCR quality: 20%
    let overall_score = (pattern_strength * 0.3)
        + (measurement_validity * 0.3)
        + (context_consistency * 0.2)
        + (ocr_quality * 0.2);

    IngredientConfidence {
        overall_score,
        pattern_strength,
        measurement_validity,
        context_consistency,
        ocr_quality,
    }
}

/// Convert a confidence score to a confidence level category
///
/// This function categorizes a numerical confidence score (0.0-1.0) into discrete levels
/// that are easier for users to understand and act upon.
///
/// # Confidence Level Thresholds
///
/// - **High** (> 0.8): Auto-accept recommended - extraction is highly reliable
/// - **Medium** (0.5-0.8): Review suggested - extraction is likely correct but should be verified
/// - **Low** (0.3-0.5): Manual correction needed - extraction is uncertain
/// - **Invalid** (< 0.3): Not a valid ingredient - extraction is likely incorrect
///
/// # Arguments
///
/// * `confidence` - The confidence struct containing the overall score
///
/// # Returns
///
/// Returns a `ConfidenceLevel` enum value
///
/// # Examples
///
/// ```
/// use just_ingredients::confidence::confidence_to_level;
/// use just_ingredients::text_processing::{ConfidenceLevel, IngredientConfidence};
///
/// let high_confidence = IngredientConfidence {
///     overall_score: 0.85,
///     pattern_strength: 0.9,
///     measurement_validity: 0.9,
///     context_consistency: 0.8,
///     ocr_quality: 0.8,
/// };
///
/// assert_eq!(confidence_to_level(&high_confidence), ConfidenceLevel::High);
/// ```
pub fn confidence_to_level(confidence: &IngredientConfidence) -> ConfidenceLevel {
    let score = confidence.overall_score;

    if score > 0.8 {
        ConfidenceLevel::High
    } else if score >= 0.5 {
        ConfidenceLevel::Medium
    } else if score >= 0.3 {
        ConfidenceLevel::Low
    } else {
        ConfidenceLevel::Invalid
    }
}

/// Calculate pattern strength score based on match quality
///
/// This function evaluates how well the regex pattern matched the text by considering:
/// - Presence of measurement unit (higher confidence)
/// - Presence of ingredient name (required for valid match)
/// - Quantity format validity (numbers, fractions)
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_pattern_strength(measurement: &MeasurementMatch) -> f32 {
    let mut score: f32 = 0.5; // Base score

    // Strong indicators of good pattern match
    if measurement.measurement.is_some() {
        score += 0.3; // Has explicit measurement unit
    }

    if !measurement.ingredient_name.is_empty() {
        score += 0.2; // Has ingredient name
    } else {
        return 0.1; // No ingredient name is a critical failure
    }

    // Penalize very short ingredient names (likely OCR errors)
    if measurement.ingredient_name.len() < 3 {
        score -= 0.4; // Very short names significantly reduce pattern strength
    }

    // Check quantity format validity
    if is_valid_quantity_format(&measurement.quantity) {
        score += 0.0; // Already covered by base score
    } else {
        score -= 0.2; // Invalid quantity format
    }

    score.clamp(0.0, 1.0)
}

/// Calculate measurement validity score
///
/// This function checks if the quantity and unit combination is reasonable:
/// - Quantity should be within reasonable bounds
/// - Unit should be appropriate for the ingredient type (future enhancement)
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_measurement_validity(measurement: &MeasurementMatch) -> f32 {
    let mut score: f32 = 0.7; // Base score for having a measurement

    // Parse quantity and check reasonableness
    if let Ok(qty) = measurement.quantity.parse::<f32>() {
        // Check if quantity is in a reasonable range
        if qty <= 0.0 {
            return 0.1; // Zero or negative is invalid
        } else if qty > 10000.0 {
            score -= 0.4; // Very large quantities are suspicious
        } else if qty > 1000.0 {
            score -= 0.2; // Large quantities reduce confidence
        } else if qty < 0.01 {
            score -= 0.3; // Very small quantities are suspicious
        }
    } else {
        // Try parsing fractions
        if measurement.quantity.contains('/') {
            let parts: Vec<&str> = measurement.quantity.split('/').collect();
            if parts.len() == 2 {
                if let (Ok(num), Ok(denom)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    if denom > 0.0 && num / denom > 0.0 && num / denom <= 10000.0 {
                        score += 0.1; // Valid fraction
                    } else {
                        score -= 0.3; // Invalid fraction
                    }
                } else {
                    score -= 0.3; // Cannot parse fraction
                }
            } else {
                score -= 0.3; // Malformed fraction
            }
        } else {
            score -= 0.2; // Cannot parse quantity
        }
    }

    // If no measurement unit, it's a quantity-only ingredient (still valid but lower confidence)
    if measurement.measurement.is_none() {
        score -= 0.1;
    }

    score.clamp(0.0, 1.0)
}

/// Calculate context consistency score
///
/// This function evaluates how well the ingredient fits within the recipe context:
/// - Ingredient name length (very short or very long names are suspicious)
/// - Presence of special characters or numbers in ingredient name
/// - Position in text (future enhancement for recipe structure analysis)
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
/// * `extracted_text` - The full OCR-extracted text for context
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_context_consistency(measurement: &MeasurementMatch, _extracted_text: &str) -> f32 {
    let mut score: f32 = 0.7; // Base score

    let name = &measurement.ingredient_name;

    // Check ingredient name length
    if name.is_empty() {
        return 0.0; // No name is invalid
    } else if name.len() < 2 {
        score -= 0.3; // Very short names are suspicious
    } else if name.len() > 50 {
        score -= 0.2; // Very long names might be OCR errors
    }

    // Check for excessive numbers in ingredient name (might be OCR error)
    let digit_count = name.chars().filter(|c| c.is_numeric()).count();
    if digit_count > 2 {
        score -= 0.3; // Too many numbers in ingredient name
    }

    // Check for common ingredient name patterns (lowercase letters, spaces, hyphens)
    let valid_chars = name
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace() || *c == '-' || *c == '\'')
        .count();
    let char_ratio = valid_chars as f32 / name.len() as f32;
    
    if char_ratio < 0.7 {
        score -= 0.2; // Too many special characters
    }

    score.clamp(0.0, 1.0)
}

/// Check if quantity format is valid (number, decimal, or fraction)
fn is_valid_quantity_format(quantity: &str) -> bool {
    // Check if it's a simple number or decimal
    if quantity.parse::<f32>().is_ok() {
        return true;
    }

    // Check if it's a fraction (e.g., "1/2")
    if quantity.contains('/') {
        let parts: Vec<&str> = quantity.split('/').collect();
        if parts.len() == 2 {
            return parts[0].parse::<f32>().is_ok() && parts[1].parse::<f32>().is_ok();
        }
    }

    // Check for Unicode fractions (½, ⅓, ¼, etc.)
    let unicode_fractions = ['½', '⅓', '⅔', '¼', '¾', '⅕', '⅖', '⅗', '⅘', '⅙', '⅚', '⅛', '⅜', '⅝', '⅞'];
    if quantity.chars().any(|c| unicode_fractions.contains(&c)) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_match(
        quantity: &str,
        measurement: Option<&str>,
        ingredient_name: &str,
    ) -> MeasurementMatch {
        MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: measurement.map(|s| s.to_string()),
            ingredient_name: ingredient_name.to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 10,
            confidence: None,
        }
    }

    #[test]
    fn test_calculate_confidence_high_quality() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let confidence = calculate_ingredient_confidence(&measurement, "2 cups flour", Some(0.95));

        assert!(confidence.overall_score > 0.7, "High quality match should have high confidence");
        assert!(confidence.pattern_strength > 0.8);
        assert!(confidence.measurement_validity > 0.6);
        assert_eq!(confidence.ocr_quality, 0.95);
    }

    #[test]
    fn test_calculate_confidence_medium_quality() {
        let measurement = create_test_match("500", None, "eggs");
        let confidence = calculate_ingredient_confidence(&measurement, "500 eggs", Some(0.7));

        assert!(confidence.overall_score >= 0.4 && confidence.overall_score <= 0.8);
    }

    #[test]
    fn test_calculate_confidence_low_quality() {
        let measurement = create_test_match("0", Some("cups"), "a");
        let confidence = calculate_ingredient_confidence(&measurement, "0 cups a", Some(0.5));

        assert!(confidence.overall_score < 0.5, "Low quality match should have low confidence, got {}", confidence.overall_score);
    }

    #[test]
    fn test_confidence_to_level_high() {
        let confidence = IngredientConfidence {
            overall_score: 0.85,
            pattern_strength: 0.9,
            measurement_validity: 0.9,
            context_consistency: 0.8,
            ocr_quality: 0.8,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::High);
    }

    #[test]
    fn test_confidence_to_level_medium() {
        let confidence = IngredientConfidence {
            overall_score: 0.65,
            pattern_strength: 0.7,
            measurement_validity: 0.7,
            context_consistency: 0.6,
            ocr_quality: 0.6,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Medium);
    }

    #[test]
    fn test_confidence_to_level_low() {
        let confidence = IngredientConfidence {
            overall_score: 0.4,
            pattern_strength: 0.5,
            measurement_validity: 0.4,
            context_consistency: 0.3,
            ocr_quality: 0.4,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Low);
    }

    #[test]
    fn test_confidence_to_level_invalid() {
        let confidence = IngredientConfidence {
            overall_score: 0.2,
            pattern_strength: 0.3,
            measurement_validity: 0.2,
            context_consistency: 0.1,
            ocr_quality: 0.2,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Invalid);
    }

    #[test]
    fn test_pattern_strength_with_unit() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength > 0.8);
    }

    #[test]
    fn test_pattern_strength_without_unit() {
        let measurement = create_test_match("6", None, "eggs");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength >= 0.5 && strength < 0.9);
    }

    #[test]
    fn test_pattern_strength_no_ingredient() {
        let measurement = create_test_match("2", Some("cups"), "");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength < 0.2);
    }

    #[test]
    fn test_measurement_validity_normal() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity > 0.6);
    }

    #[test]
    fn test_measurement_validity_zero() {
        let measurement = create_test_match("0", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.2);
    }

    #[test]
    fn test_measurement_validity_negative() {
        let measurement = create_test_match("-5", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.2);
    }

    #[test]
    fn test_measurement_validity_very_large() {
        let measurement = create_test_match("50000", Some("grams"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.5);
    }

    #[test]
    fn test_measurement_validity_fraction() {
        let measurement = create_test_match("1/2", Some("cup"), "sugar");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity > 0.6);
    }

    #[test]
    fn test_context_consistency_normal() {
        let measurement = create_test_match("2", Some("cups"), "all-purpose flour");
        let consistency = calculate_context_consistency(&measurement, "2 cups all-purpose flour");
        assert!(consistency > 0.6);
    }

    #[test]
    fn test_context_consistency_short_name() {
        let measurement = create_test_match("2", Some("cups"), "a");
        let consistency = calculate_context_consistency(&measurement, "2 cups a");
        assert!(consistency < 0.5);
    }

    #[test]
    fn test_context_consistency_too_many_numbers() {
        let measurement = create_test_match("2", Some("cups"), "flour123456");
        let consistency = calculate_context_consistency(&measurement, "2 cups flour123456");
        assert!(consistency < 0.5);
    }

    #[test]
    fn test_is_valid_quantity_format() {
        assert!(is_valid_quantity_format("2"));
        assert!(is_valid_quantity_format("2.5"));
        assert!(is_valid_quantity_format("1/2"));
        assert!(is_valid_quantity_format("½"));
        assert!(!is_valid_quantity_format("abc"));
        assert!(!is_valid_quantity_format(""));
    }

    #[test]
    fn test_confidence_boundaries() {
        // Test that confidence scores are always between 0 and 1
        let test_cases = vec![
            create_test_match("2", Some("cups"), "flour"),
            create_test_match("0", Some("cups"), ""),
            create_test_match("99999", None, "x"),
            create_test_match("1/2", Some("tsp"), "salt"),
        ];

        for measurement in test_cases {
            let confidence = calculate_ingredient_confidence(&measurement, "", Some(0.8));
            assert!(confidence.overall_score >= 0.0 && confidence.overall_score <= 1.0);
            assert!(confidence.pattern_strength >= 0.0 && confidence.pattern_strength <= 1.0);
            assert!(confidence.measurement_validity >= 0.0 && confidence.measurement_validity <= 1.0);
            assert!(confidence.context_consistency >= 0.0 && confidence.context_consistency <= 1.0);
        }
    }
}
