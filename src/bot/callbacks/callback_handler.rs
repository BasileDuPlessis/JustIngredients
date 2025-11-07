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

// Import localization
use crate::localization::t_lang;

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
        Some(RecipeDialogueState::EditingIngredient { .. }) => {
            handle_editing_ingredient_callbacks(&bot, &q, data, &dialogue, &localization).await
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

/// Handle callbacks when in EditingIngredient dialogue state
///
/// This function handles the cancel functionality for the focused editing interface:
/// - When user clicks "Cancel" during ingredient editing, restores the original recipe display
/// - Uses the original_message_id to replace the editing prompt back to the full recipe review
/// - Transitions dialogue state back to ReviewIngredients with proper message ID tracking
/// - Provides graceful fallback to sending new messages if editing fails
/// - Ensures seamless UX transition from focused editing back to full recipe review
async fn handle_editing_ingredient_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let dialogue_state = dialogue.get().await?;

    if let Some(RecipeDialogueState::EditingIngredient {
        recipe_name,
        ingredients,
        editing_index: _,
        language_code,
        message_id: _,
        original_message_id,
        extracted_text,
    }) = dialogue_state
    {
        if data == "cancel_ingredient_editing" {
            if let Some(msg) = &q.message {
                // Record user engagement metric for ingredient editing cancellation
                crate::observability::record_user_engagement_metrics(
                    q.from.id.0 as i64,
                    crate::observability::UserAction::IngredientEdit,
                    None, // No session duration for individual actions
                    language_code.as_deref(),
                );

                // Restore the original recipe display
                let review_message = format!(
                    "📝 **{}**\n\n{}\n\n{}",
                    t_lang(localization, "review-title", language_code.as_deref()),
                    t_lang(localization, "review-description", language_code.as_deref()),
                    crate::bot::format_ingredients_list(
                        &ingredients,
                        language_code.as_deref(),
                        localization
                    )
                );

                let keyboard = crate::bot::create_ingredient_review_keyboard(
                    &ingredients,
                    language_code.as_deref(),
                    localization,
                );

                // Use the original message ID to restore the recipe display
                if let Some(original_msg_id) = original_message_id {
                    match bot
                        .edit_message_text(
                            msg.chat().id,
                            teloxide::types::MessageId(original_msg_id),
                            review_message.clone(),
                        )
                        .reply_markup(keyboard.clone())
                        .await
                    {
                        Ok(_) => (),
                        Err(e) => {
                            crate::errors::error_logging::log_internal_error(
                                &e,
                                "handle_editing_ingredient_callbacks",
                                "Failed to restore original recipe display after cancel",
                                Some(msg.chat().id.0),
                            );
                            // Fallback: send new message if editing fails
                            bot.send_message(msg.chat().id, review_message)
                                .reply_markup(keyboard)
                                .await?;
                        }
                    }
                } else {
                    // No original message ID, send new message
                    bot.send_message(msg.chat().id, review_message)
                        .reply_markup(keyboard)
                        .await?;
                }

                // Reset dialogue state to review ingredients
                dialogue
                    .update(RecipeDialogueState::ReviewIngredients {
                        recipe_name,
                        ingredients,
                        language_code,
                        message_id: original_message_id, // Use original message ID for the restored display
                        extracted_text,
                        recipe_name_from_caption: None, // Recipe name came from user input, not caption
                    })
                    .await?;
            }
        }
    }

    Ok(())
}
