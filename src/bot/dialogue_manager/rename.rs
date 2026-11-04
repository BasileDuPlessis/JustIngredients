//! Renaming a saved recipe.

use crate::errors::AppError;
use crate::localization::{t_args_lang, t_lang};
use anyhow::Result;

// Import error logging utilities
use crate::errors::error_logging;

// Import validation functions
use crate::validation::validate_recipe_name;

// Import HandlerContext
use crate::bot::cleanup::send_transient;

use super::*;

/// Handle recipe rename input during dialogue
pub async fn handle_recipe_rename_input(
    ctx: DialogueContext<'_>,
    params: RecipeRenameInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let RecipeRenameInputParams {
        pool,
        new_name_input,
        recipe_id,
        current_name,
        ctx: handler_ctx,
    } = params;

    let input = new_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        // A cancel leaves nothing behind: the prompt goes and the card is active again
        dialogue.exit().await?;
        crate::bot::home::back_to_card(
            bot,
            pool,
            msg.chat.id,
            recipe_id,
            None,
            handler_ctx.localization,
            handler_ctx.language_code,
        )
        .await;
        return Ok(());
    }

    // Validate the new recipe name
    match validate_recipe_name(new_name_input) {
        Ok(validated_name) => {
            let rename_result = rename_for_sender(pool, msg, recipe_id, validated_name).await;
            crate::dialogue::end_after_write(&dialogue).await;
            // The history keeps one line for a rename; an error stays until the next action
            let failure = match rename_result {
                Ok(Some(new_name)) => {
                    show_renamed(
                        bot,
                        msg,
                        pool,
                        recipe_id,
                        &current_name,
                        &new_name,
                        handler_ctx,
                    )
                    .await?;
                    None
                }
                Ok(None) => Some(t_lang(
                    handler_ctx.localization,
                    "recipe-not-found",
                    handler_ctx.language_code,
                )),
                Err(e) => {
                    error_logging::log_database_error(
                        &e,
                        "update_recipe_name",
                        Some(msg.chat.id.0),
                        Some(&[("recipe_id", &recipe_id.to_string())]),
                    );
                    let text =
                        |key| t_lang(handler_ctx.localization, key, handler_ctx.language_code);
                    Some(format!(
                        "❌ {}\n\n{}",
                        text("error-renaming-recipe"),
                        text("error-renaming-recipe-help")
                    ))
                }
            };
            let mut keep = Vec::new();
            if let Some(failure) = failure {
                keep.push(bot.send_message(msg.chat.id, failure).await?.id);
            }
            // The list on the page of the recipe, where its new name puts it
            crate::bot::home::send_home_keeping_at(
                bot,
                pool,
                msg.chat.id,
                &keep,
                crate::bot::home::ListAt::Recipe(recipe_id),
                handler_ctx.localization,
                handler_ctx.language_code,
            )
            .await;
            Ok(())
        }
        Err(error) => {
            // Keep the dialogue: the user can type another name
            let key = match &error {
                AppError::Validation(reason) => reason.l10n_key(),
                _ => "recipe-name-invalid",
            };
            send_transient(
                bot,
                msg.chat.id,
                t_lang(handler_ctx.localization, key, handler_ctx.language_code),
            )
            .await?;
            Ok(())
        }
    }
}

/// A rename done: the recipe's cards, and the cards that name it among their linked recipes,
/// show the new name; the history gets one line, unless the name did not change.
async fn show_renamed(
    bot: &Bot,
    msg: &Message,
    pool: &PgPool,
    recipe_id: i64,
    current_name: &str,
    new_name: &str,
    handler_ctx: &HandlerContext<'_>,
) -> Result<()> {
    crate::observability::action(crate::observability::Action::Rename);
    // The cards already shown get the new name
    if let Some(user) = msg.from.as_ref() {
        crate::bot::cards::refresh_cards(
            bot,
            pool,
            msg.chat.id,
            crate::bot::telegram_id(user.id),
            recipe_id,
            handler_ctx.language_code,
            handler_ctx.localization,
        )
        .await;
        // And the cards that name it among their linked recipes
        let linked =
            crate::bot::cards::linked_ids(pool, crate::bot::telegram_id(user.id), recipe_id).await;
        crate::bot::cards::refresh_linked_cards(
            bot,
            pool,
            msg.chat.id,
            crate::bot::telegram_id(user.id),
            &linked,
            handler_ctx.language_code,
            handler_ctx.localization,
        )
        .await;
    }
    // Renamed to its own name: nothing changed, no line
    if new_name != current_name {
        let line = t_args_lang(
            handler_ctx.localization,
            "recipe-renamed-line",
            &[("old_name", current_name), ("new_name", new_name)],
            handler_ctx.language_code,
        );
        bot.send_message(msg.chat.id, line)
            .disable_notification(true)
            .await?;
    }
    Ok(())
}

/// Renames the recipe in the database for the sender of the message, its owner; a message
/// without a sender is handled like a missing recipe.
async fn rename_for_sender(
    pool: &PgPool,
    msg: &Message,
    recipe_id: i64,
    name: &str,
) -> anyhow::Result<Option<String>> {
    match msg.from.as_ref() {
        Some(user) => {
            crate::db::rename_recipe(pool, crate::bot::telegram_id(user.id), recipe_id, name).await
        }
        None => Ok(None),
    }
}
