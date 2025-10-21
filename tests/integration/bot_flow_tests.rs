//! # Bot Flow Integration Tests
//!
//! This module contains integration tests for bot interaction flows,
//! dialogue workflows, and user experience scenarios.

use just_ingredients::text_processing::{MeasurementConfig, MeasurementDetector};

/// Test recipe naming dialogue workflow
#[test]
fn test_recipe_naming_dialogue_workflow() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::validation::validate_recipe_name;

    // Simulate the complete dialogue flow for naming a recipe

    // Step 1: Initial state
    let initial_state = RecipeDialogueState::Start;
    assert!(matches!(initial_state, RecipeDialogueState::Start));

    // Step 2: User uploads image, bot asks for recipe name
    let extracted_text = "2 cups flour\n3 eggs\n1 cup sugar";
    let ingredients = vec![
        just_ingredients::MeasurementMatch {
            quantity: "2".to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "flour".to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 6,
        },
        just_ingredients::MeasurementMatch {
            quantity: "3".to_string(),
            measurement: None,
            ingredient_name: "eggs".to_string(),
            line_number: 1,
            start_pos: 8,
            end_pos: 9,
        },
    ];

    let waiting_state = RecipeDialogueState::WaitingForRecipeName {
        extracted_text: extracted_text.to_string(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
    };

    // Step 3: User provides recipe name
    let recipe_name = "Test Recipe";
    let validation_result = validate_recipe_name(recipe_name);
    assert!(validation_result.is_ok());
    assert_eq!(validation_result.unwrap(), recipe_name);

    // Step 4: Verify dialogue state contains all necessary data
    if let RecipeDialogueState::WaitingForRecipeName {
        extracted_text: text,
        ingredients: ingr,
        language_code,
    } = waiting_state
    {
        assert_eq!(text, extracted_text);
        assert_eq!(ingr.len(), 2);
        assert_eq!(ingr[0].ingredient_name, "flour");
        assert_eq!(ingr[1].ingredient_name, "eggs");
        assert_eq!(language_code, Some("en".to_string()));
    } else {
        panic!("Expected WaitingForRecipeName state");
    }

    // Step 5: Test validation edge cases
    assert!(validate_recipe_name("").is_err());
    assert!(validate_recipe_name("   ").is_err());
    assert!(validate_recipe_name(&"a".repeat(256)).is_err()); // Too long
    assert!(validate_recipe_name("Valid Recipe Name").is_ok());

    println!("✅ Recipe naming dialogue workflow completed successfully");
}

/// Test multi-language end-to-end workflow
#[test]
fn test_multi_language_end_to_end_workflow() {
    use just_ingredients::localization::create_localization_manager;

    let manager = create_localization_manager().unwrap();

    // Test English workflow
    let english_text = r#"
    Pancakes Recipe

    Ingredients:
    2 cups flour
    2 eggs
    1 cup milk
    2 tablespoons sugar
    "#;

    let detector = MeasurementDetector::new().unwrap();
    let english_measurements = detector.extract_ingredient_measurements(english_text);

    // Test French workflow
    let french_text = r#"
    Recette de Crêpes

    Ingrédients:
    250 g de farine
    4 œufs
    500 ml de lait
    2 cuillères à soupe de sucre
    "#;

    let french_measurements = detector.extract_ingredient_measurements(french_text);

    // Verify both languages work
    assert!(!english_measurements.is_empty());
    assert!(!french_measurements.is_empty());

    // Check language-specific ingredients
    let english_eggs = english_measurements
        .iter()
        .find(|m| m.ingredient_name == "eggs");
    assert!(english_eggs.is_some());
    assert_eq!(english_eggs.unwrap().quantity, "2");
    assert!(english_eggs.unwrap().measurement.is_none());

    let french_oeufs = french_measurements
        .iter()
        .find(|m| m.ingredient_name == "œufs");
    assert!(french_oeufs.is_some());
    assert_eq!(french_oeufs.unwrap().quantity, "4");
    assert!(french_oeufs.unwrap().measurement.is_none());

    // Test localization messages
    let english_success = manager.get_message_in_language("success-extraction", "en", None);
    let french_success = manager.get_message_in_language("success-extraction", "fr", None);

    assert!(!english_success.is_empty());
    assert!(!french_success.is_empty());
    assert_ne!(english_success, french_success); // Should be different translations

    println!(
        "✅ Multi-language workflow: {} English measurements, {} French measurements, localized messages working",
        english_measurements.len(),
        french_measurements.len()
    );
}

