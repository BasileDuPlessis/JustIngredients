//! Ingredient editing module for converting between database and editing formats

use crate::db::Ingredient;
use crate::text_processing::MeasurementMatch;

/// Convert database ingredients to measurement matches for editing
///
/// This function transforms database-stored ingredients into the format expected
/// by the ingredient editing interface, which reuses the recipe creation workflow.
pub fn ingredients_to_measurement_matches(ingredients: &[Ingredient]) -> Vec<MeasurementMatch> {
    ingredients.iter().enumerate().map(|(i, ing)| MeasurementMatch {
        quantity: ing.quantity.map_or("1".to_string(), |q| q.to_string()),
        measurement: ing.unit.clone(),
        ingredient_name: ing.name.clone(),
        line_number: i, // Use array index as line number
        start_pos: 0,  // Not meaningful for database data
        end_pos: ing.name.len(), // Use name length as approximation
    }).collect()
}

/// Represents the changes needed to update ingredients
#[derive(Debug, Clone)]
pub struct IngredientChanges {
    /// Ingredients to update: (ingredient_id, new_data)
    pub to_update: Vec<(i64, MeasurementMatch)>,
    /// New ingredients to add
    pub to_add: Vec<MeasurementMatch>,
    /// Ingredient IDs to delete
    pub to_delete: Vec<i64>,
}

/// Detect what changed between original and edited ingredients
///
/// This function compares the original database ingredients with the edited
/// measurement matches to determine what operations need to be performed.
/// Assumes that edited ingredients are in the same order as original ingredients.
pub fn detect_ingredient_changes(
    original: &[Ingredient],
    edited: &[MeasurementMatch],
) -> IngredientChanges {
    let mut changes = IngredientChanges {
        to_update: Vec::new(),
        to_add: Vec::new(),
        to_delete: Vec::new(),
    };

    // Compare ingredients by position (they should be in the same order)
    let min_len = original.len().min(edited.len());

    // Check for updates (ingredients that exist in both lists but have changed)
    for i in 0..min_len {
        let orig = &original[i];
        let edit = &edited[i];

        // Compare the key data
        let orig_quantity = orig.quantity.unwrap_or(1.0);
        let orig_unit = orig.unit.as_deref().unwrap_or("");
        let orig_name = &orig.name;

        let edit_quantity = edit.quantity.parse::<f64>().unwrap_or(1.0);
        let edit_unit = edit.measurement.as_deref().unwrap_or("");
        let edit_name = &edit.ingredient_name;

        // If any data changed, mark as update
        if (orig_quantity - edit_quantity).abs() > f64::EPSILON
            || orig_unit != edit_unit
            || orig_name != edit_name {
            changes.to_update.push((orig.id, edit.clone()));
        }
    }

    // Check for additions (ingredients in edited but not in original)
    for ingredient in edited.iter().skip(min_len) {
        changes.to_add.push(ingredient.clone());
    }

    // Check for deletions (ingredients in original but not in edited)
    for ingredient in original.iter().skip(min_len) {
        changes.to_delete.push(ingredient.id);
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Ingredient;
    use chrono::Utc;

    fn create_test_ingredient(id: i64, name: &str, quantity: Option<f64>, unit: Option<&str>) -> Ingredient {
        Ingredient {
            id,
            user_id: 1,
            recipe_id: Some(1),
            name: name.to_string(),
            quantity,
            unit: unit.map(|s| s.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_ingredients_to_measurement_matches() {
        let ingredients = vec![
            create_test_ingredient(1, "flour", Some(2.0), Some("cups")),
            create_test_ingredient(2, "sugar", Some(1.5), None),
        ];

        let matches = ingredients_to_measurement_matches(&ingredients);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].quantity, "2");
        assert_eq!(matches[0].measurement, Some("cups".to_string()));
        assert_eq!(matches[0].ingredient_name, "flour");
        assert_eq!(matches[1].quantity, "1.5");
        assert_eq!(matches[1].measurement, None);
        assert_eq!(matches[1].ingredient_name, "sugar");
    }

    #[test]
    fn test_detect_ingredient_changes() {
        let original = vec![
            create_test_ingredient(1, "flour", Some(2.0), Some("cups")),
            create_test_ingredient(2, "sugar", Some(1.0), None),
        ];

        let edited = vec![
            MeasurementMatch {
                quantity: "3".to_string(), // Changed from 2.0 to 3.0
                measurement: Some("cups".to_string()),
                ingredient_name: "flour".to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: 5,
            },
            MeasurementMatch {
                quantity: "1".to_string(),
                measurement: None,
                ingredient_name: "butter".to_string(), // Changed from sugar to butter
                line_number: 1,
                start_pos: 0,
                end_pos: 6,
            },
        ];

        let changes = detect_ingredient_changes(&original, &edited);

        // Should detect flour update and sugar->butter update
        assert_eq!(changes.to_update.len(), 2);
        assert_eq!(changes.to_add.len(), 0);
        assert_eq!(changes.to_delete.len(), 0);

        // Check that flour was updated
        let flour_update = changes.to_update.iter().find(|(id, _)| *id == 1).unwrap();
        assert_eq!(flour_update.1.quantity, "3");
        assert_eq!(flour_update.1.ingredient_name, "flour");

        // Check that sugar was updated to butter
        let sugar_update = changes.to_update.iter().find(|(id, _)| *id == 2).unwrap();
        assert_eq!(sugar_update.1.quantity, "1");
        assert_eq!(sugar_update.1.ingredient_name, "butter");
        assert_eq!(sugar_update.1.measurement, None);
    }
}