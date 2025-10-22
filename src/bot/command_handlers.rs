//! Command Handlers module for processing bot commands

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import localization
use crate::localization::t_lang;

// Import database functions
use crate::db::get_user_recipes_paginated;

// Import UI builder functions
use super::ui_builder::create_recipes_pagination_keyboard;

// Import HandlerContext
// use super::HandlerContext;

// Import observability
// use crate::observability;

/// Handle the /start command
pub async fn handle_start_command(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    // Record user engagement metric for start command
    if let Some(user) = msg.from.as_ref() {
        crate::observability::record_user_engagement_metrics(
            user.id.0 as i64,
            crate::observability::UserAction::StartCommand,
            None, // No session duration for individual actions
            language_code,
        );
    }

    let welcome_message = format!(
        "👋 **{}**\n\n{}\n\n{}\n\n{}\n{}\n{}\n\n{}",
        t_lang(localization, "welcome-title", language_code),
        t_lang(localization, "welcome-description", language_code),
        t_lang(localization, "welcome-features", language_code),
        t_lang(localization, "welcome-commands", language_code),
        t_lang(localization, "welcome-start", language_code),
        t_lang(localization, "welcome-help", language_code),
        t_lang(localization, "welcome-send-image", language_code)
    );
    bot.send_message(msg.chat.id, welcome_message).await?;
    Ok(())
}

/// Handle the /help command
pub async fn handle_help_command(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    // Record user engagement metric for help command
    if let Some(user) = msg.from.as_ref() {
        crate::observability::record_user_engagement_metrics(
            user.id.0 as i64,
            crate::observability::UserAction::HelpCommand,
            None, // No session duration for individual actions
            language_code,
        );
    }

    let help_message = vec![
        t_lang(localization, "help-title", language_code),
        t_lang(localization, "help-description", language_code),
        t_lang(localization, "help-step1", language_code),
        t_lang(localization, "help-step2", language_code),
        t_lang(localization, "help-step3", language_code),
        t_lang(localization, "help-step4", language_code),
        t_lang(localization, "help-formats", language_code),
        t_lang(localization, "help-commands", language_code),
        t_lang(localization, "help-start", language_code),
        t_lang(localization, "help-tips", language_code),
        t_lang(localization, "help-tip1", language_code),
        t_lang(localization, "help-tip2", language_code),
        t_lang(localization, "help-tip3", language_code),
        t_lang(localization, "help-tip4", language_code),
        t_lang(localization, "help-final", language_code),
    ]
    .join("\n\n");
    bot.send_message(msg.chat.id, help_message).await?;
    Ok(())
}

/// Handle the /recipes command
pub async fn handle_recipes_command(
    bot: &Bot,
    msg: &Message,
    pool: Arc<PgPool>,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(user_id = %msg.chat.id, "Handling /recipes command");

    // Get paginated recipes for the user
    let (recipes, total_count) = get_user_recipes_paginated(&pool, msg.chat.id.0, 5, 0).await?;

    if recipes.is_empty() {
        // No recipes found
        let no_recipes_message = format!(
            "📚 {}\n\n{}",
            t_lang(localization, "no-recipes-found", language_code),
            t_lang(localization, "no-recipes-suggestion", language_code)
        );
        bot.send_message(msg.chat.id, no_recipes_message).await?;
    } else {
        // Create the message text
        let recipes_message = format!(
            "📚 **{}**\n\n{}",
            t_lang(localization, "your-recipes", language_code),
            t_lang(localization, "select-recipe", language_code)
        );

        // Create the pagination keyboard
        let keyboard = create_recipes_pagination_keyboard(
            &recipes,
            0,
            total_count,
            5,
            language_code,
            localization,
        );

        bot.send_message(msg.chat.id, recipes_message)
            .reply_markup(keyboard)
            .await?;
    }

    Ok(())
}

/// Handle unsupported message types
pub async fn handle_unsupported_message(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    debug!(user_id = %msg.chat.id, "Received unsupported message type from user");

    let help_message = format!(
        "{}\n\n{}\n{}\n{}\n{}\n{}\n\n{}",
        t_lang(localization, "unsupported-title", language_code),
        t_lang(localization, "unsupported-description", language_code),
        t_lang(localization, "unsupported-feature1", language_code),
        t_lang(localization, "unsupported-feature2", language_code),
        t_lang(localization, "unsupported-feature3", language_code),
        t_lang(localization, "unsupported-feature4", language_code),
        t_lang(localization, "unsupported-final", language_code)
    );
    bot.send_message(msg.chat.id, help_message).await?;
    Ok(())
}
