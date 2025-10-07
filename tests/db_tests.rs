use anyhow::{Context, Result};
use ingredients::db::*;
use sqlx::PgPool;
use std::env;

/// Helper macro to skip tests when database is not available
macro_rules! skip_if_no_db {
    ($test_fn:expr) => {
        match setup_test_db().await {
            Ok(pool) => $test_fn(&pool).await,
            Err(_) => {
                eprintln!("Skipping test: Database not available");
                Ok(())
            }
        }
    };
}

async fn setup_test_db() -> Result<PgPool> {
    // Skip tests if no DATABASE_URL is provided
    let database_url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping database tests: DATABASE_URL not set");
            return Err(anyhow::anyhow!("Test database not configured"));
        }
    };

    let pool = PgPool::connect(&database_url)
        .await
        .context("Failed to connect to test database")?;

    // Clean up any existing test data
    sqlx::query("DROP TABLE IF EXISTS ingredients CASCADE")
        .execute(&pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS recipes CASCADE")
        .execute(&pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS users CASCADE")
        .execute(&pool)
        .await?;

    // Initialize schema
    init_database_schema(&pool).await?;

    Ok(pool)
}

#[tokio::test]
async fn test_user_operations() -> Result<()> {
    skip_if_no_db!(test_user_operations_impl)
}

async fn test_user_operations_impl(pool: &PgPool) -> Result<()> {
    let user = get_or_create_user(pool, 12345, Some("fr")).await?;
    assert_eq!(user.telegram_id, 12345);
    assert_eq!(user.language_code, "fr");

    // Test getting existing user
    let user2 = get_or_create_user(pool, 12345, Some("en")).await?;
    assert_eq!(user2.id, user.id); // Should return same user
    assert_eq!(user2.language_code, "fr"); // Should keep original language

    // Test get_user_by_telegram_id
    let found_user = get_user_by_telegram_id(pool, 12345).await?;
    assert_eq!(found_user, Some(user.clone()));

    // Test get_user_by_id
    let found_user_by_id = get_user_by_id(pool, user.id).await?;
    assert_eq!(found_user_by_id, Some(user));

    Ok(())
}

#[tokio::test]
async fn test_recipe_operations() -> Result<()> {
    skip_if_no_db!(test_recipe_operations_impl)
}

async fn test_recipe_operations_impl(pool: &PgPool) -> Result<()> {
    let recipe_id = create_recipe(pool, 12345, "Test OCR content").await?;
    assert!(recipe_id > 0);

    // Read recipe
    let recipe = read_recipe(pool, recipe_id).await?;
    assert!(recipe.is_some());
    let recipe = recipe.unwrap();
    assert_eq!(recipe.telegram_id, 12345);
    assert_eq!(recipe.content, "Test OCR content");

    // Update recipe
    let updated = update_recipe(pool, recipe_id, "Updated content").await?;
    assert!(updated);

    let updated_recipe = read_recipe(pool, recipe_id).await?;
    assert_eq!(updated_recipe.unwrap().content, "Updated content");

    // Delete recipe
    let deleted = delete_recipe(pool, recipe_id).await?;
    assert!(deleted);

    let not_found = read_recipe(pool, recipe_id).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[tokio::test]
async fn test_ingredient_operations() -> Result<()> {
    skip_if_no_db!(test_ingredient_operations_impl)
}

async fn test_ingredient_operations_impl(pool: &PgPool) -> Result<()> {
    let user = get_or_create_user(pool, 12345, None).await?;

    // Create recipe
    let recipe_id = create_recipe(pool, 12345, "flour 2 cups").await?;

    // Create ingredient
    let ingredient_id = create_ingredient(
        pool,
        user.id,
        Some(recipe_id),
        "flour",
        Some(2.0),
        Some("cups"),
    )
    .await?;
    assert!(ingredient_id > 0);

    // Read ingredient
    let ingredient = read_ingredient(pool, ingredient_id).await?;
    assert!(ingredient.is_some());
    let ingredient = ingredient.unwrap();
    assert_eq!(ingredient.user_id, user.id);
    assert_eq!(ingredient.recipe_id, Some(recipe_id));
    assert_eq!(ingredient.name, "flour");
    assert_eq!(ingredient.quantity, Some(2.0));
    assert_eq!(ingredient.unit, Some("cups".to_string()));

    // Update ingredient
    let updated = update_ingredient(
        pool,
        ingredient_id,
        Some("bread flour"),
        Some(3.0),
        Some("cups"),
    )
    .await?;
    assert!(updated);

    let updated_ingredient = read_ingredient(pool, ingredient_id).await?;
    assert_eq!(updated_ingredient.unwrap().name, "bread flour");

    // List ingredients by user
    let ingredients = list_ingredients_by_user(pool, user.id).await?;
    assert_eq!(ingredients.len(), 1);
    assert_eq!(ingredients[0].name, "bread flour");

    // Delete ingredient
    let deleted = delete_ingredient(pool, ingredient_id).await?;
    assert!(deleted);

    let not_found = read_ingredient(pool, ingredient_id).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[tokio::test]
async fn test_full_text_search() -> Result<()> {
    skip_if_no_db!(test_full_text_search_impl)
}

async fn test_full_text_search_impl(pool: &PgPool) -> Result<()> {
    create_recipe(pool, 12345, "flour 2 cups sugar 1 cup").await?;
    create_recipe(pool, 12345, "butter 100 grams milk 250 ml").await?;
    create_recipe(pool, 67890, "chocolate 200 grams").await?;

    // Search for entries containing "flour"
    let results = search_recipes(pool, 12345, "flour").await?;
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("flour"));

    // Search for entries containing "grams"
    let results = search_recipes(pool, 12345, "grams").await?;
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("butter"));

    // Search for non-existent term
    let results = search_recipes(pool, 12345, "nonexistent").await?;
    assert_eq!(results.len(), 0);

    Ok(())
}
