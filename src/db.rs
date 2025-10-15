use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;
use tracing::{debug, info};

// Import cache types
use crate::cache::Cache;

// Re-export types for easier access
pub use crate::observability;

/// Represents a user in the database
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Represents a recipe in the database
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub id: i64,
    pub telegram_id: i64,
    pub content: String,
    pub recipe_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Represents an ingredient in the database
#[derive(Debug, Clone, PartialEq)]
pub struct Ingredient {
    pub id: i64,
    pub user_id: i64,
    pub recipe_id: Option<i64>,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Initialize the database schema
pub async fn init_database_schema(pool: &PgPool) -> Result<()> {
    info!("Initializing database schema");

    // Create users table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT UNIQUE NOT NULL,
            language_code VARCHAR(10) DEFAULT 'en',
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await
    .context("Failed to create users table")?;

    // Create recipes table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS recipes (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT NOT NULL,
            content TEXT NOT NULL,
            recipe_name VARCHAR(255),
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
        )",
    )
    .execute(pool)
    .await
    .context("Failed to create recipes table")?;

    // Create ingredients table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ingredients (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users(id),
            recipe_id BIGINT REFERENCES recipes(id),
            name VARCHAR(255) NOT NULL,
            quantity DECIMAL(10,3),
            unit VARCHAR(50),
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (recipe_id) REFERENCES recipes(id)
        )",
    )
    .execute(pool)
    .await
    .context("Failed to create ingredients table")?;

    // Add recipe_id column if it doesn't exist (for schema migration)
    sqlx::query(
        "ALTER TABLE ingredients ADD COLUMN IF NOT EXISTS recipe_id BIGINT REFERENCES recipes(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add recipe_id column to ingredients table")?;

    // Try to add foreign key constraint (ignore if it already exists)
    let _ = sqlx::query(
        "ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey FOREIGN KEY (recipe_id) REFERENCES recipes(id)",
    )
    .execute(pool)
    .await;

    // Create indexes for performance
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx ON recipes USING GIN (content_tsv)",
    )
    .execute(pool)
    .await
    .context("Failed to create FTS index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS ingredients_user_id_idx ON ingredients(user_id)")
        .execute(pool)
        .await
        .context("Failed to create ingredients user_id index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS ingredients_recipe_id_idx ON ingredients(recipe_id)")
        .execute(pool)
        .await
        .context("Failed to create ingredients recipe_id index")?;

    info!("Database schema initialized successfully");
    Ok(())
}

/// Create a new recipe in the database
pub async fn create_recipe(pool: &PgPool, telegram_id: i64, content: &str) -> Result<i64> {
    let span = crate::observability::db_span("create_recipe", "recipes");
    let _enter = span.enter();

    let start_time = std::time::Instant::now();
    debug!(telegram_id = %telegram_id, "Creating new recipe");

    let result =
        sqlx::query("INSERT INTO recipes (telegram_id, content) VALUES ($1, $2) RETURNING id")
            .bind(telegram_id)
            .bind(content)
            .fetch_one(pool)
            .await
            .context("Failed to insert new recipe");

    let duration = start_time.elapsed();
    observability::record_db_performance_metrics(
        "create_recipe",
        duration,
        1,
        crate::observability::QueryComplexity::Simple,
    );

    match result {
        Ok(row) => {
            let recipe_id: i64 = row.get(0);
            debug!(recipe_id = %recipe_id, duration_ms = %duration.as_millis(), telegram_id = %telegram_id, "Recipe created successfully");
            Ok(recipe_id)
        }
        Err(e) => Err(e),
    }
}

/// Read a recipe from the database by ID
pub async fn read_recipe(pool: &PgPool, recipe_id: i64) -> Result<Option<Recipe>> {
    debug!(recipe_id = %recipe_id, "Reading recipe");

    let row = sqlx::query("SELECT id, telegram_id, content, created_at FROM recipes WHERE id = $1")
        .bind(recipe_id)
        .fetch_optional(pool)
        .await
        .context("Failed to read recipe")?;

    match row {
        Some(row) => {
            let recipe = Recipe {
                id: row.get(0),
                telegram_id: row.get(1),
                content: row.get(2),
                recipe_name: None, // For backward compatibility, existing entries have no recipe name
                created_at: row.get(3),
            };
            debug!(recipe_id = %recipe_id, "Recipe found");
            Ok(Some(recipe))
        }
        None => {
            debug!(recipe_id = %recipe_id, "No recipe found");
            Ok(None)
        }
    }
}

