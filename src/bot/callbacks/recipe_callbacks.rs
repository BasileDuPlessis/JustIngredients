//! Recipe callback handlers module
//!
//! This module contains all callback handlers related to recipe management operations,
//! including selection, deletion, statistics, and other recipe-related actions.

use crate::bot::html::send_html;
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardMarkup, MaybeInaccessibleMessage, User};
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use crate::bot::ui_builder::create_ingredient_review_keyboard;

// Import database functions
use crate::bot::cleanup::send_transient;

/// Handle recipe action callbacks (rename, delete)
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe accessed here.
pub async fn handle_recipe_action(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    // Parse callback data (format: "recipe_action:{action}:{recipe_id}")
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() < 3 || parts[0] != "recipe_action" {
        debug!(data = %data, "Invalid recipe action callback format");
        return Ok(());
    }

    let action = parts[1];
    let recipe_id_str = parts[2];
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);

    debug!(action = %action, recipe_id = %recipe_id, "Handling recipe action");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    match action {
        "rename" => {
            // Get current recipe details
            if let Ok(Some(recipe)) =
                crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await
            {
                let current_name = recipe
                    .recipe_name
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("Unnamed Recipe");

                let message = format!(
                    "🏷️ {}\n\n{}: {}\n\n{}",
                    t_lang(
                        localization,
                        "rename-recipe-title",
                        language_code.as_deref()
                    ),
                    t_lang(
                        localization,
                        "current-recipe-name",
                        language_code.as_deref()
                    ),
                    current_name,
                    t_lang(
                        localization,
                        "rename-recipe-instructions",
                        language_code.as_deref()
                    )
                );
                // The card goes into the history; the question is the action at the bottom, its
                // only exit "❌ Annuler" (back to the card)
                crate::bot::cleanup::delete_lists(bot, chat_id).await;
                let sent = bot
                    .send_message(chat_id, message)
                    .reply_markup(crate::bot::ui_builder::cancel_action_keyboard(
                        language_code.as_deref(),
                        localization,
                    ))
                    .await?;
                crate::bot::cleanup::track(chat_id, sent.id).await;

                // Transition to renaming state
                dialogue
                    .update(RecipeDialogueState::RenamingRecipe {
                        recipe_id,
                        current_name: current_name.to_string(),
                        language_code: language_code.clone(),
                    })
                    .await?;
            } else {
                let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
                send_transient(bot, chat_id, message).await?;
                crate::bot::home::move_list_down(
                    bot,
                    &pool,
                    chat_id,
                    localization,
                    language_code.as_deref(),
                )
                .await;
            }
        }
        "delete" => {
            // Get the original message ID to include in callback data
            let original_message_id = match msg {
                MaybeInaccessibleMessage::Regular(msg) => Some(msg.id),
                MaybeInaccessibleMessage::Inaccessible(_) => None,
            };

            let message = format!(
                "🗑️ {}\n\n{}",
                t_lang(
                    localization,
                    "delete-recipe-title",
                    language_code.as_deref()
                ),
                t_lang(
                    localization,
                    "delete-recipe-confirmation",
                    language_code.as_deref()
                )
            );

            // "🗑️ Oui, supprimer" above the single exit "❌ Annuler" (back to the card)
            let keyboard = vec![
                vec![teloxide::types::InlineKeyboardButton::callback(
                    t_lang(
                        localization,
                        "delete-confirm-button",
                        language_code.as_deref(),
                    ),
                    format!(
                        "confirm_delete_recipe:{}:{}",
                        recipe_id,
                        original_message_id.map(|id| id.0).unwrap_or(0)
                    ),
                )],
                vec![teloxide::types::InlineKeyboardButton::callback(
                    format!(
                        "❌ {}",
                        t_lang(localization, "cancel", language_code.as_deref())
                    ),
                    format!(
                        "cancel_delete_recipe:{}:{}",
                        recipe_id,
                        original_message_id.map(|id| id.0).unwrap_or(0)
                    ),
                )],
            ];
            // The card goes into the history while the confirmation is the bottom message
            crate::bot::cleanup::delete_lists(bot, chat_id).await;
            // Deleted by its own buttons, or with the next action's messages if ignored
            let sent = bot
                .send_message(chat_id, message)
                .reply_markup(teloxide::types::InlineKeyboardMarkup::new(keyboard))
                .await?;
            crate::bot::cleanup::track(chat_id, sent.id).await;
        }
        "edit_ingredients" => {
            handle_edit_ingredients_callback(
                bot,
                msg,
                recipe_id,
                pool,
                dialogue,
                from,
                localization,
            )
            .await?;
        }
        _ => {
            debug!(action = %action, "Unknown recipe action");
        }
    }

    Ok(())
}

