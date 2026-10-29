//! Ingredient editing module for converting between database and editing formats

use crate::db::Ingredient;
use crate::dialogue::IngredientField;
use crate::errors::AppError;
use crate::text_processing::MeasurementMatch;
use crate::validation::{
    parse_ingredient_from_text, parse_quantity_answer, MAX_INGREDIENT_NAME_CHARS,
};

/// The values to store for an ingredient: quantities are parsed like typed ones ("1/2",
/// "1 1/2"), and an empty quantity is stored as none.
pub fn to_new_ingredient(ingredient: &MeasurementMatch) -> crate::db::NewIngredient<'_> {
    crate::db::NewIngredient {
        name: &ingredient.ingredient_name,
        quantity: crate::validation::parse_quantity(&ingredient.quantity),
        unit: ingredient.measurement.as_deref(),
    }
}

/// Puts the answer of a line prompt into the list: replaces ingredient `index`, or adds a new
/// last ingredient when `index` is the list's length. `false` (list unchanged) beyond that.
pub fn put_ingredient(
    ingredients: &mut Vec<MeasurementMatch>,
    index: usize,
    ingredient: MeasurementMatch,
) -> bool {
    match index.cmp(&ingredients.len()) {
        std::cmp::Ordering::Less => ingredients[index] = ingredient,
        std::cmp::Ordering::Equal => ingredients.push(ingredient),
        std::cmp::Ordering::Greater => return false,
    }
    true
}

/// The ingredient after the user's answer to an edit prompt: the whole line is parsed again,
/// or only the quantity or the name is replaced. Errors carry the locale key to show.
pub fn apply_edit(
    field: IngredientField,
    current: Option<&MeasurementMatch>,
    input: &str,
) -> Result<MeasurementMatch, AppError> {
    match (field, current) {
        (IngredientField::Quantity, Some(current)) => with_quantity(current, input),
        (IngredientField::Name, Some(current)) => with_name(current, input),
        _ => parse_ingredient_from_text(input),
    }
}

/// Replaces the quantity only (`-`, "aucune"… for none); a corrected quantity needs no further
/// check, so the ⚠️ flag is cleared. A known unit typed after the number ("250 g", "250g",
/// "1/2 cup") replaces the unit too.
pub fn with_quantity(
    current: &MeasurementMatch,
    input: &str,
) -> Result<MeasurementMatch, AppError> {
    let invalid = || AppError::Validation("edit-invalid-quantity-answer".to_string());
    let (quantity, unit) = match parse_quantity_answer(input) {
        Some(quantity) => (quantity, current.measurement.clone()),
        None => {
            let (number, unit) = split_known_unit(input).ok_or_else(invalid)?;
            let quantity = parse_quantity_answer(number)
                .filter(|quantity| !quantity.is_empty())
                .ok_or_else(invalid)?;
            (quantity, Some(unit))
        }
    };
    Ok(MeasurementMatch {
        quantity,
        measurement: unit,
        requires_quantity_confirmation: false,
        ..current.clone()
    })
}

/// "250 g" → ("250", "g"): the number before the first letter, when what follows is a known
/// unit (canonical spelling, like a typed line: "250 grammes" → "g").
fn split_known_unit(input: &str) -> Option<(&str, String)> {
    let start = input.find(char::is_alphabetic)?;
    let (number, unit) = input.split_at(start);
    let unit = crate::units::canonicalize_unit(unit.trim())?;
    Some((number.trim(), unit.as_str().to_string()))
}

/// Replaces the name only.
pub fn with_name(current: &MeasurementMatch, input: &str) -> Result<MeasurementMatch, AppError> {
    let name = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(AppError::Validation("edit-empty".to_string()));
    }
    if name.chars().count() > MAX_INGREDIENT_NAME_CHARS {
        return Err(AppError::Validation(
            "edit-ingredient-name-too-long".to_string(),
        ));
    }
    Ok(MeasurementMatch {
        ingredient_name: name,
        ..current.clone()
    })
}

