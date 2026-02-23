//! Validation module for common validation patterns
//!
//! This module consolidates validation logic that was previously scattered across
//! multiple modules, providing reusable validation functions for:
//!
//! - Recipe names
//! - Ingredient input
//! - Measurement matches
//! - Quantity ranges
//! - Basic input constraints

use crate::text_processing::MeasurementMatch;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref QUANTITY_PATTERN: Regex =
        Regex::new(r"^(-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?)").expect("Invalid quantity regex pattern");
}

/// Validates a recipe name input
///
/// # Arguments
/// * `name` - The recipe name to validate
///
/// # Returns
/// * `Ok(&str)` - The trimmed recipe name if valid
/// * `Err(&str)` - Error type: "empty" or "too_long"
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_recipe_name;
///
/// assert!(validate_recipe_name("My Recipe").is_ok());
/// assert_eq!(validate_recipe_name(""), Err("empty"));
/// assert_eq!(validate_recipe_name(&"a".repeat(256)), Err("too_long"));
/// ```
pub fn validate_recipe_name(name: &str) -> Result<&str, &'static str> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err("empty");
    }

    if trimmed.len() > 255 {
        return Err("too_long");
    }

    Ok(trimmed)
}

/// Validate basic input constraints
///
/// # Arguments
/// * `input` - The input string to validate
///
/// # Returns
/// * `Ok(())` - Input is valid
/// * `Err(&str)` - Error type: "edit-empty" or "edit-too-long"
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_basic_input;
///
/// assert!(validate_basic_input("valid input").is_ok());
/// assert_eq!(validate_basic_input(""), Err("edit-empty"));
/// assert_eq!(validate_basic_input(&"a".repeat(201)), Err("edit-too-long"));
/// ```
pub fn validate_basic_input(input: &str) -> Result<(), &'static str> {
    if input.is_empty() {
        return Err("edit-empty");
    }

    if input.len() > 200 {
        return Err("edit-too-long");
    }

    Ok(())
}

/// Validate a measurement match and its ingredient name
///
/// # Arguments
/// * `measurement_match` - The measurement match to validate
/// * `temp_text` - The temporary text used for extraction
///
/// # Returns
/// * `Ok(())` - Measurement match is valid
/// * `Err(&str)` - Error type indicating validation failure
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_measurement_match;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let valid_match = MeasurementMatch {
///     quantity: "2".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 10,
/// };
///
/// assert!(validate_measurement_match(&valid_match, "temp: 2 cups flour").is_ok());
/// ```
pub fn validate_measurement_match(
    measurement_match: &MeasurementMatch,
    _temp_text: &str,
) -> Result<(), &'static str> {
    let ingredient_name = measurement_match.ingredient_name.trim();

    // With the unified regex pattern, ingredient is always captured by the regex
    // No need to extract from text after measurement
    if ingredient_name.is_empty() {
        return Err("edit-no-ingredient-name");
    }

    if ingredient_name.len() > 100 {
        return Err("edit-ingredient-name-too-long");
    }

    Ok(())
}

/// Adjust quantity for negative values if detected in the text
///
/// # Arguments
/// * `measurement_match` - The measurement match to adjust (mutable)
/// * `temp_text` - The temporary text used for extraction
///
/// # Examples
/// ```
/// use just_ingredients::validation::adjust_quantity_for_negative;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let mut match_with_negative = MeasurementMatch {
///     quantity: "2".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 7, // Position of "2" in "-2 "
///     end_pos: 10,
/// };
///
/// adjust_quantity_for_negative(&mut match_with_negative, "temp: -2 cups flour");
/// assert_eq!(match_with_negative.quantity, "-2");
/// ```
pub fn adjust_quantity_for_negative(measurement_match: &mut MeasurementMatch, temp_text: &str) {
    let quantity_start = measurement_match.start_pos;
    let mut actual_quantity = measurement_match.quantity.clone();

    // Check if there's a minus sign before the quantity
    if quantity_start > 0 && temp_text.as_bytes()[quantity_start - 1] == b'-' {
        // Check if the minus sign is not part of another word (should be preceded by space or at start)
        let before_minus = if quantity_start > 1 {
            temp_text.as_bytes()[quantity_start - 2]
        } else {
            b' '
        };
        if before_minus == b' ' || quantity_start == 1 {
            actual_quantity = format!("-{}", actual_quantity);
        }
    }

    measurement_match.quantity = actual_quantity;
}

