//! Changing recipes, for every door (`specs/tasks/engine-api.md`, §15b): each change and its
//! event are written in one transaction (the outbox), so no change goes unannounced and no event
//! announces a change that did not happen. The rules are the bot's: unique names, names and
//! memos checked as when typed in Telegram.

use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::Postgres;

use super::events::{EventKind, Origin};
use super::{EngineError, Owner};
use crate::db::{IngredientChanges, RecipePart, SavedRecipe};
use crate::errors::AppError;

/// Who makes a change: the owner, through which door, with which token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub owner: Owner,
    pub origin: Origin,
    pub token_id: Option<i64>,
}

impl Actor {
    /// The sender of a Telegram update.
    pub const fn telegram(owner: Owner) -> Self {
        Self {
            owner,
            origin: Origin::Telegram,
            token_id: None,
        }
    }

    /// The holder of a token, through `origin`.
    pub fn token(authenticated: &super::tokens::Authenticated, origin: Origin) -> Self {
        Self {
            owner: authenticated.owner,
            origin,
            token_id: Some(authenticated.token_id),
        }
    }
}

/// A recipe deleted: its name and the recipes it was linked to, read before it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedRecipe {
    pub name: Option<String>,
    pub linked: Vec<i64>,
}

/// A transaction for a change and its event: the events lock is its first lock (`db::events`).
async fn begin(pool: &PgPool) -> Result<sqlx::Transaction<'static, Postgres>, EngineError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    crate::db::lock_events(&mut tx).await?;
    Ok(tx)
}

async fn commit(tx: sqlx::Transaction<'_, Postgres>) -> Result<(), EngineError> {
    Ok(tx.commit().await.map_err(anyhow::Error::from)?)
}

async fn record(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    actor: Actor,
    kind: EventKind,
    recipe_id: i64,
    payload: serde_json::Value,
) -> Result<(), EngineError> {
    crate::db::insert_event(
        tx,
        actor.owner.telegram_id(),
        &crate::db::NewEventRow {
            kind: kind.as_str(),
            recipe_id: Some(recipe_id),
            origin: actor.origin.as_str(),
            token_id: actor.token_id,
            payload,
        },
    )
    .await?;
    Ok(())
}

fn invalid(error: AppError) -> EngineError {
    match error {
        AppError::Validation(reason) => EngineError::Invalid(reason),
        other => EngineError::Internal(anyhow::anyhow!("{other}")),
    }
}