/// Convert database ingredients to measurement matches for editing
///
/// This function transforms database-stored ingredients into the format expected
/// by the ingredient editing interface, which reuses the recipe creation workflow.
pub fn ingredients_to_measurement_matches(ingredients: &[Ingredient]) -> Vec<MeasurementMatch> {
    ingredients
        .iter()
        .enumerate()
        .map(|(i, ing)| MeasurementMatch {
            // No quantity ("sel") stays empty: "1" would show "1 sel" and could be saved
            quantity: ing
                .quantity
                .map(crate::extraction::mapping::format_quantity)
                .unwrap_or_default(),
            measurement: ing.unit.clone(),
            ingredient_name: ing.name.clone(),
            line_number: i, // Use array index as line number
            start_pos: 0,   // Not meaningful for database data
            end_pos: ing.name.len(),
            requires_quantity_confirmation: false, // Use name length as approximation
        })
        .collect()
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

        // Compare the stored values: no quantity is not "1", and "1/2" is 0.5
        let quantity_changed = match (
            orig.quantity,
            crate::validation::parse_quantity(&edit.quantity),
        ) {
            (Some(stored), Some(edited)) => (stored - edited).abs() > 1e-9,
            (None, None) => false,
            _ => true,
        };
        let unit_changed =
            orig.unit.as_deref().unwrap_or("") != edit.measurement.as_deref().unwrap_or("");

        if quantity_changed || unit_changed || orig.name != edit.ingredient_name {
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

    fn create_test_ingredient(
        id: i64,
        name: &str,
        quantity: Option<f64>,
        unit: Option<&str>,
    ) -> Ingredient {
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

    fn line(quantity: &str, unit: Option<&str>, name: &str, flagged: bool) -> MeasurementMatch {
        MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: unit.map(str::to_string),
            ingredient_name: name.to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 0,
            requires_quantity_confirmation: flagged,
        }
    }

    #[test]
    fn a_new_quantity_keeps_unit_and_name_and_clears_the_flag() {
        let edited = with_quantity(&line("0", Some("g"), "farine", true), "250").expect("valid");
        assert_eq!(edited, line("250", Some("g"), "farine", false));
        let none = with_quantity(&line("2", None, "sel", false), "-").expect("valid");
        assert_eq!(none.quantity, "");
        for invalid in ["0", "-3", "beaucoup", "1e9", "250 farine", "g", "0 g"] {
            assert!(
                matches!(with_quantity(&line("2", None, "sel", false), invalid),
                    Err(AppError::Validation(key)) if key == "edit-invalid-quantity-answer"),
                "{invalid}"
            );
        }
    }

    #[test]
    fn a_quantity_typed_with_its_unit_replaces_both() {
        let flour = line("0", Some("g"), "farine", true);
        for answer in ["250 g", "250g", " 250 G ", "250 grammes"] {
            let edited = with_quantity(&flour, answer).expect(answer);
            assert_eq!(edited.quantity, "250", "{answer}");
            assert_eq!(edited.measurement.as_deref(), Some("g"), "{answer}");
        }
        let edited = with_quantity(&line("1", None, "lait", false), "1/2 cup").expect("valid");
        assert_eq!(edited.quantity, "0.5");
        assert_eq!(edited.measurement.as_deref(), Some("cup"));
    }

    #[test]
    fn a_new_name_keeps_quantity_and_unit() {
        let edited =
            with_name(&line("200", Some("g"), "farin", true), "  farine   de blé ").expect("valid");
        assert_eq!(edited, line("200", Some("g"), "farine de blé", true));
        assert!(matches!(with_name(&line("1", None, "x", false), "   "),
            Err(AppError::Validation(key)) if key == "edit-empty"));
        assert!(
            matches!(with_name(&line("1", None, "x", false), &"a".repeat(101)),
            Err(AppError::Validation(key)) if key == "edit-ingredient-name-too-long")
        );
    }

    #[test]
    fn a_prompt_answer_replaces_an_ingredient_or_adds_one_at_the_end() {
        let mut list = vec![line("2", Some("g"), "sel", false)];
        assert!(put_ingredient(
            &mut list,
            0,
            line("3", Some("g"), "sel", false)
        ));
        assert!(put_ingredient(&mut list, 1, line("1", None, "oeuf", false)));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].quantity, "3");
        assert_eq!(list[1].ingredient_name, "oeuf");
        assert!(!put_ingredient(
            &mut list,
            5,
            line("1", None, "lait", false)
        ));
        assert_eq!(list.len(), 2, "beyond the end: unchanged");
    }

    #[test]
    fn apply_edit_parses_the_whole_line_by_default() {
        let current = line("1", None, "sel", false);
        let edited = apply_edit(IngredientField::Line, Some(&current), "3 eggs").expect("valid");
        assert_eq!(edited.ingredient_name, "eggs");
        let quantity_only =
            apply_edit(IngredientField::Quantity, Some(&current), "2").expect("valid");
        assert_eq!(quantity_only.ingredient_name, "sel");
        // Without the current ingredient (index out of range) the line is parsed
        assert!(apply_edit(IngredientField::Name, None, "2 cups flour").is_ok());
    }

    #[test]
    fn stored_values_parse_fractions_and_keep_no_quantity() {
        let half = line("1/2", Some("cup"), "sugar", false);
        assert_eq!(to_new_ingredient(&half).quantity, Some(0.5));
        let mixed = line("1 1/2", Some("cup"), "flour", false);
        assert_eq!(to_new_ingredient(&mixed).quantity, Some(1.5));
        let salt = line("", None, "sel", false);
        assert_eq!(to_new_ingredient(&salt).quantity, None);
        assert_eq!(to_new_ingredient(&salt).unit, None);
    }

    #[test]
    fn clearing_a_quantity_or_typing_a_fraction_is_a_change() {
        let stored = [create_test_ingredient(1, "sucre", Some(1.0), Some("cup"))];
        for edited_quantity in ["", "1/2", "1 1/2"] {
            let edited = [line(edited_quantity, Some("cup"), "sucre", false)];
            let changes = detect_ingredient_changes(&stored, &edited);
            assert_eq!(changes.to_update.len(), 1, "{edited_quantity:?}");
        }
        let unchanged = [line("1", Some("cup"), "sucre", false)];
        assert!(detect_ingredient_changes(&stored, &unchanged)
            .to_update
            .is_empty());
        let no_quantity = [create_test_ingredient(2, "sel", None, None)];
        let still_none = [line("", None, "sel", false)];
        assert!(detect_ingredient_changes(&no_quantity, &still_none)
            .to_update
            .is_empty());
    }

    #[test]
    fn saved_ingredients_without_quantity_stay_without_quantity() {
        let matches =
            ingredients_to_measurement_matches(&[create_test_ingredient(1, "sel", None, None)]);
        assert_eq!(matches[0].quantity, "");
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
                requires_quantity_confirmation: false,
            },
            MeasurementMatch {
                quantity: "1".to_string(),
                measurement: None,
                ingredient_name: "butter".to_string(), // Changed from sugar to butter
                line_number: 1,
                start_pos: 0,
                end_pos: 6,
                requires_quantity_confirmation: false,
            },
        ];

        let changes = detect_ingredient_changes(&original, &edited);

        // Should detect flour update and sugar->butter update
        assert_eq!(changes.to_update.len(), 2);
        assert_eq!(changes.to_add.len(), 0);
        assert_eq!(changes.to_delete.len(), 0);

        // Check that flour was updated
        let flour_update = changes
            .to_update
            .iter()
            .find(|(id, _)| *id == 1)
            .expect("Flour update should be detected");
        assert_eq!(flour_update.1.quantity, "3");
        assert_eq!(flour_update.1.ingredient_name, "flour");

        // Check that sugar was updated to butter
        let sugar_update = changes
            .to_update
            .iter()
            .find(|(id, _)| *id == 2)
            .expect("Sugar update should be detected");
        assert_eq!(sugar_update.1.quantity, "1");
        assert_eq!(sugar_update.1.ingredient_name, "butter");
        assert_eq!(sugar_update.1.measurement, None);
    }
}
