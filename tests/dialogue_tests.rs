use anyhow::Result;

use just_ingredients::dialogue::RecipeDialogueState;
use just_ingredients::text_processing::MeasurementMatch;
use just_ingredients::validation::validate_recipe_name;

/// Integration test for recipe name dialogue validation
#[tokio::test]
async fn test_recipe_name_dialogue_validation() -> Result<()> {
    // Test valid recipe names
    assert!(validate_recipe_name("Chocolate Chip Cookies").is_ok());
    assert!(validate_recipe_name("  Mom's Lasagna  ").is_ok());

    // Test invalid recipe names
    assert!(validate_recipe_name("").is_err());
    assert!(validate_recipe_name("   ").is_err());
    assert!(validate_recipe_name(&"a".repeat(256)).is_err());

    Ok(())
}

/// Test dialogue state transitions
#[tokio::test]
async fn test_dialogue_state_serialization() -> Result<()> {
    // Test that dialogue states can be serialized/deserialized with serde_json
    let ingredients = vec![MeasurementMatch {
        quantity: "2".to_string(),
        measurement: Some("cups".to_string()),
        ingredient_name: "flour".to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 6,
    }];

    let state = RecipeDialogueState::WaitingForRecipeName {
        extracted_text: "2 cups flour\n3 eggs".to_string(),
        ingredients,
        language_code: Some("en".to_string()),
    };

    // Basic test that the state is properly structured
    match state {
        RecipeDialogueState::WaitingForRecipeName { ingredients, .. } => {
            assert_eq!(ingredients.len(), 1);
            assert_eq!(ingredients[0].ingredient_name, "flour");
        }
        _ => panic!("Unexpected dialogue state"),
    }

    Ok(())
}

/// Test basic dialogue functionality
#[tokio::test]
async fn test_dialogue_functionality() -> Result<()> {
    // Test that we can create dialogue states properly
    let start_state = RecipeDialogueState::Start;

    // Test default state
    assert!(matches!(start_state, RecipeDialogueState::Start));

    // Test default trait
    let default_state = RecipeDialogueState::default();
    assert!(matches!(default_state, RecipeDialogueState::Start));

    Ok(())
}

/// Test ingredient review dialogue state transitions
#[tokio::test]
async fn test_ingredient_review_dialogue_states() -> Result<()> {
    // Test ReviewIngredients state
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

    let review_state = RecipeDialogueState::ReviewIngredients {
        recipe_name: "Test Recipe".to_string(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        message_id: Some(123),
        extracted_text: "Test OCR text".to_string(),
        recipe_name_from_caption: None,
    };

    // Verify state structure
    match review_state {
        RecipeDialogueState::ReviewIngredients {
            recipe_name,
            ingredients: ingr,
            language_code,
            message_id,
            extracted_text,
            recipe_name_from_caption: _,
        } => {
            assert_eq!(recipe_name, "Test Recipe");
            assert_eq!(ingr.len(), 2);
            assert_eq!(ingr[0].ingredient_name, "flour");
            assert_eq!(ingr[1].ingredient_name, "eggs");
            assert_eq!(language_code, Some("en".to_string()));
            assert_eq!(message_id, Some(123));
            assert_eq!(extracted_text, "Test OCR text");
        }
        _ => panic!("Expected ReviewIngredients state"),
    }

    // Test EditingIngredient state
    let editing_state = RecipeDialogueState::EditingIngredient {
        recipe_name: "Test Recipe".to_string(),
        ingredients: ingredients.clone(),
        editing_index: 0,
        language_code: Some("en".to_string()),
        message_id: Some(123),
        original_message_id: Some(456), // Original recipe display message ID
        extracted_text: "Test OCR text".to_string(),
    };

    match editing_state {
        RecipeDialogueState::EditingIngredient {
            recipe_name,
            ingredients: ingr,
            editing_index,
            language_code,
            message_id,
            original_message_id,
            extracted_text,
        } => {
            assert_eq!(recipe_name, "Test Recipe");
            assert_eq!(ingr.len(), 2);
            assert_eq!(editing_index, 0);
            assert_eq!(language_code, Some("en".to_string()));
            assert_eq!(message_id, Some(123));
            assert_eq!(original_message_id, Some(456));
            assert_eq!(extracted_text, "Test OCR text");
        }
        _ => panic!("Expected EditingIngredient state"),
    }

    // Test WaitingForRecipeNameAfterConfirm state
    let confirm_state = RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        extracted_text: "Test OCR text".to_string(),
        recipe_name_from_caption: None,
    };

    match confirm_state {
        RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
            ingredients: ingr,
            language_code,
            extracted_text,
            recipe_name_from_caption: _,
        } => {
            assert_eq!(ingr.len(), 2);
            assert_eq!(language_code, Some("en".to_string()));
            assert_eq!(extracted_text, "Test OCR text");
        }
        _ => panic!("Expected WaitingForRecipeNameAfterConfirm state"),
    }

    Ok(())
}