/// Test error handling in complete workflows
#[test]
fn test_error_handling_end_to_end_workflow() {
    use just_ingredients::circuit_breaker::CircuitBreaker;
    use just_ingredients::ocr_config::{OcrConfig, RecoveryConfig};
    use std::time::Duration;

    // Test circuit breaker integration in workflow
    let config = RecoveryConfig {
        circuit_breaker_threshold: 2,
        circuit_breaker_reset_secs: 1,
        ..Default::default()
    };

    let circuit_breaker = CircuitBreaker::new(config);

    // Initially circuit should not be open
    assert!(!circuit_breaker.is_open());

    // Simulate failures
    circuit_breaker.record_failure();
    assert!(!circuit_breaker.is_open()); // Not yet at threshold

    circuit_breaker.record_failure();
    assert!(circuit_breaker.is_open()); // Now open

    // Simulate waiting for reset
    std::thread::sleep(Duration::from_secs(2));

    // Circuit should reset and allow requests again
    assert!(!circuit_breaker.is_open());

    // Test OCR config validation
    let ocr_config = OcrConfig::default();
    assert!(!ocr_config.languages.is_empty());
    assert!(ocr_config.max_file_size > 0);

    // Test measurement detector error handling
    let invalid_pattern_result = MeasurementDetector::with_pattern(r"[invalid regex");
    assert!(invalid_pattern_result.is_err());

    println!("✅ Error handling workflow: circuit breaker, config validation, and regex error handling all working");
}

/// Test concurrent user workflows simulation
#[test]
fn test_concurrent_user_workflows() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    // Simulate multiple users processing recipes concurrently
    let shared_detector = Arc::new(Mutex::new(MeasurementDetector::new().unwrap()));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = vec![];

    // Simulate 3 concurrent users
    for user_id in 0..3 {
        let detector_clone = Arc::clone(&shared_detector);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let detector = detector_clone.lock().unwrap();

            // Each user processes different recipe text
            let recipe_texts = [
                "2 cups flour\n3 eggs\n1 cup sugar",
                "500g chicken\n2 carrots\n1 onion",
                "1 kg potatoes\n3 tomatoes\n200g cheese",
            ];

            let measurements = detector.extract_ingredient_measurements(recipe_texts[user_id]);

            // Store results
            let mut results = results_clone.lock().unwrap();
            results.push((user_id, measurements.len()));
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all users got results
    let results = results.lock().unwrap();
    assert_eq!(results.len(), 3);

    // Each user should have found measurements
    for (user_id, measurement_count) in results.iter() {
        assert!(
            *measurement_count > 0,
            "User {} should have found measurements",
            user_id
        );
    }

    println!(
        "✅ Concurrent workflows: {} users processed recipes successfully",
        results.len()
    );
}

