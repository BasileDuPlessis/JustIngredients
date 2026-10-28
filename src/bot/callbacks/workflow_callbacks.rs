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
use crate::dialogue::RecipeDialogue;
use crate::localization::t_lang;

// Import UI builder functions

// Import database functions

/// "📚 Liste des recettes" (and the ⬅️ of older cards): the message it is on goes into the history
/// without buttons (a card) or away (an answer, an older home message), a running action ends
/// like with a command, and the recipe list comes back at the bottom (U6.1).
pub async fn handle_show_list(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!("Showing the recipe list");
    let MaybeInaccessibleMessage::Regular(msg) = msg else {
        return Ok(());
    };
    // A card of an older version was never recorded: it stays in the history without buttons
    if matches!(
        crate::bot::cleanup::is_recorded(msg.chat.id, msg.id).await,
        Some(false)
    ) {
        if let Err(e) = bot.edit_message_reply_markup(msg.chat.id, msg.id).await {
            debug!(error = %e, "Could not remove the buttons of an old card");
        }
    }
    if dialogue.get().await?.is_some_and(|state| state.is_action()) {
        dialogue.exit().await?;
    }
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
    // A list of an older version is recorded now, so the next action's cleanup takes it
    crate::bot::cleanup::adopt_legacy_list(msg.chat.id, msg.id).await;
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
    dialogue: &RecipeDialogue,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Only older home messages carry this button: that message goes, then like the list button
    if let MaybeInaccessibleMessage::Regular(msg) = msg {
        crate::bot::cleanup::remove_legacy_message(bot, msg.chat.id, msg.id).await;
    }
    handle_show_list(bot, msg, pool, dialogue, language_code, localization).await
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
                dialogue,
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