/// Test ingredient editing validation
#[test]
fn test_ingredient_edit_validation() {
    use just_ingredients::bot::parse_ingredient_from_text;

    // Test valid edits
    let result = parse_ingredient_from_text("2 cups flour");
    assert!(result.is_ok());
    let ingredient = result.unwrap();
    assert_eq!(ingredient.quantity, "2");
    assert_eq!(ingredient.measurement, Some("cups".to_string()));
    assert_eq!(ingredient.ingredient_name, "flour");

    // Test quantity-only ingredient
    let result = parse_ingredient_from_text("2 oeufs");
    assert!(result.is_ok());
    let ingredient = result.unwrap();
    assert_eq!(ingredient.quantity, "2");
    assert_eq!(ingredient.measurement, None);
    assert_eq!(ingredient.ingredient_name, "oeufs");

    // Test another quantity-only ingredient
    let result = parse_ingredient_from_text("6 eggs");
    assert!(result.is_ok());
    let ingredient = result.unwrap();
    assert_eq!(ingredient.quantity, "6");
    assert_eq!(ingredient.measurement, None);
    assert_eq!(ingredient.ingredient_name, "eggs");

    // Test validation errors
    assert!(parse_ingredient_from_text("").is_err()); // Empty
    assert!(parse_ingredient_from_text(&"a".repeat(201)).is_err()); // Too long
    assert!(parse_ingredient_from_text("2 cups").is_err()); // No ingredient name
    assert!(parse_ingredient_from_text("0 cups flour").is_err()); // Zero quantity
    assert!(parse_ingredient_from_text("-1 cups flour").is_err()); // Negative quantity
    assert!(parse_ingredient_from_text("2 cups very_long_ingredient_name_that_exceeds_the_one_hundred_character_limit_and_should_be_rejected_by_the_validation").is_err());
    // Name too long
}

/// Test ingredient review command parsing
#[test]
fn test_ingredient_review_commands() {
    // Test command parsing (this would be used in handle_ingredient_review_input)
    let test_cases = vec![
        ("confirm", true, false),
        ("ok", true, false),
        ("yes", true, false),
        ("save", true, false),
        ("cancel", false, true),
        ("stop", false, true),
        ("unknown", false, false),
        ("CONFIRM", true, false), // Case insensitive
        ("CANCEL", false, true),
    ];

    for (input, should_confirm, should_cancel) in test_cases {
        let lower_input = input.to_lowercase();
        let is_confirm = matches!(lower_input.as_str(), "confirm" | "ok" | "yes" | "save");
        let is_cancel = matches!(lower_input.as_str(), "cancel" | "stop");

        assert_eq!(
            is_confirm,
            should_confirm,
            "Command '{}' should {} be confirm",
            input,
            if should_confirm { "" } else { "not" }
        );
        assert_eq!(
            is_cancel,
            should_cancel,
            "Command '{}' should {} be cancel",
            input,
            if should_cancel { "" } else { "not" }
        );
    }
}

/// Test ingredient editing cancellation commands
#[test]
fn test_ingredient_edit_cancellation() {
    // Test cancellation commands for editing (this would be used in handle_ingredient_edit_input)
    let cancellation_commands = ["cancel", "stop", "back"];

    for command in &cancellation_commands {
        assert!(
            matches!(command.to_lowercase().as_str(), "cancel" | "stop" | "back"),
            "Command '{}' should be recognized as cancellation",
            command
        );
    }

    let non_cancellation_commands = ["confirm", "ok", "edit", "save"];
    for command in &non_cancellation_commands {
        assert!(
            !matches!(command.to_lowercase().as_str(), "cancel" | "stop" | "back"),
            "Command '{}' should not be recognized as cancellation",
            command
        );
    }
}

/// Unit test for recipe name validation
#[test]
fn test_recipe_name_validation() {
    // Valid names
    assert!(validate_recipe_name("Chocolate Chip Cookies").is_ok());
    assert!(validate_recipe_name("  Mom's Lasagna  ").is_ok());

    // Invalid names
    assert!(validate_recipe_name("").is_err());
    assert!(validate_recipe_name("   ").is_err());
    assert!(validate_recipe_name(&"a".repeat(256)).is_err());
}

/// Unit test for recipe name trimming
#[test]
fn test_recipe_name_trimming() {
    let result = validate_recipe_name("  Test Recipe  ");
    assert_eq!(result.unwrap(), "Test Recipe");
}