/// Validate that quantity is within reasonable range
///
/// # Arguments
/// * `measurement_match` - The measurement match to validate
///
/// # Returns
/// * `Ok(())` - Quantity is within valid range
/// * `Err(&str)` - Error type: "edit-invalid-quantity"
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_quantity_range;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let valid_match = MeasurementMatch {
///     quantity: "2.5".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 10,
/// };
///
/// assert!(validate_quantity_range(&valid_match).is_ok());
///
/// let invalid_match = MeasurementMatch {
///     quantity: "0".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 10,
/// };
///
/// assert_eq!(validate_quantity_range(&invalid_match), Err("edit-invalid-quantity"));
/// ```
pub fn validate_quantity_range(measurement_match: &MeasurementMatch) -> Result<(), &'static str> {
    if let Some(qty) = parse_quantity(&measurement_match.quantity) {
        if qty <= 0.0 || qty > 10000.0 {
            return Err("edit-invalid-quantity");
        }
    }
    Ok(())
}

/// Parse quantity string to f64 (handles fractions and decimals)
///
/// # Arguments
/// * `quantity_str` - The quantity string to parse
///
/// # Returns
/// * `Some(f64)` - The parsed quantity value
/// * `None` - Failed to parse the quantity
///
/// # Examples
/// ```
/// use just_ingredients::validation::parse_quantity;
///
/// assert_eq!(parse_quantity("2"), Some(2.0));
/// assert_eq!(parse_quantity("1/2"), Some(0.5));
/// assert_eq!(parse_quantity("2.5"), Some(2.5));
/// assert_eq!(parse_quantity("invalid"), None);
/// ```
pub fn parse_quantity(quantity_str: &str) -> Option<f64> {
    if quantity_str.contains('/') {
        // Handle fractions like "1/2"
        let parts: Vec<&str> = quantity_str.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(numerator), Ok(denominator)) =
                (parts[0].parse::<f64>(), parts[1].parse::<f64>())
            {
                if denominator != 0.0 {
                    Some(numerator / denominator)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // Handle regular numbers, replace comma with dot for European format
        quantity_str.replace(',', ".").parse::<f64>().ok()
    }
}

/// Parse ingredient text input and create a MeasurementMatch
///
/// This function implements a multi-stage parsing algorithm for ingredient editing:
///
/// ## Parsing Algorithm
///
/// 1. **Basic Validation**: Check input length and emptiness constraints
/// 2. **Measurement Detection**: Attempt to extract measurements using the standard detector
/// 3. **Fallback Parsing**: If no measurements found, use alternative parsing strategies
/// 4. **Validation & Normalization**: Apply comprehensive validation and normalization
///
/// ## Processing Stages
///
/// ### Stage 1: Basic Input Validation
/// ```text
/// - Empty input → Error: "edit-empty"
/// - Input > 200 chars → Error: "edit-too-long"
/// - Otherwise → Proceed to measurement detection
/// ```
///
/// ### Stage 2: Standard Measurement Detection
/// - Uses `MeasurementDetector` to find standard measurement patterns
/// - Handles traditional measurements: "2 cups flour", "500g butter"
/// - Handles quantity-only ingredients: "6 eggs", "4 apples"
/// - Supports fractions and Unicode characters
///
/// ### Stage 3: Fallback Parsing (when no measurements detected)
/// - **Quantity Pattern Matching**: Look for simple numeric patterns (`-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?`)
/// - **Quantity-Only Parsing**: Extract quantity and treat remainder as ingredient name
/// - **Default Quantity**: If no quantity found, default to "1"
///
/// ### Stage 4: Validation & Normalization
/// - **Measurement Validation**: Verify measurement match integrity
/// - **Quantity Range Check**: Ensure quantity is between 0 and 10,000
/// - **Negative Quantity Handling**: Detect and handle negative quantities (e.g., "-2 cups")
/// - **Ingredient Name Validation**: Check length and content constraints
///
/// ## Error Conditions
///
/// - `"edit-empty"`: Input is empty or whitespace-only
/// - `"edit-too-long"`: Input exceeds 200 characters
/// - `"edit-no-ingredient-name"`: No ingredient name found after quantity
/// - `"edit-ingredient-name-too-long"`: Ingredient name exceeds 100 characters
/// - `"edit-invalid-quantity"`: Quantity is ≤ 0 or > 10,000
/// - `"error-processing-failed"`: Measurement detector initialization failed
///
/// ## Thread Safety
///
/// This function is thread-safe as it creates new instances of `MeasurementDetector`
/// and doesn't rely on shared mutable state.
///
/// ## Performance
///
/// - **Fast Path**: Standard measurement detection (most common case)
/// - **Fallback Path**: Regex-based quantity extraction (slower but robust)
/// - **Memory**: Minimal allocations, reuses detector instances
///
/// # Arguments
///
/// * `input` - The raw ingredient text input from user (e.g., "2 cups flour", "3 eggs")
///
/// # Returns
///
/// Returns a `MeasurementMatch` containing parsed quantity, measurement, and ingredient name,
/// or an error string key for localization
///
/// # Examples
///
/// Note: This function is used internally by the dialogue system.
/// For usage examples, see the dialogue handling functions in the bot module.
pub fn parse_ingredient_from_text(input: &str) -> Result<MeasurementMatch, &'static str> {
    use crate::text_processing::MeasurementDetector;

    let trimmed = input.trim();

    // Basic validation
    validate_basic_input(trimmed)?;

    // Try to extract measurement using the detector
    let detector = MeasurementDetector::new().map_err(|_| "error-processing-failed")?;
    let temp_text = format!("temp: {}", trimmed);
    let matches = detector.extract_ingredient_measurements(&temp_text);

    if let Some(mut measurement_match) = matches.into_iter().next() {
        validate_measurement_match(&measurement_match, &temp_text)?;
        adjust_quantity_for_negative(&mut measurement_match, &temp_text);
        validate_quantity_range(&measurement_match)?;
        Ok(measurement_match)
    } else {
        // No measurement found, try alternative parsing strategies
        parse_without_measurement_detector(trimmed)
    }
}

