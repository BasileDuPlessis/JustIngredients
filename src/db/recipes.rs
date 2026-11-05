//! Recipes: create, read, rename, delete, save with ingredients, links, lists.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use tracing::Instrument;
use tracing::debug;

use super::*;

/// Represents a recipe in the database
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Recipe {
    pub id: i64,
    pub telegram_id: i64,
    pub content: String,
    pub recipe_name: Option<String>,
    pub created_at: DateTime<Utc>,
    /// The short free text kept with the recipe (migration 10), cleaned and at most
    /// [`crate::validation::MAX_NOTE_CHARS`] characters.
    pub note: Option<String>,
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
    crate::observability::db_query("create_recipe", duration);

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

/// Ids of a recipe saved by [`save_recipe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRecipe {
    pub recipe_id: i64,
    /// Internal `users.id` of the owner.
    pub user_id: i64,
    /// The name stored: the one asked for, or "Tarte (2)" when the user already has a "Tarte".
    pub name: String,
}

/// Longest stored recipe name, in bytes (the limit of `validate_recipe_name`).
const MAX_RECIPE_NAME_BYTES: usize = 255;

/// `wanted` if none of `taken` has it, else the first free "wanted (n)" from 2, the name
/// shortened if needed so that the whole stays within the name limit.
pub fn unique_recipe_name(wanted: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == wanted) {
        return wanted.to_string();
    }
    (2u32..)
        .map(|n| {
            let suffix = format!(" ({n})");
            let mut base = wanted.to_string();
            while base.len() + suffix.len() > MAX_RECIPE_NAME_BYTES {
                base.pop();
            }
            format!("{}{suffix}", base.trim_end())
        })
        .find(|candidate| !taken.iter().any(|name| name == candidate))
        .unwrap_or_else(|| wanted.to_string())
}

/// The names of the user's recipes, except `except` (the recipe being renamed).
async fn recipe_names_of(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    telegram_id: i64,
    except: Option<i64>,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT recipe_name FROM recipes WHERE telegram_id = $1 AND recipe_name IS NOT NULL \
         AND id IS DISTINCT FROM $2",
    )
    .bind(telegram_id)
    .bind(except)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to read the recipe names")
}

/// Gives every user's recipes unique names: in each group of a user's recipes with the same name,
/// the oldest keeps it and the others become "Tarte (2)", "Tarte (3)"… (the first free numbers,
/// like a new recipe). Runs at startup; does nothing once names are unique. Returns the number
/// of recipes renamed.
pub async fn number_duplicate_recipe_names(pool: &PgPool) -> Result<u64> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start numbering duplicate names")?;
    let duplicates: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT id, telegram_id, recipe_name FROM recipes \
         WHERE (telegram_id, recipe_name) IN ( \
             SELECT telegram_id, recipe_name FROM recipes WHERE recipe_name IS NOT NULL \
             GROUP BY telegram_id, recipe_name HAVING COUNT(*) > 1) \
         ORDER BY telegram_id, recipe_name, created_at, id",
    )
    .fetch_all(&mut *tx)
    .await
    .context("Failed to find duplicate recipe names")?;

    let mut renamed = 0;
    let mut current_user = None;
    let mut taken: Vec<String> = Vec::new();
    let mut previous_name: Option<String> = None;
    for (id, telegram_id, name) in duplicates {
        if current_user != Some(telegram_id) {
            current_user = Some(telegram_id);
            taken = recipe_names_of(&mut tx, telegram_id, None).await?;
            previous_name = None;
        }
        // The oldest of a group keeps its name
        if previous_name.as_deref() != Some(name.as_str()) {
            previous_name = Some(name);
            continue;
        }
        let numbered = unique_recipe_name(&name, &taken);
        sqlx::query("UPDATE recipes SET recipe_name = $1 WHERE id = $2")
            .bind(&numbered)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("Failed to number a duplicate recipe name")?;
        taken.push(numbered);
        renamed += 1;
    }
    tx.commit()
        .await
        .context("Failed to commit the numbered names")?;
    Ok(renamed)
}

