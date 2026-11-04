//! Ingredients: rows of a recipe, edits of a saved recipe, unit normalization.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use tracing::Instrument;
use tracing::{debug, warn};

use crate::errors::error_logging;

/// Represents an ingredient in the database
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
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

/// An ingredient of a recipe being saved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewIngredient<'a> {
    pub name: &'a str,
    /// `None` for an ingredient without quantity ("sel").
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// Rewrites the units saved before they were normalized (UX plan U8.3) the way photos and typed
/// lines are normalized now (`units::normalize_measurement`): "G", "gram", "gr" → g, "litres"
/// → l, cl and dl → ml with the quantity converted, spoons in words → tbsp and tsp, count units
/// lowercased ("PINCÉES" → "pincées"). Runs at startup; does nothing once every unit is
/// normalized. Returns the number of ingredients changed.
///
/// Each ingredient is written on its own (the result does not depend on the others, and a second
/// run finds nothing to do), so one that cannot be written is logged and the others are; only
/// what `units::renormalized` finds safe is written. A blank unit becomes none.
pub async fn normalize_saved_units(pool: &PgPool) -> Result<u64> {
    let saved: Vec<(i64, Option<f64>, String)> = sqlx::query_as(
        "SELECT id, quantity::float8, unit FROM ingredients WHERE unit IS NOT NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("Failed to read the saved units")?;

    let mut changed = 0;
    for (id, quantity, unit) in saved {
        let Some(normalized) = crate::units::renormalized(quantity, &unit) else {
            continue;
        };
        let written = sqlx::query("UPDATE ingredients SET quantity = $1, unit = $2 WHERE id = $3")
            .bind(normalized.quantity)
            .bind(normalized.unit.as_deref())
            .bind(id)
            .execute(pool)
            .await;
        match written {
            Ok(_) => changed += 1,
            Err(e) => warn!(ingredient = id, error = %e, "Could not normalize a saved unit"),
        }
    }
    Ok(changed)
}

/// New values of a saved ingredient, written as they are (`None` clears the quantity or unit).
#[derive(Debug, Clone, PartialEq)]
pub struct IngredientUpdate<'a> {
    pub id: i64,
    pub ingredient: NewIngredient<'a>,
}

/// Applies the edit of a saved recipe of `telegram_id` in one transaction: every update, addition
/// and deletion, or none of them. An update writes the given values as they are, so a quantity
/// or unit can be removed. Fails without writing when the recipe is not the user's.
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
    crate::observability::db_query("apply_ingredient_changes", start_time.elapsed());
    Ok(())
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

    let result = sqlx::query_scalar::<_, i64>(
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
    crate::observability::db_query("create_ingredient", duration);

    match result {
        Ok(ingredient_id) => {
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

/// Get all ingredients for a specific recipe owned by `telegram_id`
///
/// Returns an empty list when the recipe does not exist or is owned by another user.
pub async fn get_recipe_ingredients(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Vec<Ingredient>> {
    debug!("Getting ingredients for recipe_id: {recipe_id}");

    let rows = sqlx::query_as::<_, Ingredient>("SELECT i.id, i.user_id, i.recipe_id, i.name, i.quantity::float8 AS quantity, i.unit, i.created_at, i.updated_at FROM ingredients i JOIN recipes r ON r.id = i.recipe_id WHERE i.recipe_id = $1 AND r.telegram_id = $2 ORDER BY i.created_at ASC, i.id ASC")
        .bind(recipe_id)
        .bind(telegram_id)
        .fetch_all(pool)
        .await
        .context("Failed to get recipe ingredients")?;

    let ingredients: Vec<Ingredient> = rows;

    debug!(
        "Found {} ingredients for recipe_id: {recipe_id}",
        ingredients.len()
    );
    Ok(ingredients)
}