/// Handle delete recipe confirmation callbacks
///
/// `from` is the sender of the callback query (`q.from`): only a recipe they own can be deleted.
pub async fn handle_delete_recipe_confirmation(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
    dialogue: &RecipeDialogue,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    debug!(data = %data, "Handling delete recipe confirmation");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Parse callback data (format: "confirm_delete_recipe:{recipe_id}:{original_message_id}" or "cancel_delete_recipe:{recipe_id}:{original_message_id}")
    let parts: Vec<&str> = data.split(':').collect();
    let action = parts[0];
    let recipe_id_str = parts.get(1).unwrap_or(&"");
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);
    let original_message_id: Option<i32> = parts
        .get(2)
        .and_then(|s| s.parse().ok())
        .filter(|&id| id != 0);

    // Errors stay above the list until the next action
    let mut keep = Vec::new();
    match action {
        "confirm_delete_recipe" => {
            // The name, for the history line that replaces the card
            let name = crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id)
                .await
                .ok()
                .flatten()
                .and_then(|recipe| recipe.recipe_name.map(|name| name.to_string()));
            let result = crate::db::delete_recipe(&pool, telegram_id, recipe_id).await;

            // The confirmation goes in every case
            if let MaybeInaccessibleMessage::Regular(msg) = msg {
                if let Err(e) = bot.delete_message(chat_id, msg.id).await {
                    error_logging::log_internal_error(
                        &e,
                        "handle_delete_recipe_confirmation",
                        "Failed to delete confirmation message",
                        Some(chat_id.0),
                    );
                }
            }
            let card = original_message_id.map(teloxide::types::MessageId);

            match result {
                Ok(true) => {
                    // The card becomes the history line "🗑️ « X » supprimée", without buttons
                    let line = t_args_lang(
                        localization,
                        "recipe-deleted-line",
                        &[("name", name.as_deref().unwrap_or(""))],
                        language_code.as_deref(),
                    );
                    let edited = match card {
                        Some(card) => bot
                            .edit_message_text(chat_id, card, line.clone())
                            .reply_markup(InlineKeyboardMarkup::default())
                            .await
                            .is_ok(),
                        None => false,
                    };
                    if !edited {
                        bot.send_message(chat_id, line)
                            .disable_notification(true)
                            .await?;
                    }
                    // The line is history, not a card; any other card of the recipe goes
                    crate::bot::cleanup::remove_recipe_cards(bot, chat_id, recipe_id, card).await;
                    if let Some(card) = card {
                        crate::bot::cleanup::untrack(chat_id, card).await;
                    }
                }
                Ok(false) => {
                    // Already gone: its card goes too, the message stays until the next action
                    if let Some(card) = card {
                        crate::bot::cleanup::untrack(chat_id, card).await;
                        let _ = bot.delete_message(chat_id, card).await;
                    }
                    let message =
                        t_lang(localization, "recipe-not-found", language_code.as_deref());
                    keep.push(bot.send_message(chat_id, message).await?.id);
                }
                Err(e) => {
                    // Nothing deleted: the card stays, the error until the next action
                    error_logging::log_database_error(
                        &e,
                        "delete_recipe",
                        Some(chat_id.0),
                        Some(&[("recipe_id", &recipe_id.to_string())]),
                    );
                    let message = format!(
                        "❌ {}\n\n{}",
                        t_lang(
                            localization,
                            "error-deleting-recipe",
                            language_code.as_deref()
                        ),
                        t_lang(
                            localization,
                            "error-deleting-recipe-help",
                            language_code.as_deref()
                        )
                    );
                    keep.push(bot.send_message(chat_id, message).await?.id);
                }
            }
        }
        "cancel_delete_recipe" => {
            // The confirmation goes and the card is the bottom message again
            crate::bot::home::back_to_card(
                bot,
                &pool,
                chat_id,
                recipe_id,
                original_message_id.map(teloxide::types::MessageId),
                localization,
                language_code.as_deref(),
            )
            .await;
            return Ok(());
        }
        _ => return Ok(()),
    }

    // A delete done in the middle of another action (a rename, a review) leaves that action
    // running: its home message would hide the prompts the action still waits on
    if dialogue.get().await?.is_some_and(|state| state.is_action()) {
        // The running action keeps its messages; the error goes when it ends
        for id in &keep {
            crate::bot::cleanup::track(chat_id, *id).await;
        }
    } else {
        crate::bot::home::send_home_keeping(
            bot,
            &pool,
            chat_id,
            &keep,
            localization,
            language_code.as_deref(),
        )
        .await;
    }
    Ok(())
}