/// Renames a recipe of `telegram_id` to `wanted`, or to "wanted (n)" when another of the
/// user's recipes has that name. Returns the stored name, `None` when the recipe is not the
/// user's.
pub async fn rename_recipe(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    wanted: &str,
) -> Result<Option<String>> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start the rename transaction")?;
    let taken = recipe_names_of(&mut tx, telegram_id, Some(recipe_id)).await?;
    let name = unique_recipe_name(wanted, &taken);
    let renamed =
        sqlx::query("UPDATE recipes SET recipe_name = $1 WHERE id = $2 AND telegram_id = $3")
            .bind(&name)
            .bind(recipe_id)
            .bind(telegram_id)
            .execute(&mut *tx)
            .await
            .context("Failed to rename the recipe")?
            .rows_affected()
            > 0;
    tx.commit().await.context("Failed to commit the rename")?;
    Ok(renamed.then_some(name))
}

/// Sets (`Some`) or clears (`None`) the note of a recipe of `telegram_id`. `false` when the
/// recipe is gone or is another user's: nothing is written. The text must come out of
/// [`crate::validation::validate_recipe_note`].
pub async fn set_recipe_note(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    note: Option<&str>,
) -> Result<bool> {
    let start = std::time::Instant::now();
    let updated = sqlx::query("UPDATE recipes SET note = $1 WHERE id = $2 AND telegram_id = $3")
        .bind(note)
        .bind(recipe_id)
        .bind(telegram_id)
        .execute(pool)
        .await
        .context("Failed to write the recipe note")?
        .rows_affected()
        > 0;
    crate::observability::db_query("set_recipe_note", start.elapsed());
    Ok(updated)
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
    let parts = [RecipePart {
        name: name.to_string(),
        ingredients: ingredients.to_vec(),
    }];
    save_recipe_parts(pool, telegram_id, language_code, content, &parts)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No recipe saved"))
}

/// One recipe of a photo: its name and ingredients.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipePart<'a> {
    pub name: String,
    pub ingredients: Vec<NewIngredient<'a>>,
}

/// Saves the recipes a photo becomes (UX plan U9: one per part, "Malva pudding" and "Malva
/// pudding (sauce)") in one transaction, each name made unique like a new recipe's, and links
/// them all together. Returns them in order, with their stored names.
pub async fn save_recipe_parts(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
    content: &str,
    parts: &[RecipePart<'_>],
) -> Result<Vec<SavedRecipe>> {
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

    // One name per recipe of the user: a second "Tarte" is stored as "Tarte (2)"
    let mut taken = recipe_names_of(&mut tx, telegram_id, None).await?;
    let mut saved = Vec::with_capacity(parts.len());
    for part in parts {
        let name = unique_recipe_name(&part.name, &taken);
        let recipe_id: i64 = sqlx::query_scalar(
            "INSERT INTO recipes (telegram_id, content, recipe_name) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(telegram_id)
        .bind(content)
        .bind(&name)
        .fetch_one(&mut *tx)
        .await
        .context("Failed to insert the recipe")?;

        for (position, ingredient) in part.ingredients.iter().enumerate() {
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
        taken.push(name.clone());
        saved.push(SavedRecipe {
            recipe_id,
            user_id,
            name,
        });
    }

    // Every part linked to every other one, both ways
    for (index, first) in saved.iter().enumerate() {
        for second in &saved[index + 1..] {
            link_in(&mut tx, first.recipe_id, second.recipe_id).await?;
        }
    }

    tx.commit().await.context("Failed to commit the recipe")?;
    crate::observability::db_query("save_recipe", start_time.elapsed());
    Ok(saved)
}

async fn link_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    first: i64,
    second: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO recipe_links (recipe_a, recipe_b) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(first.min(second))
    .bind(first.max(second))
    .execute(&mut **tx)
    .await
    .context("Failed to link two recipes")?;
    Ok(())
}

/// The named recipes of `telegram_id` linked to `recipe_id`, in alphabetical order.
pub async fn linked_recipes(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Vec<RecipeSummary>> {
    let rows = sqlx::query_as::<_, RecipeSummary>(
        "SELECT r.id, r.recipe_name AS name, r.created_at FROM recipe_links l \
         JOIN recipes r ON r.id = CASE WHEN l.recipe_a = $2 THEN l.recipe_b ELSE l.recipe_a END \
         WHERE (l.recipe_a = $2 OR l.recipe_b = $2) \
           AND r.telegram_id = $1 AND r.recipe_name IS NOT NULL \
           AND EXISTS (SELECT 1 FROM recipes s WHERE s.id = $2 AND s.telegram_id = $1)",
    )
    .bind(telegram_id)
    .bind(recipe_id)
    .fetch_all(pool)
    .await
    .context("Failed to read the linked recipes")?;
    let mut linked: Vec<RecipeSummary> = rows;
    linked.sort_by(|a, b| crate::search::compare_names(&a.name, &b.name).then(a.id.cmp(&b.id)));
    Ok(linked)
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

    let row = sqlx::query_as::<_, Recipe>(
        "SELECT id, telegram_id, content, recipe_name, created_at, note FROM recipes WHERE id = $1 AND telegram_id = $2",
    )
    .bind(recipe_id)
    .bind(telegram_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read recipe with recipe")?;

    match row {
        Some(row) => {
            let recipe = row;
            debug!(recipe_id = %recipe_id, "Recipe with recipe found");
            Ok(Some(recipe))
        }
        None => {
            debug!(recipe_id = %recipe_id, "No recipe found");
            Ok(None)
        }
    }
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

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM recipes WHERE telegram_id = $1 AND recipe_name = $2")
            .bind(telegram_id)
            .bind(recipe_name)
            .fetch_one(pool)
            .await
            .context("Failed to check for duplicate recipes")?;

    let has_duplicates = count > 1;

    debug!(telegram_id = %telegram_id, recipe_name = %recipe_name, count = %count, has_duplicates = %has_duplicates, "Duplicate check completed");
    Ok(has_duplicates)
    }
    .instrument(span)
    .await
}

/// A saved recipe as shown in lists: id, name and creation date.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RecipeSummary {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// The user's named recipes with their ingredient names, newest first, for the search
/// (`crate::search`).
pub async fn searchable_recipes(
    pool: &PgPool,
    telegram_id: i64,
) -> Result<Vec<crate::search::SearchableRecipe>> {
    let rows: Vec<(i64, String, DateTime<Utc>, Vec<String>)> = sqlx::query_as(
        "SELECT r.id, r.recipe_name, r.created_at, \
                COALESCE(array_agg(i.name ORDER BY i.id) FILTER (WHERE i.name IS NOT NULL), '{}') \
         FROM recipes r LEFT JOIN ingredients i ON i.recipe_id = r.id \
         WHERE r.telegram_id = $1 AND r.recipe_name IS NOT NULL \
         GROUP BY r.id ORDER BY r.created_at DESC, r.id DESC",
    )
    .bind(telegram_id)
    .fetch_all(pool)
    .await
    .context("Failed to read the recipes to search")?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, created_at, ingredients)| crate::search::SearchableRecipe {
                id,
                name,
                created_at,
                ingredients,
            },
        )
        .collect())
}

