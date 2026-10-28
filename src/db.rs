use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;
use tracing::Instrument;
use tracing::{debug, error, info};

// Import cache types
use crate::cache::Cache;

// Re-export types for easier access
use crate::errors::error_logging;
pub use crate::observability;

/// Represents a user in the database
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: std::sync::Arc<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Represents a recipe in the database
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub id: i64,
    pub telegram_id: i64,
    pub content: std::sync::Arc<String>,
    pub recipe_name: Option<std::sync::Arc<String>>,
    pub created_at: DateTime<Utc>,
}

/// Represents an ingredient in the database
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// Initialize the database schema using the migration system
pub async fn init_database_schema(pool: &PgPool) -> Result<()> {
    info!("Initializing database schema using migrations");

    // Apply any pending migrations
    migrations::apply_pending_migrations(pool).await?;

    info!("Database schema initialized successfully");
    Ok(())
}

/// Create a new recipe in the database
pub async fn create_recipe(pool: &PgPool, telegram_id: i64, content: &str) -> Result<i64> {
    let span = crate::observability::db_span("create_recipe", "recipes");
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {

    let start_time = std::time::Instant::now();
    debug!(telegram_id = %telegram_id, "Creating new recipe");

    let result = sqlx::query!(
        "INSERT INTO recipes (telegram_id, content) VALUES ($1, $2) RETURNING id",
        telegram_id,
        content
    )
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
            let recipe_id: i64 = row.id;
            debug!(recipe_id = %recipe_id, duration_ms = %duration.as_millis(), telegram_id = %telegram_id, "Recipe created successfully");
            Ok(recipe_id)
        }
        Err(e) => Err(e),
    }
    }
    .instrument(span)
    .await
}