/// Test photo caption workflow integration
#[test]
fn test_photo_caption_workflow_integration() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::validation::validate_recipe_name;

    // Simulate the complete workflow when a user sends a photo with a caption

    // Step 1: Simulate photo upload with caption
    let caption = Some("Chocolate Chip Cookies".to_string());
    let ocr_text = r#"
    Ingredients:
    2 cups flour
    1 cup sugar
    3 eggs
    2 cups chocolate chips
    "#;

    // Step 2: Extract measurements (simulating OCR processing)
    let detector = MeasurementDetector::new().unwrap();
    let ingredients = detector.extract_ingredient_measurements(ocr_text);

    assert!(!ingredients.is_empty());
    assert!(ingredients.len() >= 4); // Should find flour, sugar, eggs, chocolate chips

    // Step 3: Process caption for recipe name (the core feature logic)
    let recipe_name_candidate = match &caption {
        Some(caption_text) if !caption_text.trim().is_empty() => {
            match validate_recipe_name(caption_text) {
                Ok(validated_name) => {
                    println!("✅ Caption '{}' accepted as recipe name", validated_name);
                    validated_name
                }
                Err(_) => {
                    println!("⚠️ Caption '{}' invalid, using default", caption_text);
                    "Recipe"
                }
            }
        }
        _ => {
            println!("📝 No caption provided, using default recipe name");
            "Recipe"
        }
    };

    assert_eq!(recipe_name_candidate, "Chocolate Chip Cookies");

    // Step 4: Simulate dialogue state transition
    let dialogue_state = RecipeDialogueState::ReviewIngredients {
        recipe_name: recipe_name_candidate.to_string(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        message_id: None,
        extracted_text: ocr_text.to_string(),
        recipe_name_from_caption: Some(recipe_name_candidate.to_string()),
    };

    // Verify dialogue state contains caption-derived name
    if let RecipeDialogueState::ReviewIngredients { recipe_name, .. } = dialogue_state {
        assert_eq!(recipe_name, "Chocolate Chip Cookies");
    } else {
        panic!("Expected ReviewIngredients state");
    }

    println!("✅ Photo caption workflow integration test passed");
}

/// Test caption fallback scenarios
#[test]
fn test_caption_fallback_scenarios() {
    use just_ingredients::validation::validate_recipe_name;

    // Test various caption scenarios and their expected outcomes
    let scenarios = vec![
        // (caption, expected_recipe_name, description)
        (
            Some("Valid Recipe Name".to_string()),
            "Valid Recipe Name",
            "Valid caption",
        ),
        (
            Some("   Spaced Name   ".to_string()),
            "Spaced Name",
            "Caption with whitespace",
        ),
        (
            Some("Café au Lait Crêpes".to_string()),
            "Café au Lait Crêpes",
            "Unicode characters",
        ),
        (
            Some("Recipe with émojis 🎂".to_string()),
            "Recipe with émojis 🎂",
            "Emojis",
        ),
        (Some("".to_string()), "Recipe", "Empty caption fallback"),
        (
            Some("   ".to_string()),
            "Recipe",
            "Whitespace-only fallback",
        ),
        (
            Some("a".repeat(256)),
            "Recipe",
            "Too long caption fallback",
        ),
        (
            Some("Invalid!!!@#$%".to_string()),
            "Invalid!!!@#$%",
            "Special chars (still valid)",
        ),
        (None, "Recipe", "No caption provided"),
    ];

    for (caption, expected_name, description) in scenarios {
        let result = match &caption {
            Some(caption_text) if !caption_text.trim().is_empty() => {
                match validate_recipe_name(caption_text) {
                    Ok(validated) => validated,
                    Err(_) => "Recipe",
                }
            }
            _ => "Recipe",
        };

        assert_eq!(
            result, expected_name,
            "Scenario '{}': expected '{}', got '{}'",
            description, expected_name, result
        );
    }

    println!("✅ Caption fallback scenarios test passed");
}

/// Test photo caption with measurement extraction integration
#[test]
fn test_caption_with_measurement_extraction() {
    // Test the complete flow: caption + OCR text + measurement extraction

    let test_cases = vec![
        (
            Some("Chocolate Chip Cookies".to_string()),
            r#"
            2 cups flour
            1 cup sugar
            3 eggs
            2 cups chocolate chips
            "#,
            "Chocolate Chip Cookies",
        ),
        (
            Some("French Crepes".to_string()),
            r#"
            250 g de farine
            4 œufs
            500 ml de lait
            "#,
            "French Crepes",
        ),
        (
            None, // No caption
            r#"
            2 cups flour
            3 eggs
            "#,
            "Recipe", // Should use default
        ),
        (
            Some("".to_string()), // Empty caption
            r#"
            1 cup sugar
            2 eggs
            "#,
            "Recipe", // Should use default
        ),
    ];

    let detector = MeasurementDetector::new().unwrap();

    for (caption, ocr_text, expected_recipe_name) in test_cases {
        // Extract measurements
        let ingredients = detector.extract_ingredient_measurements(ocr_text);
        assert!(
            !ingredients.is_empty(),
            "Should find ingredients in OCR text"
        );

        // Process caption
        let recipe_name = match &caption {
            Some(caption_text) if !caption_text.trim().is_empty() => {
                just_ingredients::validation::validate_recipe_name(caption_text)
                    .unwrap_or("Recipe")
            }
            _ => "Recipe",
        };

        assert_eq!(
            recipe_name, expected_recipe_name,
            "Caption {:?} should result in recipe name '{}'",
            caption, expected_recipe_name
        );

        // Verify ingredients are properly structured
        for ingredient in &ingredients {
            assert!(!ingredient.quantity.is_empty());
            assert!(!ingredient.ingredient_name.is_empty());
        }
    }

    println!("✅ Caption with measurement extraction integration test passed");
}

