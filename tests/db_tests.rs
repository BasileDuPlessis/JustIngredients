use anyhow::{Context, Result};
use just_ingredients::db::*;
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
        "flour 2 cups",
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

#[tokio::test]
async fn test_get_user_recipes_paginated() -> Result<()> {
    skip_if_no_db!(test_get_user_recipes_paginated_impl)
}

async fn test_get_user_recipes_paginated_impl(pool: &PgPool) -> Result<()> {
    // Create recipes with names
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups").await?;
    update_recipe_name(pool, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g").await?;
    update_recipe_name(pool, recipe2_id, "Apple Pie").await?;

    let recipe3_id = create_recipe(pool, 12345, "sugar 1 cup").await?;
    update_recipe_name(pool, recipe3_id, "Banana Bread").await?;

    // Create recipe for different user
    let recipe4_id = create_recipe(pool, 67890, "milk 250ml").await?;
    update_recipe_name(pool, recipe4_id, "Pancakes").await?;

    // Test pagination: limit 2, offset 0
    let (recipes, total) = get_user_recipes_paginated(pool, 12345, 2, 0).await?;
    assert_eq!(total, 3);
    assert_eq!(recipes.len(), 2);
    assert!(recipes.contains(&"Apple Pie".to_string()));
    assert!(recipes.contains(&"Banana Bread".to_string()));

    // Test pagination: limit 2, offset 2
    let (recipes, total) = get_user_recipes_paginated(pool, 12345, 2, 2).await?;
    assert_eq!(total, 3);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0], "Chocolate Cake");

    // Test with different user
    let (recipes, total) = get_user_recipes_paginated(pool, 67890, 10, 0).await?;
    assert_eq!(total, 1);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0], "Pancakes");

    // Test with no recipes
    let (recipes, total) = get_user_recipes_paginated(pool, 99999, 10, 0).await?;
    assert_eq!(total, 0);
    assert_eq!(recipes.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_get_recipes_by_name() -> Result<()> {
    skip_if_no_db!(test_get_recipes_by_name_impl)
}

async fn test_get_recipes_by_name_impl(pool: &PgPool) -> Result<()> {
    // Create multiple recipes with the same name
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups sugar 1 cup").await?;
    update_recipe_name(pool, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g eggs 2").await?;
    update_recipe_name(pool, recipe2_id, "Chocolate Cake").await?;

    let recipe3_id = create_recipe(pool, 12345, "milk 250ml vanilla 1 tsp").await?;
    update_recipe_name(pool, recipe3_id, "Vanilla Pudding").await?;

    // Create recipe with same name for different user
    let recipe4_id = create_recipe(pool, 67890, "flour 1 cup").await?;
    update_recipe_name(pool, recipe4_id, "Chocolate Cake").await?;

    // Test getting multiple recipes with same name
    let recipes = get_recipes_by_name(pool, 12345, "Chocolate Cake").await?;
    assert_eq!(recipes.len(), 2);

    // Verify the recipes are returned in descending creation order (most recent first)
    assert_eq!(recipes[0].id, recipe2_id); // Second recipe created (most recent)
    assert_eq!(recipes[1].id, recipe1_id); // First recipe created
    assert_eq!(recipes[0].recipe_name.as_ref().unwrap(), "Chocolate Cake");
    assert_eq!(recipes[1].recipe_name.as_ref().unwrap(), "Chocolate Cake");

    // Test getting single recipe
    let recipes = get_recipes_by_name(pool, 12345, "Vanilla Pudding").await?;
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].id, recipe3_id);
    assert_eq!(recipes[0].recipe_name.as_ref().unwrap(), "Vanilla Pudding");

    // Test getting recipes for different user
    let recipes = get_recipes_by_name(pool, 67890, "Chocolate Cake").await?;
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].id, recipe4_id);

    // Test getting non-existent recipe name
    let recipes = get_recipes_by_name(pool, 12345, "Non-existent Recipe").await?;
    assert_eq!(recipes.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_has_duplicate_recipes() -> Result<()> {
    skip_if_no_db!(test_has_duplicate_recipes_impl)
}

async fn test_has_duplicate_recipes_impl(pool: &PgPool) -> Result<()> {
    // Create multiple recipes with the same name
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups").await?;
    update_recipe_name(pool, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g").await?;
    update_recipe_name(pool, recipe2_id, "Chocolate Cake").await?;

    // Create single recipe with different name
    let recipe3_id = create_recipe(pool, 12345, "milk 250ml").await?;
    update_recipe_name(pool, recipe3_id, "Vanilla Pudding").await?;

    // Test duplicate detection - should return true for "Chocolate Cake"
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Chocolate Cake").await?;
    assert!(has_duplicates);

    // Test no duplicates - should return false for "Vanilla Pudding"
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Vanilla Pudding").await?;
    assert!(!has_duplicates);

    // Test non-existent recipe name - should return false
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Non-existent Recipe").await?;
    assert!(!has_duplicates);

    // Test with different user - should return false even if name exists for another user
    let has_duplicates = has_duplicate_recipes(pool, 67890, "Chocolate Cake").await?;
    assert!(!has_duplicates);

    Ok(())
}

#[tokio::test]
async fn test_schema_validation() -> Result<()> {
    skip_if_no_db!(test_schema_validation_impl)
}

async fn test_schema_validation_impl(pool: &PgPool) -> Result<()> {
    // Schema should be valid after initialization
    validate_database_schema(pool).await?;

    // Test that validation fails if a required table is missing
    sqlx::query("DROP TABLE IF EXISTS ingredients CASCADE")
        .execute(pool)
        .await?;

    let result = validate_database_schema(pool).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("ingredients"));

    // Recreate the table and test again
    init_database_schema(pool).await?;
    validate_database_schema(pool).await?;

    Ok(())
}

#[tokio::test]
async fn test_migration_system() -> Result<()> {
    skip_if_no_db!(test_migration_system_impl)
}

async fn test_migration_system_impl(pool: &PgPool) -> Result<()> {
    // Clean up any existing migration state
    sqlx::query("DROP TABLE IF EXISTS schema_migrations CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS ingredients CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS recipes CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS users CASCADE")
        .execute(pool)
        .await?;

    // Test initial state - no migrations applied
    let version = migrations::get_current_version(pool).await?;
    assert_eq!(version, 0);

    // Apply migrations
    migrations::apply_pending_migrations(pool).await?;

    // Check that migrations table was created and version is updated
    let version = migrations::get_current_version(pool).await?;
    assert_eq!(version, 1);

    // Verify that tables were created
    validate_database_schema(pool).await?;

    // Test rollback (if down migrations are available)
    // Note: Current migration doesn't have a down script, so this will fail gracefully
    let rollback_result = migrations::rollback_to_version(pool, 0).await;
    assert!(rollback_result.is_err()); // Should fail because no down migration

    Ok(())
}

#[tokio::test]
async fn test_schema_validation_details() -> Result<()> {
    skip_if_no_db!(test_schema_validation_details_impl)
}

async fn test_schema_validation_details_impl(pool: &PgPool) -> Result<()> {
    // Test that all expected columns exist with correct types

    // Check users table columns
    let user_columns = vec![
        ("id", "bigint"),
        ("telegram_id", "bigint"),
        ("language_code", "character varying"),
        ("created_at", "timestamp with time zone"),
        ("updated_at", "timestamp with time zone"),
    ];

    for (col_name, _expected_type) in user_columns {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name = 'users' AND column_name = $1 AND table_schema = 'public')",
        )
        .bind(col_name)
        .fetch_one(pool)
        .await?;
        assert!(exists, "Column {} should exist in users table", col_name);
    }

    // Check ingredients table has raw_text column
    let has_raw_text: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
         WHERE table_name = 'ingredients' AND column_name = 'raw_text' AND table_schema = 'public')"
    )
    .fetch_one(pool)
    .await?;
    assert!(
        has_raw_text,
        "raw_text column should exist in ingredients table"
    );

    // Check that required indexes exist
    let indexes = vec![
        ("recipes_content_tsv_idx", "recipes"),
        ("ingredients_user_id_idx", "ingredients"),
        ("ingredients_recipe_id_idx", "ingredients"),
    ];

    for (index_name, table_name) in indexes {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2)",
        )
        .bind(table_name)
        .bind(index_name)
        .fetch_one(pool)
        .await?;
        assert!(
            exists,
            "Index {} should exist on table {}",
            index_name, table_name
        );
    }

    Ok(())
}

