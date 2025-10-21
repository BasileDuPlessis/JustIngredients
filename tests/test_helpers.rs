//! # Test Helper Library
//!
//! This module provides common test setup functions to reduce code duplication
//! across integration tests and improve test reliability and consistency.

use just_ingredients::db;
use just_ingredients::text_processing::MeasurementDetector;
use sqlx::postgres::PgPool;
use std::sync::Arc;

/// Setup a test database connection pool
///
/// This function handles the common pattern of:
/// 1. Checking for DATABASE_URL environment variable
/// 2. Creating a connection pool
/// 3. Initializing the database schema
///
/// Returns None if DATABASE_URL is not set (graceful skip for integration tests)
pub async fn setup_test_database() -> Result<Option<Arc<PgPool>>, Box<dyn std::error::Error>> {
    // Check if DATABASE_URL is set
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⚠️ Skipping database test - DATABASE_URL not set");
            return Ok(None);
        }
    };

    // Create connection pool
    let pool = match sqlx::postgres::PgPool::connect(&database_url).await {
        Ok(pool) => Arc::new(pool),
        Err(e) => {
            println!("⚠️ Skipping database test - failed to connect: {}", e);
            return Ok(None);
        }
    };

    // Initialize schema
    if let Err(e) = db::init_database_schema(&pool).await {
        println!("⚠️ Skipping database test - failed to init schema: {}", e);
        return Ok(None);
    }

    Ok(Some(pool))
}

/// Create a test user in the database
///
/// Returns the created user
pub async fn create_test_user(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<db::User, Box<dyn std::error::Error>> {
    let user = db::get_or_create_user(pool, telegram_id, language_code).await?;
    Ok(user)
}

/// Create a test recipe with optional name
///
/// Returns the recipe ID
pub async fn create_test_recipe(
    pool: &PgPool,
    telegram_id: i64,
    content: &str,
    name: Option<&str>,
) -> Result<i64, Box<dyn std::error::Error>> {
    let recipe_id = db::create_recipe(pool, telegram_id, content).await?;

    if let Some(recipe_name) = name {
        db::update_recipe_name(pool, recipe_id, recipe_name).await?;
    }

    Ok(recipe_id)
}

/// Create test ingredients for a recipe from OCR text
///
/// Uses MeasurementDetector to extract ingredients and creates them in the database.
/// Returns the created ingredient IDs.
pub async fn create_test_ingredients_from_text(
    pool: &PgPool,
    user_id: i64,
    recipe_id: Option<i64>,
    ocr_text: &str,
) -> Result<Vec<i64>, Box<dyn std::error::Error>> {
    let detector = MeasurementDetector::new()?;
    let measurements = detector.extract_ingredient_measurements(ocr_text);

    let mut ingredient_ids = Vec::new();

    for measurement in measurements {
        let ingredient_id = db::create_ingredient(
            pool,
            user_id,
            recipe_id,
            &measurement.ingredient_name,
            measurement.quantity.parse().ok(),
            measurement.measurement.as_deref(),
            &format!("{} {}", measurement.quantity, measurement.ingredient_name),
        )
        .await?;

        ingredient_ids.push(ingredient_id);
    }

    Ok(ingredient_ids)
}

/// Create a complete test recipe with ingredients
///
/// This is a convenience function that creates a user, recipe, and ingredients in one call.
/// Returns (user, recipe_id, ingredient_ids)
pub async fn create_complete_test_recipe(
    pool: &PgPool,
    telegram_id: i64,
    recipe_content: &str,
    recipe_name: Option<&str>,
    ingredients_text: &str,
) -> Result<(db::User, i64, Vec<i64>), Box<dyn std::error::Error>> {
    // Create user
    let user = create_test_user(pool, telegram_id, Some("en")).await?;

    // Create recipe
    let recipe_id = create_test_recipe(pool, telegram_id, recipe_content, recipe_name).await?;

    // Create ingredients
    let ingredient_ids = create_test_ingredients_from_text(
        pool,
        user.id,
        Some(recipe_id),
        ingredients_text,
    )
    .await?;

    Ok((user, recipe_id, ingredient_ids))
}

/// Clean up test data for a specific user
///
/// Deletes all ingredients and recipes for the given telegram_id.
/// This should be called in test cleanup to avoid data pollution.
pub async fn cleanup_test_data(
    pool: &PgPool,
    telegram_id: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get all ingredients for this user
    let ingredients = db::list_ingredients_by_user(pool, telegram_id).await?;

    // Delete ingredients first (to satisfy foreign key constraints)
    for ingredient in ingredients {
        db::delete_ingredient(pool, ingredient.id).await?;
    }

    // Get all recipes for this user
    let (recipe_names, _) = db::get_user_recipes_paginated(pool, telegram_id, 1000, 0).await?;

    // Delete recipes
    for recipe_name in recipe_names {
        // We need to get the recipe by name to get its ID
        let recipes = db::get_recipes_by_name(pool, telegram_id, &recipe_name).await?;
        for recipe in recipes {
            db::delete_recipe(pool, recipe.id).await?;
        }
    }

    Ok(())
}

/// Clean up specific recipe and its ingredients
///
/// More targeted cleanup for tests that create specific recipes.
pub async fn cleanup_test_recipe(
    pool: &PgPool,
    recipe_id: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get ingredients for this recipe
    let ingredients = db::get_recipe_ingredients(pool, recipe_id).await?;

    // Delete ingredients first
    for ingredient in ingredients {
        db::delete_ingredient(pool, ingredient.id).await?;
    }

    // Delete recipe
    db::delete_recipe(pool, recipe_id).await?;

    Ok(())
}

/// Setup localization manager for tests
///
/// Returns a shared localization manager instance
pub fn setup_test_localization() -> Arc<just_ingredients::localization::LocalizationManager> {
    just_ingredients::localization::create_localization_manager()
        .expect("Failed to create localization manager for tests")
}

/// Generate a unique test identifier
///
/// Useful for creating unique test data that won't conflict between tests
pub fn generate_test_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}", prefix, timestamp)
}

/// Create a test recipe with a unique name
///
/// Convenience function that generates a unique recipe name to avoid conflicts
pub async fn create_unique_test_recipe(
    pool: &PgPool,
    telegram_id: i64,
    content: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let unique_name = generate_test_id("test_recipe");
    create_test_recipe(pool, telegram_id, content, Some(&unique_name)).await
}