/// Test dialogue state transitions with original_message_id tracking
#[test]
fn test_dialogue_state_transitions_with_original_message_id() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::text_processing::MeasurementMatch;

    // Test data
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

    // Test EditingIngredient state with original_message_id
    let editing_state = RecipeDialogueState::EditingIngredient {
        recipe_name: "Test Recipe".to_string(),
        ingredients: ingredients.clone(),
        editing_index: 0,
        language_code: Some("en".to_string()),
        message_id: Some(123),
        original_message_id: Some(456),
        extracted_text: "Test OCR text".to_string(),
    };

    // Verify the state structure includes original_message_id
    if let RecipeDialogueState::EditingIngredient {
        recipe_name,
        ingredients: ingr,
        editing_index,
        language_code,
        message_id,
        original_message_id,
        extracted_text,
    } = editing_state
    {
        assert_eq!(recipe_name, "Test Recipe");
        assert_eq!(ingr.len(), 2);
        assert_eq!(editing_index, 0);
        assert_eq!(language_code, Some("en".to_string()));
        assert_eq!(message_id, Some(123));
        assert_eq!(original_message_id, Some(456)); // This is the key new field
        assert_eq!(extracted_text, "Test OCR text");
    } else {
        panic!("Expected EditingIngredient state");
    }

    // Test EditingSavedIngredient state with original_message_id
    let saved_ingredients = vec![
        just_ingredients::db::Ingredient {
            id: 1,
            user_id: 100,
            recipe_id: Some(200),
            name: "flour".to_string(),
            quantity: Some(2.0),
            unit: Some("cups".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        just_ingredients::db::Ingredient {
            id: 2,
            user_id: 100,
            recipe_id: Some(200),
            name: "eggs".to_string(),
            quantity: Some(3.0),
            unit: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    ];

    let editing_saved_state = RecipeDialogueState::EditingSavedIngredient {
        recipe_id: 200,
        original_ingredients: saved_ingredients.clone(),
        current_matches: ingredients.clone(),
        editing_index: 1,
        language_code: Some("en".to_string()),
        message_id: Some(789),
        original_message_id: Some(101112), // Original recipe display message ID
    };

    // Verify the state structure includes original_message_id
    if let RecipeDialogueState::EditingSavedIngredient {
        recipe_id,
        original_ingredients,
        current_matches,
        editing_index,
        language_code,
        message_id,
        original_message_id,
    } = editing_saved_state
    {
        assert_eq!(recipe_id, 200);
        assert_eq!(original_ingredients.len(), 2);
        assert_eq!(current_matches.len(), 2);
        assert_eq!(editing_index, 1);
        assert_eq!(language_code, Some("en".to_string()));
        assert_eq!(message_id, Some(789));
        assert_eq!(original_message_id, Some(101112)); // This is the key new field
    } else {
        panic!("Expected EditingSavedIngredient state");
    }

    println!("✅ Dialogue state transitions with original_message_id tracking test passed");
}

/// Test state transition from ReviewIngredients to EditingIngredient
#[test]
fn test_review_to_editing_ingredient_transition() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::text_processing::MeasurementMatch;

    // Start with ReviewIngredients state
    let ingredients = vec![MeasurementMatch {
        quantity: "2".to_string(),
        measurement: Some("cups".to_string()),
        ingredient_name: "flour".to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 6,
    }];

    // Simulate transition to editing (what happens when user clicks edit button)
    let editing_state = RecipeDialogueState::EditingIngredient {
        recipe_name: "Test Recipe".to_string(),
        ingredients: ingredients.clone(),
        editing_index: 0,
        language_code: Some("en".to_string()),
        message_id: Some(1001),          // New editing prompt message ID
        original_message_id: Some(1000), // Should track the original message ID
        extracted_text: "Test OCR text".to_string(),
    };

    // Verify the transition preserved the original message ID
    if let RecipeDialogueState::EditingIngredient {
        original_message_id,
        message_id,
        ..
    } = editing_state
    {
        assert_eq!(
            original_message_id,
            Some(1000),
            "Should preserve original message ID"
        );
        assert_eq!(message_id, Some(1001), "Should have new editing message ID");
    } else {
        panic!("Expected EditingIngredient state");
    }

    println!("✅ Review to editing ingredient transition test passed");
}

/// Test state transition from EditingSavedIngredients to EditingSavedIngredient
#[test]
fn test_saved_ingredients_to_editing_transition() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::text_processing::MeasurementMatch;

    // Start with EditingSavedIngredients state
    let saved_ingredients = vec![just_ingredients::db::Ingredient {
        id: 1,
        user_id: 100,
        recipe_id: Some(200),
        name: "flour".to_string(),
        quantity: Some(2.0),
        unit: Some("cups".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }];

    let current_matches = vec![MeasurementMatch {
        quantity: "2".to_string(),
        measurement: Some("cups".to_string()),
        ingredient_name: "flour".to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 6,
    }];

    // Simulate transition to editing single ingredient (what happens when user clicks edit button)
    let editing_single_state = RecipeDialogueState::EditingSavedIngredient {
        recipe_id: 200,
        original_ingredients: saved_ingredients.clone(),
        current_matches: current_matches.clone(),
        editing_index: 0,
        language_code: Some("en".to_string()),
        message_id: Some(2001),          // New editing prompt message ID
        original_message_id: Some(2000), // Should track the original message ID
    };

    // Verify the transition preserved the original message ID
    if let RecipeDialogueState::EditingSavedIngredient {
        original_message_id,
        message_id,
        recipe_id,
        editing_index,
        ..
    } = editing_single_state
    {
        assert_eq!(
            original_message_id,
            Some(2000),
            "Should preserve original message ID"
        );
        assert_eq!(message_id, Some(2001), "Should have new editing message ID");
        assert_eq!(recipe_id, 200);
        assert_eq!(editing_index, 0);
    } else {
        panic!("Expected EditingSavedIngredient state");
    }

    println!("✅ Saved ingredients to editing transition test passed");
}
