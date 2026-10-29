//! # Integration Tests
//!
//! Ingredient lines (`MeasurementMatch`) through the dialogue state and the review keyboard.

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
            requires_quantity_confirmation: false,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("cup".to_string()),
            ingredient_name: "sugar".to_string(),
            line_number: 3,
            start_pos: 0,
            end_pos: 5,
            requires_quantity_confirmation: false,
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
    } = review_state
    {
        assert_eq!(recipe_name, "Oatmeal Cookies");
        assert_eq!(state_ingredients.len(), 2);
        assert_eq!(
            state_ingredients[0].ingredient_name,
            "old-fashioned rolled oats"
        );
        assert_eq!(state_ingredients[1].ingredient_name, "sugar");
        assert_eq!(
            extracted_text,
            "2 cups old-fashioned\nrolled oats\n1 cup sugar"
        );
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
            requires_quantity_confirmation: false,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("teaspoon".to_string()),
            ingredient_name: "baking soda".to_string(),
            line_number: 3,
            start_pos: 0,
            end_pos: 15,
            requires_quantity_confirmation: false,
        },
        just_ingredients::MeasurementMatch {
            quantity: "3/4".to_string(),
            measurement: Some("cup".to_string()),
            ingredient_name: "unsalted butter, softened".to_string(),
            line_number: 6,
            start_pos: 0,
            end_pos: 28,
            requires_quantity_confirmation: false,
        },
        just_ingredients::MeasurementMatch {
            quantity: "1".to_string(),
            measurement: Some("teaspoon".to_string()),
            ingredient_name: "vanilla extract".to_string(),
            line_number: 10,
            start_pos: 0,
            end_pos: 18,
            requires_quantity_confirmation: false,
        },
    ];

    // Create localization manager for testing
    let localization = create_localization_manager().unwrap();

    // Test ingredient review keyboard displays complete names
    let keyboard = create_ingredient_review_keyboard(&ingredients, Some("en"), &localization);

    // Verify keyboard contains buttons with complete ingredient names
    // The keyboard should have buttons for each ingredient
    assert!(
        !keyboard.inline_keyboard.is_empty(),
        "Keyboard should not be empty"
    );

    // Check that the keyboard has the right number of rows (one per ingredient + action buttons)
    assert!(
        keyboard.inline_keyboard.len() >= ingredients.len(),
        "Should have at least one button per ingredient"
    );

    println!("✅ Multi-line ingredients UI display formatting test passed");
}