/// Read a recipe owned by `telegram_id` from the database by ID
///
/// A recipe owned by another user is reported as missing (`Ok(None)`).
pub async fn read_recipe(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Option<Recipe>> {
    debug!(recipe_id = %recipe_id, "Reading recipe");

    let row = sqlx::query(
        "SELECT id, telegram_id, content, created_at FROM recipes WHERE id = $1 AND telegram_id = $2",
    )
    .bind(recipe_id)
    .bind(telegram_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read recipe")?;

    match row {
        Some(row) => {
            let recipe = Recipe {
                id: row.get(0),
                telegram_id: row.get(1),
                content: std::sync::Arc::new(row.get(2)),
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

/// Update an existing recipe owned by `telegram_id` in the database
///
/// Returns `false` when the recipe does not exist or is owned by another user.
pub async fn update_recipe(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    new_content: &str,
) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Updating recipe");

    let result = sqlx::query("UPDATE recipes SET content = $1 WHERE id = $2 AND telegram_id = $3")
        .bind(new_content)
        .bind(recipe_id)
        .bind(telegram_id)
        .execute(pool)
        .await
        .context("Failed to update recipe")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe updated successfully");
        Ok(true)
    } else {
        debug!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// Delete a recipe owned by `telegram_id` from the database
///
/// Returns `false` when the recipe does not exist or is owned by another user;
/// in that case nothing is deleted.
pub async fn delete_recipe(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Deleting recipe");

    // Ingredients first (only if the caller owns the recipe). Migration 2 makes the foreign key
    // cascade; deleting them explicitly also works on a database where it did not apply.
    // One transaction: never a recipe left without its ingredients, or the reverse
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start the delete transaction")?;
    let ingredients_deleted = sqlx::query(
        "DELETE FROM ingredients WHERE recipe_id = (SELECT id FROM recipes WHERE id = $1 AND telegram_id = $2)",
    )
    .bind(recipe_id)
    .bind(telegram_id)
    .execute(&mut *tx)
    .await
    .context("Failed to delete ingredients for recipe")?;

    debug!(recipe_id = %recipe_id, ingredients_deleted = %ingredients_deleted.rows_affected(), "Deleted associated ingredients");

    // Now delete the recipe itself
    let result = sqlx::query("DELETE FROM recipes WHERE id = $1 AND telegram_id = $2")
        .bind(recipe_id)
        .bind(telegram_id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete recipe")?;
    tx.commit()
        .await
        .context("Failed to commit the recipe deletion")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe deleted successfully");
        Ok(true)
    } else {
        debug!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// An ingredient of a recipe being saved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewIngredient<'a> {
    pub name: &'a str,
    /// `None` for an ingredient without quantity ("sel").
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// Ids of a recipe saved by [`save_recipe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedRecipe {
    pub recipe_id: i64,
    /// Internal `users.id` of the owner.
    pub user_id: i64,
}

/// Saves a new recipe of `telegram_id` with its name and ingredients in one transaction: either
/// everything is written or nothing is, so a failing ingredient leaves no partial recipe. The
/// owner's `users` row is created on first use with `ON CONFLICT`, which is safe when two first
/// requests arrive together. `content` is the extracted text, stored once on the recipe
/// (`ingredients.raw_text` stays NULL).
pub async fn save_recipe(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
    content: &str,
    name: &str,
    ingredients: &[NewIngredient<'_>],
) -> Result<SavedRecipe> {
    let start_time = std::time::Instant::now();
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start the recipe transaction")?;

    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (telegram_id, language_code) VALUES ($1, $2) \
         ON CONFLICT (telegram_id) DO UPDATE SET telegram_id = EXCLUDED.telegram_id \
         RETURNING id",
    )
    .bind(telegram_id)
    .bind(language_code.unwrap_or("en"))
    .fetch_one(&mut *tx)
    .await
    .context("Failed to get or create the recipe owner")?;

    let recipe_id: i64 = sqlx::query_scalar(
        "INSERT INTO recipes (telegram_id, content, recipe_name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(telegram_id)
    .bind(content)
    .bind(name)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to insert the recipe")?;

    for (position, ingredient) in ingredients.iter().enumerate() {
        sqlx::query(
            "INSERT INTO ingredients (user_id, recipe_id, name, quantity, unit) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(recipe_id)
        .bind(ingredient.name)
        .bind(ingredient.quantity)
        .bind(ingredient.unit)
        .execute(&mut *tx)
        .await
        // The position, not the name: errors are logged and must not carry recipe content
        .with_context(|| format!("Failed to insert ingredient {}", position + 1))?;
    }

    tx.commit().await.context("Failed to commit the recipe")?;
    observability::record_db_performance_metrics(
        "save_recipe",
        start_time.elapsed(),
        2 + ingredients.len() as u64,
        crate::observability::QueryComplexity::Medium,
    );
    Ok(SavedRecipe { recipe_id, user_id })
}

/// New values of a saved ingredient, written as they are (`None` clears the quantity or unit).
#[derive(Debug, Clone, PartialEq)]
pub struct IngredientUpdate<'a> {
    pub id: i64,
    pub ingredient: NewIngredient<'a>,
}

/// Applies the edit of a saved recipe of `telegram_id` in one transaction: every update, addition
/// and deletion, or none of them. Unlike [`update_ingredient`], an update writes the given values,
/// so a quantity or unit can be removed. Fails without writing when the recipe is not the user's.
pub async fn apply_ingredient_changes(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    language_code: Option<&str>,
    updates: &[IngredientUpdate<'_>],
    additions: &[NewIngredient<'_>],
    deletions: &[i64],
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start the ingredient transaction")?;

    let owned: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM recipes WHERE id = $1 AND telegram_id = $2")
            .bind(recipe_id)
            .bind(telegram_id)
            .fetch_optional(&mut *tx)
            .await
            .context("Failed to check the recipe owner")?;
    if owned.is_none() {
        return Err(anyhow::anyhow!(
            "Recipe {recipe_id} not found for this user"
        ));
    }

    for update in updates {
        sqlx::query(
            "UPDATE ingredients SET name = $1, quantity = $2, unit = $3, \
             updated_at = CURRENT_TIMESTAMP WHERE id = $4 AND recipe_id = $5",
        )
        .bind(update.ingredient.name)
        .bind(update.ingredient.quantity)
        .bind(update.ingredient.unit)
        .bind(update.id)
        .bind(recipe_id)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("Failed to update the ingredient {}", update.id))?;
    }

    if !additions.is_empty() {
        let user_id: i64 = sqlx::query_scalar(
            "INSERT INTO users (telegram_id, language_code) VALUES ($1, $2) \
             ON CONFLICT (telegram_id) DO UPDATE SET telegram_id = EXCLUDED.telegram_id \
             RETURNING id",
        )
        .bind(telegram_id)
        .bind(language_code.unwrap_or("en"))
        .fetch_one(&mut *tx)
        .await
        .context("Failed to get or create the recipe owner")?;
        for (position, ingredient) in additions.iter().enumerate() {
            sqlx::query(
                "INSERT INTO ingredients (user_id, recipe_id, name, quantity, unit) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(user_id)
            .bind(recipe_id)
            .bind(ingredient.name)
            .bind(ingredient.quantity)
            .bind(ingredient.unit)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to add ingredient {}", position + 1))?;
        }
    }

    for id in deletions {
        sqlx::query("DELETE FROM ingredients WHERE id = $1 AND recipe_id = $2")
            .bind(id)
            .bind(recipe_id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to delete the ingredient {id}"))?;
    }

    tx.commit()
        .await
        .context("Failed to commit the ingredient changes")?;
    observability::record_db_performance_metrics(
        "apply_ingredient_changes",
        start_time.elapsed(),
        (updates.len() + additions.len() + deletions.len()) as u64,
        crate::observability::QueryComplexity::Medium,
    );
    Ok(())
}

/// Get or create a user by Telegram ID
pub async fn get_or_create_user(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<User> {
    debug!(telegram_id = %telegram_id, "Starting get_or_create_user");

    // Try to get existing user
    debug!(telegram_id = %telegram_id, "Attempting to find existing user");
    match get_user_by_telegram_id(pool, telegram_id).await {
        Ok(Some(user)) => {
            debug!(telegram_id = %telegram_id, user_id = %user.id, "Found existing user, returning");
            return Ok(user);
        }
        Ok(None) => {
            debug!(telegram_id = %telegram_id, "No existing user found, will create new user");
        }
        Err(e) => {
            error!(telegram_id = %telegram_id, error = %e, "Error looking up existing user");
            return Err(e);
        }
    }

    // Create new user
    let language_code = language_code.unwrap_or("en");
    debug!(telegram_id = %telegram_id, language_code = %language_code, "Attempting to create new user");

    let insert_result = sqlx::query(
        "INSERT INTO users (telegram_id, language_code) VALUES ($1, $2) RETURNING id, telegram_id, language_code, created_at, updated_at"
    )
    .bind(telegram_id)
    .bind(language_code)
    .fetch_one(pool)
    .await;

    match insert_result {
        Ok(row) => {
            let user = User {
                id: row.get(0),
                telegram_id: row.get(1),
                language_code: std::sync::Arc::new(row.get(2)),
                created_at: row.get(3),
                updated_at: row.get(4),
            };

            debug!(telegram_id = %telegram_id, user_id = %user.id, returned_telegram_id = %user.telegram_id, "User insert succeeded, verifying data");

            // Verify that the created user has the correct telegram_id
            if user.telegram_id != telegram_id {
                error!(
                    expected_telegram_id = %telegram_id,
                    actual_telegram_id = %user.telegram_id,
                    user_id = %user.id,
                    "CRITICAL: User creation returned wrong telegram_id!"
                );
                return Err(anyhow::anyhow!(
                    "User creation returned wrong telegram_id: expected {}, got {}",
                    telegram_id,
                    user.telegram_id
                ));
            }

            debug!(user_id = %user.id, telegram_id = %user.telegram_id, "User created and verified successfully");
            Ok(user)
        }
        Err(e) => {
            error!(telegram_id = %telegram_id, language_code = %language_code, error = %e, "User insert failed");
            Err(e.into())
        }
    }
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
                language_code: std::sync::Arc::new(row.get(2)),
                created_at: row.get(3),
                updated_at: row.get(4),
            };
            debug!("User found with ID: {}", user.id);
            Ok(Some(user))
        }
        None => {
            debug!("No user found with telegram_id: {telegram_id}");
            Ok(None)
        }
    }
}

/// Get a user by internal database ID
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> {
    debug!(user_id = %user_id, "Getting user by internal ID");

    let row = sqlx::query(
        "SELECT id, telegram_id, language_code, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("Failed to get user by internal ID")?;

    match row {
        Some(row) => {
            let user = User {
                id: row.get(0),
                telegram_id: row.get(1),
                language_code: std::sync::Arc::new(row.get(2)),
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
        let cache_manager = match cache.lock() {
            Ok(manager) => manager,
            Err(poisoned) => {
                crate::observability::record_mutex_poisoning("cache_manager", "user_lookup");
                // For poisoned mutex, we can still access the data but log the incident
                poisoned.into_inner()
            }
        };
        if let Some(user) = cache_manager.user_cache.get(&telegram_id) {
            debug!(telegram_id = %telegram_id, "User found in cache");
            return Ok(user);
        }
    }

    // Cache miss - fetch from database
    let user = get_or_create_user(pool, telegram_id, language_code).await?;

    // Cache the result
    {
        let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");
        cache_manager.user_cache.insert(
            telegram_id,
            user.clone(),
            std::time::Duration::from_secs(300),
        ); // 5 minutes
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
        let cache_manager = cache.lock().expect("Failed to acquire cache manager lock");
        if let Some(user) = cache_manager.user_cache.get(&telegram_id) {
            debug!(telegram_id = %telegram_id, "User found in cache");
            return Ok(Some(user));
        }
    }

    // Cache miss - fetch from database
    let user = get_user_by_telegram_id(pool, telegram_id).await?;

    // Cache the result if found
    if let Some(ref user) = user {
        let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");
        cache_manager.user_cache.insert(
            telegram_id,
            user.clone(),
            std::time::Duration::from_secs(300),
        ); // 5 minutes
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
        let cache_manager = cache.lock().expect("Failed to acquire cache manager lock");
        if let Some(user) = cache_manager.find_user_by_id(user_id) {
            debug!(user_id = %user_id, "User found in cache by ID");
            return Ok(Some(user));
        }
    }

    // Cache miss - fetch from database
    let user = get_user_by_id(pool, user_id).await?;

    // Cache the result if found (by telegram_id for future lookups)
    if let Some(ref user) = user {
        let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");
        cache_manager.user_cache.insert(
            user.telegram_id,
            user.clone(),
            std::time::Duration::from_secs(300),
        );
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
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {

    let start_time = std::time::Instant::now();
    debug!("Creating new ingredient for user_id: {user_id}");

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
            debug!(ingredient_id = %ingredient_id, duration_ms = %duration.as_millis(), user_id = %user_id, recipe_id = ?recipe_id, "Ingredient created successfully");
            Ok(ingredient_id)
        }
        Err(e) => {
            error_logging::log_database_error(
                &e,
                "create_ingredient",
                Some(user_id),
                Some(&[
                    ("table", &"ingredients"),
                    (
                        "recipe_id",
                        &recipe_id.map_or("None".to_string(), |id| id.to_string()),
                    ),
                ]),
            );
            Err(e)
        }
    }
    }
    .instrument(span)
    .await
}

/// Read a single ingredient by ID, if it belongs to a recipe owned by `telegram_id`
///
/// An ingredient of another user's recipe is reported as missing (`Ok(None)`).
pub async fn read_ingredient(
    pool: &PgPool,
    telegram_id: i64,
    ingredient_id: i64,
) -> Result<Option<Ingredient>> {
    debug!("Reading ingredient with ID: {ingredient_id}");

    let row = sqlx::query(
        "SELECT i.id, i.user_id, i.recipe_id, i.name, i.quantity::float8, i.unit, i.created_at, i.updated_at FROM ingredients i JOIN recipes r ON r.id = i.recipe_id WHERE i.id = $1 AND r.telegram_id = $2"
    )
    .bind(ingredient_id)
    .bind(telegram_id)
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
            debug!(ingredient_id = %ingredient_id, "Ingredient found");
            Ok(Some(ingredient))
        }
        None => {
            debug!("No ingredient found with ID: {ingredient_id}");
            Ok(None)
        }
    }
}

/// Update an existing ingredient of a recipe owned by `telegram_id`
///
/// Returns `false` when the ingredient does not exist or belongs to another user's recipe.
pub async fn update_ingredient(
    pool: &PgPool,
    telegram_id: i64,
    ingredient_id: i64,
    name: Option<&str>,
    quantity: Option<f64>,
    unit: Option<&str>,
) -> Result<bool> {
    debug!("Updating ingredient with ID: {ingredient_id}");

    let result = sqlx::query("UPDATE ingredients SET name = COALESCE($1, name), quantity = COALESCE($2, quantity), unit = COALESCE($3, unit), updated_at = CURRENT_TIMESTAMP WHERE id = $4 AND EXISTS (SELECT 1 FROM recipes r WHERE r.id = ingredients.recipe_id AND r.telegram_id = $5)")
        .bind(name)
        .bind(quantity)
        .bind(unit)
        .bind(ingredient_id)
        .bind(telegram_id)
        .execute(pool)
        .await
        .context("Failed to update ingredient")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!("Ingredient updated successfully with ID: {ingredient_id}");
        Ok(true)
    } else {
        debug!("No ingredient found with ID: {ingredient_id}");
        Ok(false)
    }
}

/// Delete an ingredient of a recipe owned by `telegram_id`
///
/// Returns `false` when the ingredient does not exist or belongs to another user's recipe.
pub async fn delete_ingredient(
    pool: &PgPool,
    telegram_id: i64,
    ingredient_id: i64,
) -> Result<bool> {
    debug!("Deleting ingredient with ID: {ingredient_id}");

    let result = sqlx::query("DELETE FROM ingredients WHERE id = $1 AND EXISTS (SELECT 1 FROM recipes r WHERE r.id = ingredients.recipe_id AND r.telegram_id = $2)")
        .bind(ingredient_id)
        .bind(telegram_id)
        .execute(pool)
        .await
        .context("Failed to delete ingredient")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!("Ingredient deleted successfully with ID: {ingredient_id}");
        Ok(true)
    } else {
        debug!("No ingredient found with ID: {ingredient_id}");
        Ok(false)
    }
}

/// List all ingredients for a user
pub async fn list_ingredients_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Ingredient>> {
    debug!("Listing ingredients for user_id: {user_id}");

    let rows = sqlx::query("SELECT id, user_id, recipe_id, name, quantity::float8, unit, created_at, updated_at FROM ingredients WHERE user_id = $1 ORDER BY created_at DESC, id DESC")
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

    debug!(
        "Found {} ingredients for user_id: {user_id}",
        ingredients.len()
    );
    Ok(ingredients)
}

/// Get all ingredients for a specific recipe owned by `telegram_id`
///
/// Returns an empty list when the recipe does not exist or is owned by another user.
pub async fn get_recipe_ingredients(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Vec<Ingredient>> {
    debug!("Getting ingredients for recipe_id: {recipe_id}");

    let rows = sqlx::query("SELECT i.id, i.user_id, i.recipe_id, i.name, i.quantity::float8, i.unit, i.created_at, i.updated_at FROM ingredients i JOIN recipes r ON r.id = i.recipe_id WHERE i.recipe_id = $1 AND r.telegram_id = $2 ORDER BY i.created_at ASC, i.id ASC")
        .bind(recipe_id)
        .bind(telegram_id)
        .fetch_all(pool)
        .await
        .context("Failed to get recipe ingredients")?;

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

    debug!(
        "Found {} ingredients for recipe_id: {recipe_id}",
        ingredients.len()
    );
    Ok(ingredients)
}

/// Update the recipe name for a recipe owned by `telegram_id`
///
/// Returns `false` when the recipe does not exist or is owned by another user.
pub async fn update_recipe_name(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    recipe_name: &str,
) -> Result<bool> {
    debug!(recipe_id = %recipe_id, "Updating recipe recipe name");

    let result =
        sqlx::query("UPDATE recipes SET recipe_name = $1 WHERE id = $2 AND telegram_id = $3")
            .bind(recipe_name)
            .bind(recipe_id)
            .bind(telegram_id)
            .execute(pool)
            .await
            .context("Failed to update recipe recipe name")?;

    let rows_affected = result.rows_affected();
    if rows_affected > 0 {
        debug!(recipe_id = %recipe_id, "Recipe recipe name updated successfully");
        Ok(true)
    } else {
        debug!("No recipe found with ID: {recipe_id}");
        Ok(false)
    }
}

/// Get recipe with recipe name, if it is owned by `telegram_id`
///
/// A recipe owned by another user is reported as missing (`Ok(None)`).
pub async fn read_recipe_with_name(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Option<Recipe>> {
    debug!(recipe_id = %recipe_id, "Reading recipe with recipe name");

    let row = sqlx::query(
        "SELECT id, telegram_id, content, recipe_name, created_at FROM recipes WHERE id = $1 AND telegram_id = $2",
    )
    .bind(recipe_id)
    .bind(telegram_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read recipe with recipe")?;

    match row {
        Some(row) => {
            let recipe = Recipe {
                id: row.get(0),
                telegram_id: row.get(1),
                content: std::sync::Arc::new(row.get(2)),
                recipe_name: row.get::<Option<String>, _>(3).map(std::sync::Arc::new),
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
    debug!(telegram_id = %telegram_id, "Searching recipes");

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
            content: std::sync::Arc::new(row.get(2)),
            recipe_name: row.get::<Option<String>, _>(3).map(std::sync::Arc::new),
            created_at: row.get(4),
        })
        .collect();

    debug!(telegram_id = %telegram_id, result_count = recipes.len(), "Recipe search completed");
    Ok(recipes)
}

/// Check if a recipe name has duplicates for a user
pub async fn has_duplicate_recipes(
    pool: &PgPool,
    telegram_id: i64,
    recipe_name: &str,
) -> Result<bool> {
    let span = crate::observability::db_span("has_duplicate_recipes", "recipes");
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {

    debug!(telegram_id = %telegram_id, recipe_name = %recipe_name, "Checking for duplicate recipes");

    let row =
        sqlx::query("SELECT COUNT(*) FROM recipes WHERE telegram_id = $1 AND recipe_name = $2")
            .bind(telegram_id)
            .bind(recipe_name)
            .fetch_one(pool)
            .await
            .context("Failed to check for duplicate recipes")?;

    let count: i64 = row.get(0);
    let has_duplicates = count > 1;

    debug!(telegram_id = %telegram_id, recipe_name = %recipe_name, count = %count, has_duplicates = %has_duplicates, "Duplicate check completed");
    Ok(has_duplicates)
    }
    .instrument(span)
    .await
}

/// A saved recipe as shown in lists: id, name and creation date.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeSummary {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Recipes of a user, newest first, and the user's total number of recipes.
///
/// Recipes without a name are left out, as in the recipe list.
pub async fn get_user_recipes_newest_first(
    pool: &PgPool,
    telegram_id: i64,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RecipeSummary>, i64)> {
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

    let total: i64 = sqlx::query(
        "SELECT COUNT(*) FROM recipes WHERE telegram_id = $1 AND recipe_name IS NOT NULL",
    )
    .bind(telegram_id)
    .fetch_one(pool)
    .await
    .context("Failed to count recipes")?
    .get(0);

    let rows = sqlx::query(
        "SELECT id, recipe_name, created_at FROM recipes \
         WHERE telegram_id = $1 AND recipe_name IS NOT NULL \
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(telegram_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get the latest recipes")?;

    let recipes = rows
        .into_iter()
        .map(|row| RecipeSummary {
            id: row.get(0),
            name: row.get(1),
            created_at: row.get(2),
        })
        .collect();
    Ok((recipes, total))
}

/// Validate that the database schema matches what the application expects
pub async fn validate_database_schema(pool: &PgPool) -> Result<()> {
    info!("Validating database schema");

    // Check that all required tables exist
    let required_tables = vec!["users", "recipes", "ingredients", "chat_messages"];
    for table_name in required_tables {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public')"
        )
        .bind(table_name)
        .fetch_one(pool)
        .await
        .context(format!("Failed to check if table '{}' exists", table_name))?;

        if !exists {
            return Err(anyhow::anyhow!(
                "Required table '{}' does not exist",
                table_name
            ));
        }
        debug!("✓ Table '{}' exists", table_name);
    }

    // Validate users table schema
    validate_table_columns(
        pool,
        "users",
        &[
            ("id", "bigint"),
            ("telegram_id", "bigint"),
            ("language_code", "character varying"),
            ("created_at", "timestamp with time zone"),
            ("updated_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Validate recipes table schema
    validate_table_columns(
        pool,
        "recipes",
        &[
            ("id", "bigint"),
            ("telegram_id", "bigint"),
            ("content", "text"),
            ("recipe_name", "character varying"),
            ("created_at", "timestamp with time zone"),
            ("content_tsv", "tsvector"),
        ],
    )
    .await?;

    // Validate ingredients table schema
    validate_table_columns(
        pool,
        "ingredients",
        &[
            ("id", "bigint"),
            ("user_id", "bigint"),
            ("recipe_id", "bigint"),
            ("name", "character varying"),
            ("quantity", "numeric"),
            ("unit", "character varying"),
            ("raw_text", "text"),
            ("created_at", "timestamp with time zone"),
            ("updated_at", "timestamp with time zone"),
        ],
    )
    .await?;

    // Validate indexes exist
    validate_indexes(pool, "recipes", &["recipes_content_tsv_idx"]).await?;
    validate_indexes(
        pool,
        "ingredients",
        &["ingredients_user_id_idx", "ingredients_recipe_id_idx"],
    )
    .await?;

    info!("✓ Database schema validation completed successfully");
    Ok(())
}

/// Helper function to validate that a table has the required columns with correct types
async fn validate_table_columns(
    pool: &PgPool,
    table_name: &str,
    expected_columns: &[(&str, &str)],
) -> Result<()> {
    for (column_name, expected_type) in expected_columns {
        let column_info: Option<(String, String)> = sqlx::query_as(
            "SELECT data_type, is_nullable FROM information_schema.columns
             WHERE table_name = $1 AND column_name = $2 AND table_schema = 'public'",
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(pool)
        .await
        .context(format!(
            "Failed to check column '{}' in table '{}'",
            column_name, table_name
        ))?;

        match column_info {
            Some((data_type, _is_nullable)) => {
                // Check if the data type matches (allowing for some flexibility)
                if !data_type
                    .to_lowercase()
                    .contains(&expected_type.to_lowercase())
                {
                    return Err(anyhow::anyhow!(
                        "Column '{}' in table '{}' has type '{}', expected '{}'",
                        column_name,
                        table_name,
                        data_type,
                        expected_type
                    ));
                }
                debug!(
                    "✓ Column '{}' in '{}' has correct type '{}'",
                    column_name, table_name, data_type
                );
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Required column '{}' does not exist in table '{}'",
                    column_name,
                    table_name
                ));
            }
        }
    }
    Ok(())
}

/// Helper function to validate that required indexes exist
async fn validate_indexes(
    pool: &PgPool,
    table_name: &str,
    expected_indexes: &[&str],
) -> Result<()> {
    for index_name in expected_indexes {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2)",
        )
        .bind(table_name)
        .bind(index_name)
        .fetch_one(pool)
        .await
        .context(format!(
            "Failed to check if index '{}' exists on table '{}'",
            index_name, table_name
        ))?;

        if !exists {
            return Err(anyhow::anyhow!(
                "Required index '{}' does not exist on table '{}'",
                index_name,
                table_name
            ));
        }
        debug!("✓ Index '{}' exists on table '{}'", index_name, table_name);
    }
    Ok(())
}

/// A message recorded in `chat_messages`, taken back to be deleted or edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatMessage {
    pub message_id: i32,
    /// Sent less than 48 hours ago: Telegram still lets the bot delete it.
    pub deletable: bool,
}

/// Records a message of `chat` with its `kind` (and recipe, for a card). Recording it again
/// replaces its kind and recipe.
pub async fn record_chat_message(
    pool: &PgPool,
    chat_id: i64,
    message_id: i32,
    kind: &str,
    recipe_id: Option<i64>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO chat_messages (chat_id, message_id, kind, recipe_id) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (chat_id, message_id) DO UPDATE SET kind = EXCLUDED.kind, recipe_id = EXCLUDED.recipe_id",
    )
    .bind(chat_id)
    .bind(i64::from(message_id))
    .bind(kind)
    .bind(recipe_id)
    .execute(pool)
    .await
    .context("Failed to record a chat message")?;
    Ok(())
}

/// Forgets a recorded message: it stays in the chat.
pub async fn forget_chat_message(pool: &PgPool, chat_id: i64, message_id: i32) -> Result<()> {
    sqlx::query("DELETE FROM chat_messages WHERE chat_id = $1 AND message_id = $2")
        .bind(chat_id)
        .bind(i64::from(message_id))
        .execute(pool)
        .await
        .context("Failed to forget a chat message")?;
    Ok(())
}

/// Takes the recorded messages of `chat` of the given kinds: they are no longer recorded, and
/// come back oldest first, each telling whether Telegram still allows deleting it.
pub async fn take_chat_messages(
    pool: &PgPool,
    chat_id: i64,
    kinds: &[&str],
) -> Result<Vec<ChatMessage>> {
    let kinds: Vec<String> = kinds.iter().map(|kind| (*kind).to_string()).collect();
    let rows = sqlx::query(
        "DELETE FROM chat_messages WHERE chat_id = $1 AND kind = ANY($2) \
         RETURNING message_id, sent_at > CURRENT_TIMESTAMP - INTERVAL '48 hours' AS deletable",
    )
    .bind(chat_id)
    .bind(&kinds)
    .fetch_all(pool)
    .await
    .context("Failed to take chat messages")?;
    let mut messages: Vec<ChatMessage> = rows
        .iter()
        .map(|row| ChatMessage {
            message_id: i32::try_from(row.get::<i64, _>("message_id")).unwrap_or(0),
            deletable: row.get("deletable"),
        })
        .filter(|message| message.message_id > 0)
        .collect();
    messages.sort_by_key(|message| message.message_id);
    Ok(messages)
}

/// Database migration system
pub mod migrations {
    use super::*;

    /// Represents a database migration
    pub struct Migration {
        pub version: i32,
        pub name: &'static str,
        pub up: &'static str,
        pub down: Option<&'static str>,
    }

    /// Get all available migrations in order
    pub fn get_migrations() -> Vec<Migration> {
        vec![
            Migration {
                version: 1,
                name: "create_initial_tables",
                up: r#"
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

                    -- Create ingredients table
                    CREATE TABLE IF NOT EXISTS ingredients (
                        id BIGSERIAL PRIMARY KEY,
                        user_id BIGINT NOT NULL REFERENCES users(id),
                        recipe_id BIGINT REFERENCES recipes(id),
                        name VARCHAR(255) NOT NULL,
                        quantity DECIMAL(10,3),
                        unit VARCHAR(50),
                        raw_text TEXT,
                        created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                        updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
                    );

                    -- Create indexes
                    CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
                    CREATE INDEX IF NOT EXISTS ingredients_user_id_idx ON ingredients(user_id);
                    CREATE INDEX IF NOT EXISTS ingredients_recipe_id_idx ON ingredients(recipe_id);
                "#,
                down: Some(
                    r#"
                    DROP TABLE IF EXISTS ingredients;
                    DROP TABLE IF EXISTS recipes;
                    DROP TABLE IF EXISTS users;
                "#,
                ),
            },
            Migration {
                version: 2,
                name: "recipes_owner_index_and_ingredient_cascade",
                // Every user-facing query filters recipes by owner, newest first; deleting a recipe
                // deletes its ingredients.
                up: r#"
                    CREATE INDEX IF NOT EXISTS recipes_telegram_id_created_at_idx
                        ON recipes (telegram_id, created_at DESC, id DESC);
                    ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
                    ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
                        FOREIGN KEY (recipe_id) REFERENCES recipes(id) ON DELETE CASCADE;
                "#,
                down: Some(
                    r#"
                    ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
                    ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
                        FOREIGN KEY (recipe_id) REFERENCES recipes(id);
                    DROP INDEX IF EXISTS recipes_telegram_id_created_at_idx;
                "#,
                ),
            },
            Migration {
                version: 3,
                name: "chat_messages",
                // Ids of the bot's and the user's messages that a later action deletes or edits
                // (UX plan U5): never their content
                up: r#"
                    CREATE TABLE IF NOT EXISTS chat_messages (
                        chat_id BIGINT NOT NULL,
                        message_id BIGINT NOT NULL,
                        kind TEXT NOT NULL,
                        recipe_id BIGINT,
                        sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        PRIMARY KEY (chat_id, message_id)
                    );
                    CREATE INDEX IF NOT EXISTS chat_messages_chat_kind_idx
                        ON chat_messages (chat_id, kind);
                "#,
                down: Some(
                    r#"
                    DROP TABLE IF EXISTS chat_messages;
                "#,
                ),
            },
        ]
    }

    /// Split SQL string into individual statements by semicolons
    pub fn split_sql_statements(sql: &str) -> Result<Vec<String>, String> {
        let mut statements = Vec::new();
        let mut current_statement = String::new();
        let mut in_string = false;
        let mut string_char = '\0';
        let mut in_comment = false;
        let mut chars = sql.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                // Handle string literals
                '"' | '\'' if !in_comment => {
                    if !in_string {
                        in_string = true;
                        string_char = ch;
                    } else if ch == string_char {
                        in_string = false;
                        string_char = '\0';
                    }
                    current_statement.push(ch);
                }
                // Handle comments
                '-' if !in_string && !in_comment => {
                    // Check if this is the start of a comment
                    if chars.peek() == Some(&'-') {
                        in_comment = true;
                        current_statement.push(ch); // Push first -
                        match chars.next() {
                            Some(second_dash) => current_statement.push(second_dash), // Push second -
                            None => {
                                return Err(
                                    "Unexpected end of input while parsing comment".to_string()
                                )
                            }
                        }
                    } else {
                        current_statement.push(ch);
                    }
                }
                '\n' if in_comment => {
                    in_comment = false;
                    current_statement.push(ch);
                }
                // Handle semicolons outside of strings and comments
                ';' if !in_string && !in_comment => {
                    current_statement.push(ch);
                    statements.push(current_statement.trim().to_string());
                    current_statement = String::new();
                }
                _ => {
                    current_statement.push(ch);
                }
            }
        }

        // Add any remaining statement
        if !current_statement.trim().is_empty() {
            statements.push(current_statement.trim().to_string());
        }

        Ok(statements)
    }

    /// Initialize the migrations table
    pub async fn init_migrations_table(pool: &PgPool) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .context("Failed to create migrations table")?;

        Ok(())
    }

    /// Get the current schema version
    pub async fn get_current_version(pool: &PgPool) -> Result<i32> {
        let version: Option<i32> =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
                .fetch_one(pool)
                .await
                .context("Failed to get current schema version")?;

        Ok(version.unwrap_or(0))
    }

    /// Apply pending migrations
    pub async fn apply_pending_migrations(pool: &PgPool) -> Result<()> {
        init_migrations_table(pool).await?;

        let current_version = get_current_version(pool).await?;
        let migrations = get_migrations();

        for migration in migrations {
            if migration.version > current_version {
                info!(
                    "Applying migration {}: {}",
                    migration.version, migration.name
                );

                // Execute the migration in a transaction
                let mut tx = pool.begin().await.context(format!(
                    "Failed to start transaction for migration {}",
                    migration.version
                ))?;

                // Split the migration SQL into individual statements and execute each one
                let statements = split_sql_statements(migration.up).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to parse SQL for migration {}: {}",
                        migration.version,
                        e
                    )
                })?;
                for statement in statements {
                    if !statement.trim().is_empty() {
                        sqlx::query(&statement)
                            .execute(&mut *tx)
                            .await
                            .context(format!(
                                "Failed to execute statement in migration {}: {}",
                                migration.version, migration.name
                            ))?;
                    }
                }

                // Record the migration
                sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
                    .bind(migration.version)
                    .bind(migration.name)
                    .execute(&mut *tx)
                    .await
                    .context(format!("Failed to record migration {}", migration.version))?;

                tx.commit()
                    .await
                    .context(format!("Failed to commit migration {}", migration.version))?;

                info!(
                    "✓ Successfully applied migration {}: {}",
                    migration.version, migration.name
                );
            }
        }

        let final_version = get_current_version(pool).await?;
        info!("Database schema is at version {}", final_version);

        Ok(())
    }

    /// Rollback to a specific version (for development/testing)
    pub async fn rollback_to_version(pool: &PgPool, target_version: i32) -> Result<()> {
        let current_version = get_current_version(pool).await?;
        let migrations = get_migrations();

        if target_version >= current_version {
            info!(
                "Already at version {} (target: {})",
                current_version, target_version
            );
            return Ok(());
        }

        // Get migrations to rollback in reverse order
        let migrations_to_rollback: Vec<&Migration> = migrations
            .iter()
            .filter(|m| m.version > target_version)
            .collect();

        for migration in migrations_to_rollback.into_iter().rev() {
            if let Some(down_sql) = migration.down {
                info!(
                    "Rolling back migration {}: {}",
                    migration.version, migration.name
                );

                let mut tx = pool.begin().await.context(format!(
                    "Failed to start transaction for rollback of migration {}",
                    migration.version
                ))?;

                // Split the rollback SQL into individual statements and execute each one
                let statements = split_sql_statements(down_sql).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to parse rollback SQL for migration {}: {}",
                        migration.version,
                        e
                    )
                })?;
                for statement in statements {
                    if !statement.trim().is_empty() {
                        sqlx::query(&statement)
                            .execute(&mut *tx)
                            .await
                            .context(format!(
                                "Failed to execute rollback statement in migration {}: {}",
                                migration.version, migration.name
                            ))?;
                    }
                }

                sqlx::query("DELETE FROM schema_migrations WHERE version = $1")
                    .bind(migration.version)
                    .execute(&mut *tx)
                    .await
                    .context(format!(
                        "Failed to remove migration record {}",
                        migration.version
                    ))?;

                tx.commit().await.context(format!(
                    "Failed to commit rollback of migration {}",
                    migration.version
                ))?;

                info!(
                    "✓ Successfully rolled back migration {}: {}",
                    migration.version, migration.name
                );
            } else {
                return Err(anyhow::anyhow!(
                    "Cannot rollback migration {}: no down migration defined",
                    migration.version
                ));
            }
        }

        let final_version = get_current_version(pool).await?;
        info!("Database schema rolled back to version {}", final_version);

        Ok(())
    }
}