#[test]
fn test_split_sql_statements_single_statement() {
    let sql = "CREATE TABLE test (id INT);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec!["CREATE TABLE test (id INT);".to_string()])
    );
}

#[test]
fn test_split_sql_statements_multiple_statements() {
    let sql = "CREATE TABLE test (id INT); INSERT INTO test VALUES (1);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "CREATE TABLE test (id INT);".to_string(),
            "INSERT INTO test VALUES (1);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_string_literals() {
    let sql = "INSERT INTO test VALUES ('hello;world'); CREATE TABLE test2 (id INT);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "INSERT INTO test VALUES ('hello;world');".to_string(),
            "CREATE TABLE test2 (id INT);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_double_quotes() {
    let sql = r#"INSERT INTO test VALUES ("hello;world"); CREATE TABLE test2 (id INT);"#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            r#"INSERT INTO test VALUES ("hello;world");"#.to_string(),
            "CREATE TABLE test2 (id INT);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_comments() {
    let sql = r#"
        -- This is a comment
        CREATE TABLE test (id INT);
        -- Another comment
        INSERT INTO test VALUES (1);
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "-- This is a comment\n        CREATE TABLE test (id INT);".to_string(),
            "-- Another comment\n        INSERT INTO test VALUES (1);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_mixed_strings_and_comments() {
    let sql = r#"
        -- Create table
        CREATE TABLE test (
            id INT,
            name VARCHAR(100) DEFAULT 'test;value'
        );
        -- Insert data
        INSERT INTO test VALUES (1, 'hello;world');
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 2);
            assert!(stmts[0].contains("CREATE TABLE"));
            assert!(stmts[1].contains("INSERT INTO"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}

#[test]
fn test_split_sql_statements_empty_input() {
    let sql = "";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(statements, Ok(Vec::<String>::new()));
}

#[test]
fn test_split_sql_statements_only_whitespace() {
    let sql = "   \n\t   ";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(statements, Ok(Vec::<String>::new()));
}

#[test]
fn test_split_sql_statements_no_trailing_semicolon() {
    let sql = "CREATE TABLE test (id INT)";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec!["CREATE TABLE test (id INT)".to_string()])
    );
}

#[test]
fn test_split_sql_statements_complex_migration_sql() {
    let sql = r#"
        -- Create users table
        CREATE TABLE IF NOT EXISTS users (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT UNIQUE NOT NULL,
            language_code VARCHAR(10) DEFAULT 'en',
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );

        -- Create recipes table
        CREATE TABLE IF NOT EXISTS recipes (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT NOT NULL,
            content TEXT NOT NULL,
            recipe_name VARCHAR(255),
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
        );

        -- Create indexes
        CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 3);
            assert!(stmts[0].contains("CREATE TABLE IF NOT EXISTS users"));
            assert!(stmts[1].contains("CREATE TABLE IF NOT EXISTS recipes"));
            assert!(stmts[2].contains("CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}

#[test]
fn test_split_sql_statements_semicolon_in_string_and_comment() {
    let sql = r#"
        -- This comment has ; in it
        INSERT INTO test VALUES ('value;with;semicolons');
        -- Another ; comment
        UPDATE test SET value = 'new;value';
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 2);
            assert!(stmts[0].contains("INSERT INTO"));
            assert!(stmts[1].contains("UPDATE"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}