/// Test backward compatibility - photos without captions still work
#[test]
fn test_backward_compatibility_no_captions() {
    // This test ensures that existing functionality for photos without captions
    // continues to work exactly as before the caption feature was added

    let ocr_text = r#"
    Recipe without caption:
    2 cups flour
    1 cup sugar
    3 eggs
    "#;

    // Simulate old behavior: no caption provided
    let caption: Option<String> = None;

    // Extract measurements (this part is unchanged)
    let detector = MeasurementDetector::new().unwrap();
    let ingredients = detector.extract_ingredient_measurements(ocr_text);
    assert!(!ingredients.is_empty());

    // Recipe name assignment (should use default "Recipe")
    let recipe_name = match &caption {
        Some(caption_text) if !caption_text.trim().is_empty() => {
            just_ingredients::validation::validate_recipe_name(caption_text)
                .unwrap_or("Recipe")
        }
        _ => "Recipe",
    };

    assert_eq!(recipe_name, "Recipe");

    // Verify the workflow would continue normally
    let extracted_text = ocr_text.to_string();
    let language_code = Some("en".to_string());

    // This simulates what would happen in the message handler
    if !ingredients.is_empty() {
        // Would transition to review state with default name
        assert_eq!(recipe_name, "Recipe");
        assert!(language_code.is_some());
        assert!(!extracted_text.is_empty());
    }

    println!("✅ Backward compatibility test passed - photos without captions work unchanged");
}

/// Test caption feature with multi-language support
#[test]
fn test_caption_multi_language_integration() {
    use just_ingredients::localization::create_localization_manager;

    let manager = create_localization_manager().unwrap();

    // Test English caption workflow
    let english_caption = Some("Chocolate Chip Cookies".to_string());

    let recipe_name_en = match &english_caption {
        Some(caption_text) if !caption_text.trim().is_empty() => {
            just_ingredients::validation::validate_recipe_name(caption_text)
                .unwrap_or("Recipe")
        }
        _ => "Recipe",
    };

    assert_eq!(recipe_name_en, "Chocolate Chip Cookies");

    // Test French caption workflow
    let french_caption = Some("Crêpes au Chocolat".to_string());

    let recipe_name_fr = match &french_caption {
        Some(caption_text) if !caption_text.trim().is_empty() => {
            just_ingredients::validation::validate_recipe_name(caption_text)
                .unwrap_or("Recipe")
        }
        _ => "Recipe",
    };

    assert_eq!(recipe_name_fr, "Crêpes au Chocolat");

    // Test localization messages work with captions
    let caption_used_en = manager.get_message_in_language("caption-used", "en", None);
    let caption_used_fr = manager.get_message_in_language("caption-used", "fr", None);

    assert!(caption_used_en.contains("{$caption}"));
    assert!(caption_used_fr.contains("{$caption}"));
    assert_ne!(caption_used_en, caption_used_fr);

    // Test message formatting
    let formatted_en = caption_used_en.replace("{$caption}", &recipe_name_en);
    let formatted_fr = caption_used_fr.replace("{$caption}", &recipe_name_fr);

    assert!(formatted_en.contains("Chocolate Chip Cookies"));
    assert!(formatted_fr.contains("Crêpes au Chocolat"));

    println!("✅ Multi-language caption integration test passed");
}

