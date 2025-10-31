//! Callback Handler module for processing inline keyboard callback queries

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import recipe callbacks module
use super::recipe_callbacks;

// Import workflow callbacks module
use super::workflow_callbacks;

// Import review callbacks module
use super::review_callbacks;

// Import editing callbacks module
use super::editing_callbacks;

// Import observability
use crate::observability;

/// Handle callback queries from inline keyboards
pub async fn callback_handler(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let span = crate::observability::telegram_span("callback_handler", Some(q.from.id.0 as i64));
    let _enter = span.enter();

    let start_time = std::time::Instant::now();

    // Check dialogue state
    let dialogue_state = dialogue.get().await?;
    debug!(user_id = %q.from.id, dialogue_state = ?dialogue_state, "Retrieved dialogue state");

    let data = q.data.as_deref().unwrap_or("");

    let result = match dialogue_state {
        Some(RecipeDialogueState::ReviewIngredients { .. }) => {
            review_callbacks::handle_review_ingredients_callbacks(
                &bot,
                &q,
                data,
                pool.clone(),
                &dialogue,
                &localization,
            )
            .await
        }
        Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
            editing_callbacks::handle_editing_saved_ingredients_callbacks(
                &bot,
                &q,
                data,
                pool.clone(),
                &dialogue,
                &localization,
            )
            .await
        }
        _ => Ok(()), // No state-specific handling needed
    };

    // Handle general callbacks that work in any state
    if let Some(msg) = &q.message {
        if data.starts_with("select_recipe:") {
            recipe_callbacks::handle_recipe_selection(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("recipe_instance:") {
            recipe_callbacks::handle_recipe_instance_selection(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("recipe_action:") {
            recipe_callbacks::handle_recipe_action(
                &bot,
                msg,
                data,
                pool.clone(),
                &dialogue,
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data == "back_to_recipes" {
            workflow_callbacks::handle_back_to_recipes(
                &bot,
                msg,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("confirm_delete_recipe")
            || data.starts_with("cancel_delete_recipe")
        {
            recipe_callbacks::handle_delete_recipe_confirmation(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("page:") {
            workflow_callbacks::handle_recipes_pagination(
                &bot,
                msg,
                data,
                pool,
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("workflow_") {
            workflow_callbacks::handle_workflow_button(
                &bot,
                &q,
                data,
                &pool,
                &dialogue,
                &localization,
            )
            .await?;
        }
    }

    // Answer the callback query to remove the loading state
    bot.answer_callback_query(q.id).await?;

    let duration = start_time.elapsed();
    observability::record_request_metrics("telegram_callback", 200, duration);

    result
}

/// Cache-enabled callback handler for improved performance
///
/// This version includes caching for database queries to reduce
/// database load and improve response times.
pub async fn callback_handler_with_cache(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    _cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    // For now, delegate to the original handler
    // TODO: Integrate caching into specific operations
    callback_handler(bot, q, pool, dialogue, localization).await
}
