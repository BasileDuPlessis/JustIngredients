//! Converts an [`Extraction`] into the `MeasurementMatch` values used by the review UI, the
//! editing dialogues and the database code, applying the same rules as manual input:
//! canonical units, name length limit and quantity range.

use crate::extraction::{ExtractedIngredient, Extraction};
use crate::text_processing::MeasurementMatch;
use crate::units::normalize_measurement;

/// Longest ingredient name accepted by validation (`validation::validate_measurement_match`).
pub const MAX_NAME_BYTES: usize = 100;

/// Largest quantity accepted by validation (`validation::validate_quantity_range`).
pub const MAX_QUANTITY: f64 = 10_000.0;

/// Maps every ingredient with a non-empty name, in order. The list index becomes the line
/// number; character positions are not meaningful for model output and are set to 0.
pub fn to_measurement_matches(extraction: &Extraction) -> Vec<MeasurementMatch> {
    extraction
        .ingredients
        .iter()
        .filter_map(to_measurement_match)
        .enumerate()
        .map(|(index, mut measurement)| {
            measurement.line_number = index;
            measurement
        })
        .collect()
}

fn to_measurement_match(ingredient: &ExtractedIngredient) -> Option<MeasurementMatch> {
    let name = truncate_to_bytes(ingredient.name.trim(), MAX_NAME_BYTES);
    if name.is_empty() {
        return None;
    }

    let quantity = ingredient.quantity.filter(|q| q.is_finite());
    let normalized = normalize_measurement(quantity, ingredient.unit.as_deref());
    let out_of_range = normalized
        .quantity
        .is_some_and(|q| q <= 0.0 || q > MAX_QUANTITY);

    Some(MeasurementMatch {
        quantity: normalized.quantity.map(format_quantity).unwrap_or_default(),
        measurement: normalized.unit,
        ingredient_name: name.to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 0,
        requires_quantity_confirmation: ingredient.needs_review || out_of_range,
    })
}

/// Formats a quantity as stored (`DECIMAL(10,3)`): at most 3 decimals, no trailing zeros.
pub fn format_quantity(quantity: f64) -> String {
    let rounded = format!("{quantity:.3}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Cuts `text` to at most `max` bytes without splitting a character.
fn truncate_to_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::RecipeLanguage;

    fn ingredient(name: &str, quantity: Option<f64>, unit: Option<&str>) -> ExtractedIngredient {
        ExtractedIngredient {
            name: name.to_string(),
            quantity,
            unit: unit.map(str::to_string),
            needs_review: false,
            source_text: String::new(),
        }
    }

    fn extraction(ingredients: Vec<ExtractedIngredient>) -> Extraction {
        Extraction {
            is_recipe: true,
            language: RecipeLanguage::Fr,
            transcription: String::new(),
            ingredients,
        }
    }

    #[test]
    fn quantities_units_and_names_are_mapped() {
        let matches = to_measurement_matches(&extraction(vec![
            ingredient("farine", Some(150.0), Some("grammes")),
            ingredient("lait", Some(0.5), Some("l")),
            ingredient("sucre", Some(1.0 / 3.0), Some("cup")),
        ]));
        let summary: Vec<_> = matches
            .iter()
            .map(|m| {
                (
                    m.quantity.as_str(),
                    m.measurement.as_deref(),
                    m.ingredient_name.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("150", Some("g"), "farine"),
                ("0.5", Some("l"), "lait"),
                ("0.333", Some("cup"), "sucre")
            ]
        );
        assert_eq!(
            matches.iter().map(|m| m.line_number).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(matches.iter().all(|m| !m.requires_quantity_confirmation));
    }

    #[test]
    fn centilitres_are_converted() {
        let matches = to_measurement_matches(&extraction(vec![ingredient(
            "crème",
            Some(20.0),
            Some("cl"),
        )]));
        assert_eq!(matches[0].quantity, "200");
        assert_eq!(matches[0].measurement.as_deref(), Some("ml"));
    }

    #[test]
    fn ingredients_without_quantity_are_kept_unflagged() {
        let matches = to_measurement_matches(&extraction(vec![ingredient("sel", None, None)]));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].quantity, "");
        assert_eq!(matches[0].measurement, None);
        assert!(!matches[0].requires_quantity_confirmation);
    }

    #[test]
    fn unknown_units_are_kept() {
        let matches = to_measurement_matches(&extraction(vec![ingredient(
            "ail",
            Some(2.0),
            Some("Gousses"),
        )]));
        assert_eq!(matches[0].measurement.as_deref(), Some("gousses"));
    }

    #[test]
    fn out_of_range_quantities_and_model_doubts_are_flagged() {
        let mut unsure = ingredient("beurre", Some(2.0), Some("tbsp"));
        unsure.needs_review = true;
        let matches = to_measurement_matches(&extraction(vec![
            ingredient("farine", Some(15_000.0), Some("g")),
            ingredient("eau", Some(0.0), Some("ml")),
            ingredient("oeufs", Some(f64::NAN), None),
            unsure,
        ]));
        assert!(matches[0].requires_quantity_confirmation);
        assert!(matches[1].requires_quantity_confirmation);
        assert_eq!(matches[2].quantity, "", "NaN is treated as missing");
        assert!(!matches[2].requires_quantity_confirmation);
        assert!(matches[3].requires_quantity_confirmation);
    }

    #[test]
    fn empty_names_are_skipped_and_long_names_truncated() {
        let long_name = "é".repeat(60); // 120 bytes
        let matches = to_measurement_matches(&extraction(vec![
            ingredient("   ", Some(1.0), None),
            ingredient(&long_name, Some(1.0), None),
        ]));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ingredient_name.len(), MAX_NAME_BYTES);
        assert_eq!(matches[0].line_number, 0);
    }

    #[test]
    fn quantities_are_formatted_like_the_database_column() {
        assert_eq!(format_quantity(2.0), "2");
        assert_eq!(format_quantity(2.50), "2.5");
        assert_eq!(format_quantity(0.1236), "0.124");
        assert_eq!(format_quantity(0.1234), "0.123");
        assert_eq!(format_quantity(1500.0), "1500");
        assert_eq!(format_quantity(-0.0001), "0");
    }
}