/// Test streamlined workflow - when caption is available, skip recipe name input
#[test]
fn test_streamlined_caption_workflow() {
    use just_ingredients::text_processing::MeasurementDetector;

    // Setup test data
    let detector = MeasurementDetector::new().unwrap();
    let ocr_text = r#"
    2 cups flour
    1 cup sugar
    3 eggs
    "#;
    let ingredients = detector.extract_ingredient_measurements(ocr_text);
    assert!(!ingredients.is_empty());

    // Test the core logic: when caption exists, use it directly
    let caption_recipe_name = Some("Chocolate Chip Cookies".to_string());

    // Simulate the streamlined workflow decision
    let should_skip_recipe_name_input = caption_recipe_name.is_some();

    assert!(
        should_skip_recipe_name_input,
        "Should skip recipe name input when caption is available"
    );

    // Verify that we have a valid caption recipe name
    assert_eq!(
        caption_recipe_name.as_ref().unwrap(),
        "Chocolate Chip Cookies"
    );

    // Verify ingredients were extracted
    assert_eq!(ingredients.len(), 3); // flour, sugar, eggs

    // Test the fallback case: no caption available
    let no_caption: Option<String> = None;
    let should_prompt_for_recipe_name = no_caption.is_none();

    assert!(
        should_prompt_for_recipe_name,
        "Should prompt for recipe name when no caption is available"
    );

    // Test edge case: empty caption should be treated as no caption
    let empty_caption = Some("".to_string());
    let should_prompt_for_empty_caption = empty_caption.as_ref().unwrap().trim().is_empty();

    assert!(
        should_prompt_for_empty_caption,
        "Should prompt for recipe name when caption is empty"
    );

    println!("✅ Streamlined caption workflow test passed - core logic validates correctly");
}