/// Update an existing recipe in the database
pub async fn update_recipe(pool: &PgPool, recipe_id: i64, new_content: &str) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Updating recipe");

    let result = sqlx::query("UPDATE recipes SET content = $1 WHERE id = $2")
        .bind(new_content)
        .bind(recipe_id)
        .execute(pool)
        .await
        .context("Failed to update recipe")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe updated successfully");
        Ok(true)
    } else {
        info!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// Delete a recipe from the database
pub async fn delete_recipe(pool: &PgPool, recipe_id: i64) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Deleting recipe");

    let result = sqlx::query("DELETE FROM recipes WHERE id = $1")
        .bind(recipe_id)
        .execute(pool)
        .await
        .context("Failed to delete recipe")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe deleted successfully");
        Ok(true)
    } else {
        info!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// Get or create a user by Telegram ID
pub async fn get_or_create_user(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<User> {
    debug!(telegram_id = %telegram_id, "Getting or creating user");

    // Try to get existing user
    if let Some(user) = get_user_by_telegram_id(pool, telegram_id).await? {
        return Ok(user);
    }

    // Create new user
    let language_code = language_code.unwrap_or("en");
    let row = sqlx::query(
        "INSERT INTO users (telegram_id, language_code) VALUES ($1, $2) RETURNING id, telegram_id, language_code, created_at, updated_at"
    )
    .bind(telegram_id)
    .bind(language_code)
    .fetch_one(pool)
    .await
    .context("Failed to create new user")?;

    let user = User {
        id: row.get(0),
        telegram_id: row.get(1),
        language_code: row.get(2),
        created_at: row.get(3),
        updated_at: row.get(4),
    };

    debug!(user_id = %user.id, "User created successfully");
    Ok(user)
}

/// Get a user by Telegram ID
pub async fn get_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> Result<Option<User>> {
    debug!(telegram_id = %telegram_id, "Getting user by telegram_id");

    let row = sqlx::query("SELECT id, telegram_id, language_code, created_at, updated_at FROM users WHERE telegram_id = $1")
        .bind(telegram_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get user by telegram_id")?;

    match row {
        Some(row) => {
            let user = User {
                id: row.get(0),
                telegram_id: row.get(1),
                language_code: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            };
            info!("User found with ID: {}", user.id);
            Ok(Some(user))
        }
        None => {
            info!("No user found with telegram_id: {telegram_id}");
            Ok(None)
        }
    }
}

/// Get a user by internal database ID
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> {
    debug!(user_id = %user_id, "Getting user by internal ID");

    let row = sqlx::query("SELECT id, telegram_id, language_code, created_at, updated_at FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get user by internal ID")?;

    match row {
        Some(row) => {
            let user = User {
                id: row.get(0),
                telegram_id: row.get(1),
                language_code: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            };
            debug!(user_id = %user.id, "User found by internal ID");
            Ok(Some(user))
        }
        None => {
            debug!(user_id = %user_id, "No user found with internal ID");
            Ok(None)
        }
    }
}

/// Get or create a user by Telegram ID with caching
pub async fn get_or_create_user_cached(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
    cache: &std::sync::Mutex<crate::cache::CacheManager>,
) -> Result<User> {
    // Try cache first
    {
        let mut cache_manager = cache.lock().unwrap();
        if let Some(user) = cache_manager.user_cache.get(&telegram_id) {
            debug!(telegram_id = %telegram_id, "User found in cache");
            return Ok(user);
        }
    }

    // Cache miss - fetch from database
    let user = get_or_create_user(pool, telegram_id, language_code).await?;

    // Cache the result
    {
        let mut cache_manager = cache.lock().unwrap();
        cache_manager.user_cache.insert(telegram_id, user.clone(), std::time::Duration::from_secs(300)); // 5 minutes
    }

    Ok(user)
}

/// Get a user by Telegram ID with caching
pub async fn get_user_by_telegram_id_cached(
    pool: &PgPool,
    telegram_id: i64,
    cache: &std::sync::Mutex<crate::cache::CacheManager>,
) -> Result<Option<User>> {
    // Try cache first
    {
        let cache_manager = cache.lock().unwrap();
        if let Some(user) = cache_manager.user_cache.get(&telegram_id) {
            debug!(telegram_id = %telegram_id, "User found in cache");
            return Ok(Some(user));
        }
    }

    // Cache miss - fetch from database
    let user = get_user_by_telegram_id(pool, telegram_id).await?;

    // Cache the result if found
    if let Some(ref user) = user {
        let mut cache_manager = cache.lock().unwrap();
        cache_manager.user_cache.insert(telegram_id, user.clone(), std::time::Duration::from_secs(300)); // 5 minutes
    }

    Ok(user)
}

/// Get a user by internal ID with caching
pub async fn get_user_by_id_cached(
    pool: &PgPool,
    user_id: i64,
    cache: &std::sync::Mutex<crate::cache::CacheManager>,
) -> Result<Option<User>> {
    // Try cache first using the helper method
    {
        let cache_manager = cache.lock().unwrap();
        if let Some(user) = cache_manager.find_user_by_id(user_id) {
            debug!(user_id = %user_id, "User found in cache by ID");
            return Ok(Some(user));
        }
    }

    // Cache miss - fetch from database
    let user = get_user_by_id(pool, user_id).await?;

    // Cache the result if found (by telegram_id for future lookups)
    if let Some(ref user) = user {
        let mut cache_manager = cache.lock().unwrap();
        cache_manager.user_cache.insert(user.telegram_id, user.clone(), std::time::Duration::from_secs(300));
    }

    Ok(user)
}

/// Create a new ingredient in the database
pub async fn create_ingredient(
    pool: &PgPool,
    user_id: i64,
    recipe_id: Option<i64>,
    name: &str,
    quantity: Option<f64>,
    unit: Option<&str>,
    raw_text: &str,
) -> Result<i64> {
    let span = crate::observability::db_span("create_ingredient", "ingredients");
    let _enter = span.enter();

    let start_time = std::time::Instant::now();
    info!("Creating new ingredient for user_id: {user_id}");

    let result = sqlx::query(
        "INSERT INTO ingredients (user_id, recipe_id, name, quantity, unit, raw_text) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
    )
    .bind(user_id)
    .bind(recipe_id)
    .bind(name)
    .bind(quantity)
    .bind(unit)
    .bind(raw_text)
    .fetch_one(pool)
    .await
    .context("Failed to insert new ingredient");

    let duration = start_time.elapsed();
    observability::record_db_performance_metrics(
        "create_ingredient",
        duration,
        1,
        crate::observability::QueryComplexity::Simple,
    );

    match result {
        Ok(row) => {
            let ingredient_id: i64 = row.get(0);
            info!(ingredient_id = %ingredient_id, duration_ms = %duration.as_millis(), user_id = %user_id, recipe_id = ?recipe_id, name = %name, "Ingredient created successfully");
            Ok(ingredient_id)
        }
        Err(e) => Err(e),
    }
}

/// Read a single ingredient by ID
pub async fn read_ingredient(pool: &PgPool, ingredient_id: i64) -> Result<Option<Ingredient>> {
    info!("Reading ingredient with ID: {ingredient_id}");

    let row = sqlx::query(
        "SELECT id, user_id, recipe_id, name, quantity, unit, created_at, updated_at FROM ingredients WHERE id = $1"
    )
    .bind(ingredient_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch ingredient")?;

    match row {
        Some(row) => {
            let ingredient = Ingredient {
                id: row.get(0),
                user_id: row.get(1),
                recipe_id: row.get(2),
                name: row.get(3),
                quantity: row.get(4),
                unit: row.get(5),
                created_at: row.get(6),
                updated_at: row.get(7),
            };
            info!("Ingredient found: {:?}", ingredient);
            Ok(Some(ingredient))
        }
        None => {
            info!("No ingredient found with ID: {ingredient_id}");
            Ok(None)
        }
    }
}

/// Update an existing ingredient in the database
pub async fn update_ingredient(
    pool: &PgPool,
    ingredient_id: i64,
    name: Option<&str>,
    quantity: Option<f64>,
    unit: Option<&str>,
) -> Result<bool> {
    info!("Updating ingredient with ID: {ingredient_id}");

    let result = sqlx::query("UPDATE ingredients SET name = COALESCE($1, name), quantity = COALESCE($2, quantity), unit = COALESCE($3, unit), updated_at = CURRENT_TIMESTAMP WHERE id = $4")
        .bind(name)
        .bind(quantity)
        .bind(unit)
        .bind(ingredient_id)
        .execute(pool)
        .await
        .context("Failed to update ingredient")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        info!("Ingredient updated successfully with ID: {ingredient_id}");
        Ok(true)
    } else {
        info!("No ingredient found with ID: {ingredient_id}");
        Ok(false)
    }
}

/// Delete an ingredient from the database
pub async fn delete_ingredient(pool: &PgPool, ingredient_id: i64) -> Result<bool> {
    info!("Deleting ingredient with ID: {ingredient_id}");

    let result = sqlx::query("DELETE FROM ingredients WHERE id = $1")
        .bind(ingredient_id)
        .execute(pool)
        .await
        .context("Failed to delete ingredient")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        info!("Ingredient deleted successfully with ID: {ingredient_id}");
        Ok(true)
    } else {
        info!("No ingredient found with ID: {ingredient_id}");
        Ok(false)
    }
}

/// List all ingredients for a user
pub async fn list_ingredients_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Ingredient>> {
    info!("Listing ingredients for user_id: {user_id}");

    let rows = sqlx::query("SELECT id, user_id, recipe_id, name, quantity, unit, created_at, updated_at FROM ingredients WHERE user_id = $1 ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .context("Failed to list ingredients by user")?;

    let ingredients: Vec<Ingredient> = rows
        .into_iter()
        .map(|row| Ingredient {
            id: row.get(0),
            user_id: row.get(1),
            recipe_id: row.get(2),
            name: row.get(3),
            quantity: row.get(4),
            unit: row.get(5),
            created_at: row.get(6),
            updated_at: row.get(7),
        })
        .collect();

    info!(
        "Found {} ingredients for user_id: {user_id}",
        ingredients.len()
    );
    Ok(ingredients)
}

/// Update the recipe name for a recipe
pub async fn update_recipe_recipe_name(
    pool: &PgPool,
    recipe_id: i64,
    recipe_name: &str,
) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Updating recipe recipe name");

    let result = sqlx::query("UPDATE recipes SET recipe_name = $1 WHERE id = $2")
        .bind(recipe_name)
        .bind(recipe_id)
        .execute(pool)
        .await
        .context("Failed to update recipe recipe name")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe recipe name updated successfully");
        Ok(true)
    } else {
        info!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// Get recipe with recipe name
pub async fn read_recipe_with_recipe(pool: &PgPool, recipe_id: i64) -> Result<Option<Recipe>> {
    debug!(recipe_id = %recipe_id, "Reading recipe with recipe name");

    let row = sqlx::query(
        "SELECT id, telegram_id, content, recipe_name, created_at FROM recipes WHERE id = $1",
    )
    .bind(recipe_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read recipe with recipe")?;

    match row {
        Some(row) => {
            let recipe = Recipe {
                id: row.get(0),
                telegram_id: row.get(1),
                content: row.get(2),
                recipe_name: row.get(3),
                created_at: row.get(4),
            };
            debug!(recipe_id = %recipe_id, "Recipe with recipe found");
            Ok(Some(recipe))
        }
        None => {
            debug!(recipe_id = %recipe_id, "No recipe found");
            Ok(None)
        }
    }
}

/// Search recipes using full-text search
pub async fn search_recipes(pool: &PgPool, telegram_id: i64, query: &str) -> Result<Vec<Recipe>> {
    info!("Searching recipes for telegram_id: {telegram_id} with query: {query}");

    let rows = sqlx::query("SELECT id, telegram_id, content, recipe_name, created_at FROM recipes WHERE telegram_id = $1 AND content_tsv @@ plainto_tsquery('english', $2) ORDER BY created_at DESC")
        .bind(telegram_id)
        .bind(query)
        .fetch_all(pool)
        .await
        .context("Failed to search recipes")?;

    let recipes: Vec<Recipe> = rows
        .into_iter()
        .map(|row| Recipe {
            id: row.get(0),
            telegram_id: row.get(1),
            content: row.get(2),
            recipe_name: row.get(3),
            created_at: row.get(4),
        })
        .collect();

    info!(telegram_id = %telegram_id, query = %query, result_count = recipes.len(), "Recipe search completed");
    Ok(recipes)
}

/// Get paginated list of recipe names for a user
pub async fn get_user_recipes_paginated(
    pool: &PgPool,
    telegram_id: i64,
    limit: i64,
    offset: i64,
) -> Result<(Vec<String>, i64)> {
    // Validate pagination parameters to prevent DoS attacks
    if !(1..=100).contains(&limit) {
        return Err(anyhow::anyhow!(
            "Invalid pagination limit: {} (must be between 1 and 100)",
            limit
        ));
    }
    if !(0..=10000).contains(&offset) {
        return Err(anyhow::anyhow!(
            "Invalid pagination offset: {} (must be between 0 and 10000)",
            offset
        ));
    }

    debug!(telegram_id = %telegram_id, limit = %limit, offset = %offset, "Getting paginated recipes for user");

    // Get total count of distinct recipe names
    let total_row = sqlx::query(
        "SELECT COUNT(DISTINCT recipe_name) FROM recipes WHERE telegram_id = $1 AND recipe_name IS NOT NULL"
    )
    .bind(telegram_id)
    .fetch_one(pool)
    .await
    .context("Failed to get total recipe count")?;
    let total: i64 = total_row.get(0);

    // Get paginated recipe names
    let rows = sqlx::query(
        "SELECT DISTINCT recipe_name FROM recipes WHERE telegram_id = $1 AND recipe_name IS NOT NULL ORDER BY recipe_name LIMIT $2 OFFSET $3"
    )
    .bind(telegram_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get paginated recipes")?;

    let recipe_names: Vec<String> = rows.into_iter().map(|row| row.get(0)).collect();

    debug!(total = %total, count = %recipe_names.len(), "Retrieved paginated recipes");
    Ok((recipe_names, total))
}
