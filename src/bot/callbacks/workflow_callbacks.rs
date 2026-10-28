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

// Import database functions

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

    // The card leaves the history: forget it, then delete it
    crate::bot::cleanup::untrack(chat_id, message_id).await;
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
    let page: usize = data
        .strip_prefix("page:")
        .and_then(|page| page.parse().ok())
        .unwrap_or(0);
    debug!(page = %page, "Handling recipes pagination");
    // Can't respond to inaccessible messages
    let MaybeInaccessibleMessage::Regular(msg) = msg else {
        return Ok(());
    };
    // The list message becomes the requested page
    crate::bot::recipe_list::show_recipe_list(
        bot,
        &pool,
        msg.chat.id,
        page,
        Some(msg.id),
        language_code.as_deref(),
        localization,
    )
    .await
    .map(|_| ())
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
    // Can't respond to inaccessible messages
    let MaybeInaccessibleMessage::Regular(msg) = msg else {
        return Ok(());
    };
    // The list moves to the bottom, like at the end of an action
    crate::bot::home::send_home(
        bot,
        &pool,
        msg.chat.id,
        localization,
        language_code.as_deref(),
    )
    .await;
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
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .chat()
                    .id,
                t_lang(
                    localization,
                    "workflow-what-next",
                    q.from.language_code.as_deref(),
                ),
            )
            .await?;
            dialogue
                .update(crate::dialogue::RecipeDialogueState::Start)
                .await?;
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
                q.message
                    .as_ref()
                    .expect("Callback query should have a message"),
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
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .chat()
                    .id,
                t_lang(
                    localization,
                    "workflow-search-coming-soon",
                    q.from.language_code.as_deref(),
                ),
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}