/// Recipes of a user in alphabetical order (`search::compare_names`: case and accents ignored,
/// numbers by value), one page of them, and the user's total number of recipes. A user has at
/// most a few hundred recipes: the names are sorted here, without a Postgres collation.
///
/// Recipes without a name are left out, as in the recipe list.
pub async fn get_user_recipes_alphabetical(
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

    let recipes = alphabetical_recipes(pool, telegram_id).await?;
    let total = i64::try_from(recipes.len()).unwrap_or(i64::MAX);
    let page = recipes
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(0))
        .take(usize::try_from(limit).unwrap_or(0))
        .collect();
    Ok((page, total))
}

/// Every named recipe of `telegram_id`, in the order of the recipe list.
async fn alphabetical_recipes(pool: &PgPool, telegram_id: i64) -> Result<Vec<RecipeSummary>> {
    let rows = sqlx::query_as::<_, RecipeSummary>(
        "SELECT id, recipe_name AS name, created_at FROM recipes \
         WHERE telegram_id = $1 AND recipe_name IS NOT NULL",
    )
    .bind(telegram_id)
    .fetch_all(pool)
    .await
    .context("Failed to get the recipes")?;

    let mut recipes: Vec<RecipeSummary> = rows;
    recipes.sort_by(|a, b| {
        crate::search::compare_names(&a.name, &b.name)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.id.cmp(&b.id))
    });
    Ok(recipes)
}

/// Where a recipe of `telegram_id` comes in the recipe list, from 0; `None` when it is not
/// listed (another user's, unnamed or gone).
pub async fn recipe_list_position(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Result<Option<usize>> {
    Ok(alphabetical_recipes(pool, telegram_id)
        .await?
        .iter()
        .position(|recipe| recipe.id == recipe_id))
}

/// Where a recipe named `name` would come in the recipe list (a deleted one): the number of the
/// user's recipes listed before it.
pub async fn name_list_position(pool: &PgPool, telegram_id: i64, name: &str) -> Result<usize> {
    Ok(alphabetical_recipes(pool, telegram_id)
        .await?
        .iter()
        .filter(|recipe| crate::search::compare_names(&recipe.name, name).is_lt())
        .count())
}