/// Parse ingredient when no measurement detector match is found
fn parse_without_measurement_detector(trimmed: &str) -> Result<MeasurementMatch, &'static str> {
    // Try to extract a simple quantity pattern
    if let Some(captures) = QUANTITY_PATTERN.captures(trimmed) {
        if let Some(quantity_match) = captures.get(1) {
            return parse_with_quantity(trimmed, quantity_match);
        }
    }

    // No quantity found, treat the whole input as ingredient name
    if trimmed.len() > 100 {
        return Err("edit-ingredient-name-too-long");
    }

    Ok(MeasurementMatch {
        quantity: "1".to_string(), // Default quantity
        measurement: None,
        ingredient_name: trimmed.to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: trimmed.len(),
        requires_quantity_confirmation: false,
    })
}

/// Parse ingredient when a quantity pattern is found
fn parse_with_quantity(
    trimmed: &str,
    quantity_match: regex::Match,
) -> Result<MeasurementMatch, &'static str> {
    let quantity = quantity_match.as_str().trim().to_string();
    let remaining = trimmed[quantity_match.end()..].trim().to_string();

    // Validate quantity
    if let Some(qty) = parse_quantity(&quantity) {
        if qty <= 0.0 || qty > 10000.0 {
            return Err("edit-invalid-quantity");
        }
    }

    let ingredient_name = if remaining.is_empty() {
        return Err("edit-no-ingredient-name");
    } else if remaining.len() > 100 {
        return Err("edit-ingredient-name-too-long");
    } else {
        remaining
    };

    Ok(MeasurementMatch {
        quantity,
        measurement: None,
        ingredient_name,
        line_number: 0,
        start_pos: 0,
        end_pos: trimmed.len(),
        requires_quantity_confirmation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_recipe_name() {
        // Valid names
        assert!(validate_recipe_name("My Recipe").is_ok());
        assert!(validate_recipe_name("  Recipe  ").is_ok());
        match validate_recipe_name("  Recipe  ") {
            Ok(name) => assert_eq!(name, "Recipe"),
            Err(e) => panic!("Expected valid recipe name, got error: {}", e),
        }

        // Empty names
        assert_eq!(validate_recipe_name(""), Err("empty"));
        assert_eq!(validate_recipe_name("   "), Err("empty"));

        // Too long names
        let long_name = "a".repeat(256);
        assert_eq!(validate_recipe_name(&long_name), Err("too_long"));
    }

    #[test]
    fn test_validate_basic_input() {
        // Valid input
        assert!(validate_basic_input("valid input").is_ok());
        assert!(validate_basic_input("a").is_ok());

        // Empty input
        assert_eq!(validate_basic_input(""), Err("edit-empty"));

        // Too long input
        let long_input = "a".repeat(201);
        assert_eq!(validate_basic_input(&long_input), Err("edit-too-long"));
    }

    #[test]
    fn test_parse_quantity() {
        // Whole numbers
        assert_eq!(parse_quantity("2"), Some(2.0));
        assert_eq!(parse_quantity("0"), Some(0.0));
        assert_eq!(parse_quantity("-5"), Some(-5.0));

        // Decimals
        assert_eq!(parse_quantity("2.5"), Some(2.5));
        assert_eq!(parse_quantity("1.0"), Some(1.0));

        // Fractions
        assert_eq!(parse_quantity("1/2"), Some(0.5));
        assert_eq!(parse_quantity("3/4"), Some(0.75));
        assert_eq!(parse_quantity("2/1"), Some(2.0));

        // European format
        assert_eq!(parse_quantity("2,5"), Some(2.5));

        // Invalid cases
        assert_eq!(parse_quantity(""), None);
        assert_eq!(parse_quantity("abc"), None);
        assert_eq!(parse_quantity("1/0"), None);
        assert_eq!(parse_quantity("1/"), None);
        assert_eq!(parse_quantity("/2"), None);
    }

    #[test]
    fn test_validate_quantity_range() {
        let create_match = |quantity: &str| MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "flour".to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 10,
            requires_quantity_confirmation: false,
        };

        // Valid ranges
        assert!(validate_quantity_range(&create_match("1")).is_ok());
        assert!(validate_quantity_range(&create_match("10000")).is_ok());
        assert!(validate_quantity_range(&create_match("0.1")).is_ok());
        assert!(validate_quantity_range(&create_match("1/2")).is_ok());

        // Invalid ranges
        assert_eq!(
            validate_quantity_range(&create_match("0")),
            Err("edit-invalid-quantity")
        );
        assert_eq!(
            validate_quantity_range(&create_match("-1")),
            Err("edit-invalid-quantity")
        );
        assert_eq!(
            validate_quantity_range(&create_match("10001")),
            Err("edit-invalid-quantity")
        );
    }

    #[test]
    fn test_adjust_quantity_for_negative() {
        let create_match = |quantity: &str, start_pos: usize| MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "flour".to_string(),
            line_number: 0,
            start_pos,
            end_pos: 10,
            requires_quantity_confirmation: false,
        };

        // Should add negative sign
        let mut match1 = create_match("2", 7); // Position of "2" in "-2 "
        adjust_quantity_for_negative(&mut match1, "temp: -2 cups flour");
        assert_eq!(match1.quantity, "-2");

        // Should not add negative sign (minus not at valid position)
        let mut match2 = create_match("2", 8); // Position after "some -2 "
        adjust_quantity_for_negative(&mut match2, "temp: some -2 cups flour");
        assert_eq!(match2.quantity, "2");

        // Should not add negative sign (no minus)
        let mut match3 = create_match("2", 6);
        adjust_quantity_for_negative(&mut match3, "temp: 2 cups flour");
        assert_eq!(match3.quantity, "2");
    }

    #[test]
    fn debug_parse_ingredient() {
        use crate::text_processing::MeasurementDetector;

        println!("Testing parse_ingredient_from_text with '2 cups flour'");

        match parse_ingredient_from_text("2 cups flour") {
            Ok(result) => {
                println!(
                    "Success: quantity='{}', measurement={:?}, ingredient='{}'",
                    result.quantity, result.measurement, result.ingredient_name
                );
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }

        println!("\nTesting MeasurementDetector directly");
        let detector = match MeasurementDetector::new() {
            Ok(d) => d,
            Err(e) => panic!("Failed to create MeasurementDetector: {}", e),
        };
        let temp_text = format!("temp: {}", "2 cups flour");
        println!("Input text: '{}'", temp_text);

        let matches = detector.extract_ingredient_measurements(&temp_text);
        println!("Found {} matches", matches.len());

        for (i, m) in matches.iter().enumerate() {
            println!(
                "Match {}: quantity='{}', measurement={:?}, ingredient='{}'",
                i, m.quantity, m.measurement, m.ingredient_name
            );
        }
    }
}