/// Saves the recipes of one photo or text (one per part, all linked together), each with its
/// `recipe.created` event. Returns them in order, with their stored (unique) names.
pub async fn create(
    pool: &PgPool,
    actor: Actor,
    language_code: Option<&str>,
    content: &str,
    parts: &[RecipePart<'_>],
) -> Result<Vec<SavedRecipe>, EngineError> {
    let mut tx = begin(pool).await?;
    let saved = crate::db::save_recipe_parts_in(
        &mut tx,
        actor.owner.telegram_id(),
        language_code,
        content,
        parts,
    )
    .await?;
    for recipe in &saved {
        let linked: Vec<i64> = saved
            .iter()
            .map(|other| other.recipe_id)
            .filter(|id| *id != recipe.recipe_id)
            .collect();
        record(
            &mut tx,
            actor,
            EventKind::RecipeCreated,
            recipe.recipe_id,
            json!({ "linked": linked }),
        )
        .await?;
    }
    commit(tx).await?;
    Ok(saved)
}

/// Renames a recipe of the owner; returns the stored name ("Tarte (2)" when the user already
/// has a "Tarte"). [`EngineError::Invalid`] for a name refused as in the bot,
/// [`EngineError::NotFound`] for a recipe that is not the owner's.
pub async fn rename(
    pool: &PgPool,
    actor: Actor,
    recipe_id: i64,
    wanted: &str,
) -> Result<String, EngineError> {
    let wanted = crate::validation::validate_recipe_name(wanted).map_err(invalid)?;
    let mut tx = begin(pool).await?;
    let Some(name) =
        crate::db::rename_recipe_in(&mut tx, actor.owner.telegram_id(), recipe_id, wanted).await?
    else {
        return Err(EngineError::NotFound);
    };
    record(
        &mut tx,
        actor,
        EventKind::RecipeRenamed,
        recipe_id,
        json!({}),
    )
    .await?;
    commit(tx).await?;
    Ok(name)
}

/// Sets (`Some`, cleaned and checked as a memo typed in the bot) or clears (`None`) the memo
/// of a recipe of the owner; returns the memo as stored.
pub async fn set_memo(
    pool: &PgPool,
    actor: Actor,
    recipe_id: i64,
    memo: Option<&str>,
) -> Result<Option<String>, EngineError> {
    let memo = memo
        .map(crate::validation::validate_recipe_note)
        .transpose()
        .map_err(invalid)?;
    let mut tx = begin(pool).await?;
    if !crate::db::set_recipe_note_in(
        &mut tx,
        actor.owner.telegram_id(),
        recipe_id,
        memo.as_deref(),
    )
    .await?
    {
        return Err(EngineError::NotFound);
    }
    record(&mut tx, actor, EventKind::MemoChanged, recipe_id, json!({})).await?;
    commit(tx).await?;
    Ok(memo)
}

/// Applies the edit of a recipe's ingredients (updates, additions, deletions), all or nothing.
pub async fn edit_ingredients(
    pool: &PgPool,
    actor: Actor,
    recipe_id: i64,
    language_code: Option<&str>,
    changes: IngredientChanges<'_>,
) -> Result<(), EngineError> {
    let mut tx = begin(pool).await?;
    if !crate::db::apply_ingredient_changes_in(
        &mut tx,
        actor.owner.telegram_id(),
        recipe_id,
        language_code,
        changes,
    )
    .await?
    {
        return Err(EngineError::NotFound);
    }
    record(
        &mut tx,
        actor,
        EventKind::IngredientsChanged,
        recipe_id,
        json!({}),
    )
    .await?;
    commit(tx).await?;
    Ok(())
}

/// Deletes a recipe of the owner with its ingredients; returns its name and linked recipes, read
/// before it went (its event carries them: they cannot be read afterwards).
pub async fn delete(
    pool: &PgPool,
    actor: Actor,
    recipe_id: i64,
) -> Result<DeletedRecipe, EngineError> {
    let telegram_id = actor.owner.telegram_id();
    // Read once the lock is held: no other change of the engine can come in between
    let mut tx = begin(pool).await?;
    let name = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id)
        .await?
        .ok_or(EngineError::NotFound)?
        .recipe_name;
    let linked: Vec<i64> = crate::db::linked_recipes(pool, telegram_id, recipe_id)
        .await?
        .into_iter()
        .map(|recipe| recipe.id)
        .collect();
    if !crate::db::delete_recipe_in(&mut tx, telegram_id, recipe_id).await? {
        return Err(EngineError::NotFound);
    }
    record(
        &mut tx,
        actor,
        EventKind::RecipeDeleted,
        recipe_id,
        json!({ "name": name, "linked": linked }),
    )
    .await?;
    commit(tx).await?;
    Ok(DeletedRecipe { name, linked })
}

/// Most ingredient lines a recipe given as text may have.
pub const MAX_LINES: usize = 100;

/// Creates one recipe from a name and its ingredients as typed lines ("200 g de farine"), read by
/// the bot's typed-line parser: units normalized, quantities checked. No photo, no Gemini call.
/// The first refused line is told with its number ([`EngineError::InvalidLine`]); nothing is
/// written then.
pub async fn create_from_lines(
    pool: &PgPool,
    actor: Actor,
    name: &str,
    lines: &[String],
) -> Result<SavedRecipe, EngineError> {
    let name = crate::validation::validate_recipe_name(name).map_err(invalid)?;
    if lines.len() > MAX_LINES {
        return Err(EngineError::TooMany { max: MAX_LINES });
    }
    // Blank lines are skipped; a refused line keeps its number in what was given
    let lines: Vec<(usize, &str)> = lines
        .iter()
        .map(|line| line.trim())
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(EngineError::Invalid(
            crate::errors::ValidationError::EmptyLine,
        ));
    }
    let mut parsed = Vec::with_capacity(lines.len());
    for &(index, line) in &lines {
        match crate::validation::parse_ingredient_from_text(line) {
            Ok(ingredient) => parsed.push(ingredient),
            Err(AppError::Validation(reason)) => {
                return Err(EngineError::InvalidLine {
                    line: index + 1,
                    reason,
                });
            }
            Err(other) => return Err(EngineError::Internal(anyhow::anyhow!("{other}"))),
        }
    }
    let ingredients = parsed.iter().map(crate::db::NewIngredient::from).collect();
    // The lines as the recipe's text: it tells an English recipe from a French one
    let content = lines
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    let saved = create(
        pool,
        actor,
        None,
        &content,
        &[RecipePart {
            name: name.to_string(),
            ingredients,
        }],
    )
    .await?;
    saved
        .into_iter()
        .next()
        .ok_or_else(|| EngineError::Internal(anyhow::anyhow!("No recipe saved")))
}