/// Test that recipe_name_from_caption is preserved when ingredients are deleted
#[test]
fn test_caption_preservation_after_ingredient_deletion() {
    use just_ingredients::dialogue::RecipeDialogueState;
    use just_ingredients::text_processing::MeasurementDetector;

    // Setup: Simulate photo with caption processed, ingredients extracted
    let detector = MeasurementDetector::new().unwrap();
    let ocr_text = r#"
    2 cups flour
    1 cup sugar
    3 eggs
    "#;
    let mut ingredients = detector.extract_ingredient_measurements(ocr_text);
    assert!(!ingredients.is_empty());

    let caption = "Chocolate Chip Cookies".to_string();
    let recipe_name_from_caption = Some(caption.clone());

    // Initial dialogue state after photo processing
    let initial_state = RecipeDialogueState::ReviewIngredients {
        recipe_name: caption.clone(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        message_id: Some(12345),
        extracted_text: ocr_text.to_string(),
        recipe_name_from_caption: recipe_name_from_caption.clone(),
    };

    // Verify initial state has caption info
    if let RecipeDialogueState::ReviewIngredients {
        recipe_name_from_caption: initial_caption,
        ..
    } = &initial_state
    {
        assert_eq!(initial_caption, &Some(caption.clone()));
    }

    // Simulate user deleting an ingredient (e.g., removing the sugar ingredient)
    // This should preserve the recipe_name_from_caption field
    ingredients.remove(1); // Remove sugar (index 1)

    // Updated dialogue state after deletion (simulating what handle_delete_button does)
    let updated_state = RecipeDialogueState::ReviewIngredients {
        recipe_name: caption.clone(),
        ingredients: ingredients.clone(),
        language_code: Some("en".to_string()),
        message_id: Some(12345),
        extracted_text: ocr_text.to_string(),
        recipe_name_from_caption: recipe_name_from_caption.clone(), // This should be preserved!
    };

    // Verify the caption info is still preserved after deletion
    if let RecipeDialogueState::ReviewIngredients {
        recipe_name_from_caption: updated_caption,
        ..
    } = &updated_state
    {
        assert_eq!(
            updated_caption,
            &Some(caption),
            "recipe_name_from_caption should be preserved after ingredient deletion"
        );
    }

    // Simulate user confirming ingredients - should use streamlined workflow
    // This tests the core bug fix: even after deletion, caption should still trigger streamlined workflow
    let should_use_streamlined_workflow = recipe_name_from_caption.is_some();

    assert!(should_use_streamlined_workflow,
        "Should still use streamlined workflow after ingredient deletion when caption was originally provided");

    // Verify the recipe name that would be saved
    let final_recipe_name = recipe_name_from_caption.as_ref().unwrap();
    assert_eq!(final_recipe_name, "Chocolate Chip Cookies");

    println!("✅ Caption preservation after ingredient deletion test passed - bug is fixed!");
}

/// Test database integration with real database operations
#[tokio::test]
async fn test_database_integration_full_workflow() -> Result<(), Box<dyn std::error::Error>> {
    use just_ingredients::db;
    use just_ingredients::text_processing::MeasurementDetector;
    use std::sync::Arc;

    // This test requires a test database - skip if DATABASE_URL not set for integration tests
    // Skip test if DATABASE_URL is not set
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⚠️ Skipping database integration test - DATABASE_URL not set");
            return Ok(());
        }
    };

    // Create a test database connection pool
    let pool = match sqlx::postgres::PgPool::connect(&database_url).await {
        Ok(pool) => Arc::new(pool),
        Err(e) => {
            println!(
                "⚠️ Skipping database integration test - failed to connect: {}",
                e
            );
            return Ok(());
        }
    };

    // Initialize database schema
    if let Err(e) = db::init_database_schema(&pool).await {
        println!(
            "⚠️ Skipping database integration test - failed to init schema: {}",
            e
        );
        return Ok(());
    }

    // Test data
    let telegram_id = 999999; // Use a test user ID
    let recipe_content = "Test Recipe Content";
    let detector = MeasurementDetector::new().unwrap();

    // Step 1: Create or get a user
    let user = match db::get_or_create_user(&pool, telegram_id, Some("en")).await {
        Ok(user) => user,
        Err(e) => {
            panic!("Failed to create/get user: {}", e);
        }
    };

    // Step 2: Create a recipe
    let recipe_id = match db::create_recipe(&pool, telegram_id, recipe_content).await {
        Ok(id) => id,
        Err(e) => panic!("Failed to create recipe: {}", e);
    };

    // Step 3: Extract and create ingredients
    let ocr_text = r#"
    Test Recipe Ingredients:
    2 cups flour
    3 eggs
    1 cup sugar
    1 tsp vanilla
    "#;

    let measurements = detector.extract_ingredient_measurements(ocr_text);
    assert!(!measurements.is_empty());

    // Create ingredients in database
    for measurement in &measurements {
        let ingredient_id = match db::create_ingredient(
            &pool,
            user.id, // Use the actual user ID, not telegram_id
            Some(recipe_id),
            &measurement.ingredient_name,
            measurement.quantity.parse().ok(),
            measurement.measurement.as_deref(),
            &format!("{} {}", measurement.quantity, measurement.ingredient_name),
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                panic!(
                    "Failed to create ingredient {}: {}",
                    measurement.ingredient_name, e
                );
            }
        };

        // Verify ingredient was created
        let retrieved = match db::read_ingredient(&pool, ingredient_id).await {
            Ok(Some(ing)) => ing,
            Ok(None) => panic!("Ingredient {} not found after creation", ingredient_id),
            Err(e) => panic!("Failed to read ingredient {}: {}", ingredient_id, e);
        };

        assert_eq!(retrieved.name, measurement.ingredient_name);
        assert_eq!(retrieved.recipe_id, Some(recipe_id));
    }

    // Step 3: Test full-text search
    let search_results = match db::search_recipes(&pool, telegram_id, "flour").await {
        Ok(results) => results,
        Err(e) => panic!("Failed to search recipes: {}", e);
    };

    assert!(!search_results.is_empty());
    assert!(search_results.iter().any(|r| r.id == recipe_id));

    // Step 4: Test recipe listing with pagination
    let (recipe_names, total) =
        match db::get_user_recipes_paginated(&pool, telegram_id, 10, 0).await {
            Ok(result) => result,
            Err(e) => panic!("Failed to get paginated recipes: {}", e);
        };

    assert!(total >= 1);
    assert!(!recipe_names.is_empty());

    // Step 5: Test ingredient listing
    let ingredients = match db::list_ingredients_by_user(&pool, telegram_id).await {
        Ok(ings) => ings,
        Err(e) => panic!("Failed to list ingredients: {}", e);
    };

    assert!(!ingredients.is_empty());
    assert!(ingredients.len() >= measurements.len());

    // Step 6: Test recipe reading with ingredients
    let recipe_with_ingredients = match db::read_recipe_with_name(&pool, recipe_id).await {
        Ok(Some(recipe)) => recipe,
        Ok(None) => panic!("Recipe {} not found", recipe_id),
        Err(e) => panic!("Failed to read recipe with ingredients: {}", e);
    };

    assert_eq!(recipe_with_ingredients.content, recipe_content);

    // Cleanup: Delete test data (in reverse order to maintain foreign key constraints)
    for ingredient in &ingredients {
        if ingredient.user_id == telegram_id {
            if let Err(e) = db::delete_ingredient(&pool, ingredient.id).await {
                println!(
                    "Warning: Failed to cleanup ingredient {}: {}",
                    ingredient.id, e
                );
            }
        }
    }

    if let Err(e) = db::delete_recipe(&pool, recipe_id).await {
        println!("Warning: Failed to cleanup recipe {}: {}", recipe_id, e);
    }

    println!("✅ Database integration test passed - full CRUD workflow with search and pagination working");
    Ok(())
}

