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
use super::commands::command_lines;
use super::html::{send_html, Html};
use super::ui_builder::create_recipes_pagination_keyboard;

// Import HandlerContext
// use super::HandlerContext;

// Import observability
// use crate::observability;

/// The welcome message (HTML): bold title and commands header.
pub fn render_welcome(
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Html {
    let t = |key: &str| t_lang(localization, key, language_code);
    Html::text("👋 ")
        + Html::bold(&t("welcome-title"))
        + Html::text(&format!(
            "\n\n{}\n\n{}\n\n",
            t("welcome-description"),
            t("welcome-features")
        ))
        + Html::bold(&t("welcome-commands"))
        + Html::text(&format!(
            "\n{}\n\n{}",
            command_lines(localization, language_code),
            t("welcome-send-image")
        ))
}

/// The help message (HTML): bold title and section headers.
pub fn render_help(
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Html {
    let t = |key: &str| t_lang(localization, key, language_code);
    let paragraphs = |keys: &[&str]| {
        keys.iter()
            .map(|key| t(key))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    Html::bold(&t("help-title"))
        + Html::text("\n\n")
        + Html::text(&paragraphs(&[
            "help-description",
            "help-step1",
            "help-step2",
            "help-step3",
            "help-step4",
            "help-formats",
        ]))
        + Html::text("\n\n")
        + Html::bold(&t("help-commands"))
        + Html::text(&format!(
            "\n{}\n\n",
            command_lines(localization, language_code)
        ))
        + Html::bold(&t("help-tips"))
        + Html::text("\n\n")
        + Html::text(&paragraphs(&[
            "help-tip1",
            "help-tip2",
            "help-tip3",
            "help-tip4",
            "help-final",
        ]))
}

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

    let welcome_message = render_welcome(localization, language_code);
    send_html(bot, msg.chat.id, welcome_message).await?;
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

    let help_message = render_help(localization, language_code);
    send_html(bot, msg.chat.id, help_message).await?;
    Ok(())
}

/// Handle the search command (`/cherche`, `/search`): search is not available yet
pub async fn handle_search_command(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    if let Some(user) = msg.from.as_ref() {
        crate::observability::record_user_engagement_metrics(
            user.id.0 as i64,
            crate::observability::UserAction::RecipeSearch,
            None, // No session duration for individual actions
            language_code,
        );
    }

    bot.send_message(
        msg.chat.id,
        t_lang(localization, "search-coming-soon", language_code),
    )
    .await?;
    Ok(())
}

/// Handle a command the bot does not know: say so and list the commands
pub async fn handle_unknown_command(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    let message = format!(
        "{}\n\n{}",
        t_lang(localization, "unknown-command", language_code),
        command_lines(localization, language_code)
    );
    bot.send_message(msg.chat.id, message).await?;
    Ok(())
}

/// Handle the list command (`/liste`, `/list`, `/recipes`)
pub async fn handle_recipes_command(
    bot: &Bot,
    msg: &Message,
    pool: Arc<PgPool>,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
    _cache: Option<&std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    debug!(user_id = %msg.chat.id, "Handling list command");

    // Get paginated recipes for the user
    // Note: For now, we don't cache recipe lists since they change frequently
    // But we could cache user lookups if needed
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
            "📚 {}\n\n{}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::create_localization_manager;

    #[test]
    fn welcome_and_help_have_bold_titles_and_the_commands() {
        let localization = create_localization_manager().expect("localization");
        for language in ["en", "fr"] {
            let welcome = render_welcome(&localization, Some(language));
            let help = render_help(&localization, Some(language));
            for html in [welcome.as_str(), help.as_str()] {
                assert!(html.matches("<b>").count() >= 2, "{language}: {html}");
                assert_eq!(html.matches("<b>").count(), html.matches("</b>").count());
                assert!(!html.contains("Missing translation"), "{language}: {html}");
            }
            let commands = command_lines(&localization, Some(language));
            assert!(welcome.as_str().contains(&commands), "{language}");
            assert!(help.as_str().contains(&commands), "{language}");
        }
        assert!(render_help(&localization, Some("fr"))
            .as_str()
            .starts_with("<b>🆘 Aide d'Ingredients Bot</b>"));
    }
}