/// Handle edit ingredients callback for saved recipes
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
async fn handle_edit_ingredients_callback(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    recipe_id: i64,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    debug!(recipe_id = %recipe_id, "Handling edit ingredients callback");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = match crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            send_transient(bot, chat_id, message).await?;
            crate::bot::home::move_list_down(
                bot,
                &pool,
                chat_id,
                localization,
                language_code.as_deref(),
            )
            .await;
            return Ok(());
        }
    };

    // Get current ingredients
    let original_ingredients =
        crate::db::get_recipe_ingredients(&pool, telegram_id, recipe_id).await?;
    if original_ingredients.is_empty() {
        let message = format!(
            "❌ {}\n\n{}",
            t_lang(
                localization,
                "no-ingredients-to-edit",
                language_code.as_deref()
            ),
            t_lang(
                localization,
                "no-ingredients-to-edit-help",
                language_code.as_deref()
            )
        );
        send_transient(bot, chat_id, message).await?;
        crate::bot::home::move_list_down(
            bot,
            &pool,
            chat_id,
            localization,
            language_code.as_deref(),
        )
        .await;
        return Ok(());
    }

    // Convert to measurement matches for editing
    let current_matches =
        crate::ingredient_editing::ingredients_to_measurement_matches(&original_ingredients);

    // Send editing interface
    let edit_message = crate::bot::ui_builder::render_saved_editing(
        Some(
            recipe
                .recipe_name
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("Unnamed Recipe"),
        ),
        &current_matches,
        language_code.as_deref(),
        localization,
    );

    let keyboard =
        create_ingredient_review_keyboard(&current_matches, language_code.as_deref(), localization);

    // The card goes into the history; the editing view is the action at the bottom
    crate::bot::cleanup::delete_lists(bot, chat_id).await;
    let sent_message = send_html(bot, chat_id, edit_message)
        .reply_markup(keyboard)
        .await?;
    // The editing view goes when the edit ends: the recipe's card is updated instead (U5.4)
    crate::bot::cleanup::track(chat_id, sent_message.id).await;

    // Transition to editing state
    dialogue
        .update(RecipeDialogueState::EditingSavedIngredients {
            recipe_id,
            original_ingredients,
            current_matches,
            language_code: language_code.clone(),
            message_id: Some(sent_message.id.0 as i32),
        })
        .await?;

    Ok(())
}
