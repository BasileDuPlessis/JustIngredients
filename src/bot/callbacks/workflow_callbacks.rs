//! Workflow callback handlers module
//!
//! This module contains all callback handlers related to UI workflow and navigation,
//! including recipe listing, pagination, and post-confirmation actions.

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::MaybeInaccessibleMessage;
use tracing::debug;

// Import localization
use crate::localization::t_lang;

// Import UI builder functions
use crate::bot::ui_builder::create_recipes_pagination_keyboard;

// Import database functions
use crate::db::get_user_recipes_paginated;

/// Handle back to recipes callback - simply deletes the current message
pub async fn handle_back_to_recipes(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    _pool: Arc<PgPool>,
    _language_code: &Option<String>,
    _localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!("Handling back to recipes - removing message");

    // Extract chat id and message id from the message
    let (chat_id, message_id) = match msg {
        MaybeInaccessibleMessage::Regular(msg) => (msg.chat.id, msg.id),
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't delete inaccessible messages
            return Ok(());
        }
    };

    // Simply delete the message - no database queries, no content regeneration
    if let Err(e) = bot.delete_message(chat_id, message_id).await {
        debug!("Failed to delete message: {:?}", e);
        // If deletion fails, just ignore - the message might already be deleted or inaccessible
    }

    Ok(())
}

/// Handle recipes pagination callback
pub async fn handle_recipes_pagination(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let page_str = data.strip_prefix("page:").unwrap_or("0");
    let page: usize = page_str.parse().unwrap_or(0);
    debug!(page = %page, "Handling recipes pagination");

    // Extract chat id from the message
    let (chat_id, message_id) = match msg {
        MaybeInaccessibleMessage::Regular(msg) => (msg.chat.id, msg.id),
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Calculate offset
    let limit = 5i64;
    let offset = (page as i64) * limit;

    // Get paginated recipes
    let (recipes, total_count) =
        get_user_recipes_paginated(&pool, chat_id.0, limit, offset).await?;

    if recipes.is_empty() {
        // This shouldn't happen in normal pagination, but handle gracefully
        let message = t_lang(localization, "no-recipes-found", language_code.as_deref());
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Create updated message text
    let recipes_message = format!(
        "📚 **{}**\n\n{}",
        t_lang(localization, "your-recipes", language_code.as_deref()),
        t_lang(localization, "select-recipe", language_code.as_deref())
    );

    // Create updated keyboard
    let keyboard = create_recipes_pagination_keyboard(
        &recipes,
        page,
        total_count,
        limit,
        language_code.as_deref(),
        localization,
    );

    // Edit the original message
    bot.edit_message_text(chat_id, message_id, recipes_message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle list recipes workflow callback
pub async fn handle_list_recipes(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!("Handling list recipes workflow");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get user's recipes (first page)
    let limit = 5i64;
    let offset = 0i64;
    let (recipes, total_count) =
        get_user_recipes_paginated(&pool, chat_id.0, limit, offset).await?;

    if recipes.is_empty() {
        // No recipes found
        let message = format!(
            "📚 **{}**\n\n{}",
            t_lang(localization, "your-recipes", language_code.as_deref()),
            t_lang(
                localization,
                "no-recipes-suggestion",
                language_code.as_deref()
            )
        );
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Create message text
    let recipes_message = format!(
        "📚 **{}**\n\n{}",
        t_lang(localization, "your-recipes", language_code.as_deref()),
        t_lang(localization, "select-recipe", language_code.as_deref())
    );

    // Create keyboard
    let keyboard = create_recipes_pagination_keyboard(
        &recipes,
        0, // current page
        total_count,
        limit,
        language_code.as_deref(),
        localization,
    );

    // Send the message with keyboard
    bot.send_message(chat_id, recipes_message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle workflow button callbacks (post-confirmation actions)
pub async fn handle_workflow_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: &Arc<PgPool>,
    dialogue: &crate::dialogue::RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    match data {
        "workflow_add_another" => {
            // Record user engagement metric for workflow continuation
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::WorkflowContinue,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            bot.send_message(
                q.message.as_ref().unwrap().chat().id,
                t_lang(
                    localization,
                    "workflow-what-next",
                    q.from.language_code.as_deref(),
                ),
            )
            .await?;
            dialogue.update(crate::dialogue::RecipeDialogueState::Start).await?;
        }
        "workflow_list_recipes" => {
            // Record user engagement metric for recipe listing
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::RecipesCommand,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            handle_list_recipes(
                bot,
                q.message.as_ref().unwrap(),
                pool.clone(),
                &q.from.language_code,
                localization,
            )
            .await?;
        }
        "workflow_search_recipes" => {
            // Record user engagement metric for recipe search
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::RecipeSearch,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            bot.send_message(
                q.message.as_ref().unwrap().chat().id,
                "🔍 Recipe search coming soon! For now, use the 'List My Recipes' button.",
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}