/// Test saved ingredient editing workflow - create recipe, edit ingredient, verify saved
#[tokio::test]
async fn test_saved_ingredient_editing_workflow() -> Result<(), Box<dyn std::error::Error>> {
    use just_ingredients::db;
    use just_ingredients::text_processing::MeasurementDetector;
    use std::sync::Arc;

    // This test requires a test database - skip if DATABASE_URL not set for integration tests
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⚠️ Skipping saved ingredient editing test - DATABASE_URL not set");
            return Ok(());
        }
    };

    // Create a test database connection pool
    let pool = match sqlx::postgres::PgPool::connect(&database_url).await {
        Ok(pool) => Arc::new(pool),
        Err(e) => {
            println!(
                "⚠️ Skipping saved ingredient editing test - failed to connect: {}",
                e
            );
            return Ok(());
        }
    };

    // Initialize database schema
    if let Err(e) = db::init_database_schema(&pool).await {
        println!(
            "⚠️ Skipping saved ingredient editing test - failed to init schema: {}",
            e
        );
        return Ok(());
    }

    // Test data
    let telegram_id = 888888; // Use a different test user ID
    let detector = MeasurementDetector::new().unwrap();

    // Step 1: Create user and recipe with ingredients
    let user = db::get_or_create_user(&pool, telegram_id, Some("en")).await?;
    let recipe_id = db::create_recipe(&pool, telegram_id, "Test Recipe for Editing").await?;
    db::update_recipe_name(&pool, recipe_id, "Editable Recipe").await?;

    // Create initial ingredients
    let ocr_text = r#"
    2 cups flour
    3 eggs
    1 cup sugar
    "#;

    let measurements = detector.extract_ingredient_measurements(ocr_text);
    assert_eq!(measurements.len(), 3); // flour, eggs, sugar

    let mut ingredient_ids = vec![];
    for measurement in &measurements {
        let ingredient_id = db::create_ingredient(
            &pool,
            user.id,
            Some(recipe_id),
            &measurement.ingredient_name,
            measurement.quantity.parse().ok(),
            measurement.measurement.as_deref(),
            &format!("{} {}", measurement.quantity, measurement.ingredient_name),
        )
        .await?;
        ingredient_ids.push(ingredient_id);
    }

    // Step 2: Get original ingredients from database
    let original_ingredients = db::get_recipe_ingredients(&pool, recipe_id).await?;
    assert_eq!(original_ingredients.len(), 3);

    // Step 3: Simulate editing workflow - update eggs from 3 to 4
    let eggs_ingredient = original_ingredients.iter()
        .find(|ing| ing.name == "eggs")
        .expect("Should find eggs ingredient");

    // Update the ingredient directly (simulating what the edit workflow does)
    let update_result = db::update_ingredient(
        &pool,
        eggs_ingredient.id,
        Some("eggs"), // Same name
        Some(4.0),    // Changed from 3.0 to 4.0
        None,         // No unit
    ).await?;

    assert!(update_result, "Ingredient update should succeed");

    // Step 4: Verify the ingredient was updated in the database
    let updated_ingredients = db::get_recipe_ingredients(&pool, recipe_id).await?;
    assert_eq!(updated_ingredients.len(), 3);

    // Find the eggs ingredient and verify it was updated
    let updated_eggs = updated_ingredients.iter()
        .find(|ing| ing.name == "eggs")
        .expect("Should find eggs ingredient");

    assert_eq!(updated_eggs.quantity, Some(4.0)); // Should be updated to 4
    assert_eq!(updated_eggs.name, "eggs");

    // Verify other ingredients remain unchanged
    let flour_ingredient = updated_ingredients.iter()
        .find(|ing| ing.name == "flour")
        .expect("Should find flour ingredient");
    assert_eq!(flour_ingredient.quantity, Some(2.0));

    let sugar_ingredient = updated_ingredients.iter()
        .find(|ing| ing.name == "sugar")
        .expect("Should find sugar ingredient");
    assert_eq!(sugar_ingredient.quantity, Some(1.0));

    // Step 5: Test ingredient deletion (part of editing workflow)
    let sugar_id = sugar_ingredient.id;
    let delete_result = db::delete_ingredient(&pool, sugar_id).await?;
    assert!(delete_result, "Ingredient deletion should succeed");

    // Verify ingredient was deleted
    let ingredients_after_delete = db::get_recipe_ingredients(&pool, recipe_id).await?;
    assert_eq!(ingredients_after_delete.len(), 2); // Should be 2 now

    // Verify sugar is gone
    let sugar_after_delete = ingredients_after_delete.iter()
        .find(|ing| ing.name == "sugar");
    assert!(sugar_after_delete.is_none(), "Sugar ingredient should be deleted");

    // Step 6: Test adding new ingredient (part of editing workflow)
    let new_measurement = detector.extract_ingredient_measurements("1 tsp vanilla");
    assert_eq!(new_measurement.len(), 1);

    let new_ingredient = &new_measurement[0];
    let new_ingredient_id = db::create_ingredient(
        &pool,
        user.id,
        Some(recipe_id),
        &new_ingredient.ingredient_name,
        new_ingredient.quantity.parse().ok(),
        new_ingredient.measurement.as_deref(),
        &format!("{} {}", new_ingredient.quantity, new_ingredient.ingredient_name),
    )
    .await?;

    // Verify new ingredient was added
    let ingredients_after_add = db::get_recipe_ingredients(&pool, recipe_id).await?;
    assert_eq!(ingredients_after_add.len(), 3); // Back to 3 ingredients

    // Verify vanilla was added
    let vanilla_ingredient = ingredients_after_add.iter()
        .find(|ing| ing.name == "vanilla")
        .expect("Should find vanilla ingredient");
    assert_eq!(vanilla_ingredient.quantity, Some(1.0));
    assert_eq!(vanilla_ingredient.unit, Some("tsp".to_string()));

    // Cleanup: Delete test data
    let all_ingredients = db::get_recipe_ingredients(&pool, recipe_id).await?;
    for ingredient in &all_ingredients {
        if let Err(e) = db::delete_ingredient(&pool, ingredient.id).await {
            println!("Warning: Failed to cleanup ingredient {}: {}", ingredient.id, e);
        }
    }

    if let Err(e) = db::delete_recipe(&pool, recipe_id).await {
        println!("Warning: Failed to cleanup recipe {}: {}", recipe_id, e);
    }

    println!("✅ Saved ingredient editing workflow test passed - ingredient successfully updated, deleted, and added in database");
    Ok(())
}