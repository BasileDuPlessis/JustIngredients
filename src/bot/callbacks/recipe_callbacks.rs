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
    let telegram_id = crate::bot::telegram_id(from.id);

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

    // A card shown by an older version was never recorded: it becomes the active card now
    if let MaybeInaccessibleMessage::Regular(card) = msg {
        crate::bot::cleanup::adopt_card(chat_id, card.id, recipe_id).await;
    }
    let card_action = CardAction {
        bot,
        chat_id,
        pool: &pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    };

    match action {
        "rename" => ask_new_name(&card_action, dialogue).await?,
        "note" => ask_note(&card_action, dialogue).await?,
        "delete" => ask_delete_confirmation(&card_action, msg).await?,
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

/// What a button of a recipe card acts with.
#[derive(Clone, Copy)]
struct CardAction<'a> {
    bot: &'a Bot,
    chat_id: ChatId,
    pool: &'a Arc<PgPool>,
    /// The owner: the sender of the callback query.
    telegram_id: i64,
    recipe_id: i64,
    language_code: &'a Option<String>,
    localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// ✏️ Renommer on a card: the question goes at the bottom with its only exit ❌ (back to the
/// card), and the typed answer renames the recipe (`RenamingRecipe`).
async fn ask_new_name(action: &CardAction<'_>, dialogue: &RecipeDialogue) -> Result<()> {
    let CardAction {
        bot,
        chat_id,
        pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    } = *action;
    // Get current recipe details
    if let Ok(Some(recipe)) = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await {
        let current_name = recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe");

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
        let answer = send_transient(bot, chat_id, message).await?;
        crate::bot::home::answer_at_bottom(
            bot,
            chat_id,
            answer.id,
            false,
            localization,
            language_code.as_deref(),
        )
        .await;
    }
    Ok(())
}

/// 📌 Mémo on a card: the question goes at the bottom with the current note, "🗑️ Effacer la
/// note" and its exit "❌ Annuler" (back to the card); the typed answer is the new note
/// (`EditingRecipeNote`).
async fn ask_note(action: &CardAction<'_>, dialogue: &RecipeDialogue) -> Result<()> {
    let CardAction {
        bot,
        chat_id,
        pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    } = *action;
    let recipe = match crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await {
        Ok(recipe) => recipe,
        Err(error) => {
            error_logging::log_database_error(
                &error,
                "read_recipe_with_name",
                Some(telegram_id),
                Some(&[("recipe_id", &recipe_id.to_string())]),
            );
            None
        }
    };
    let Some(recipe) = recipe else {
        let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
        let answer = send_transient(bot, chat_id, message).await?;
        crate::bot::home::answer_at_bottom(
            bot,
            chat_id,
            answer.id,
            false,
            localization,
            language_code.as_deref(),
        )
        .await;
        return Ok(());
    };
    crate::bot::note::ask(
        bot,
        chat_id,
        dialogue,
        crate::bot::note::NoteTarget {
            recipe_id,
            name: recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe"),
            current: recipe.note.as_deref(),
        },
        false,
        localization,
        language_code.as_deref(),
    )
    .await
}

/// "🗑️ Effacer la note" on a note question opened from a card. Pressed when no such question
/// runs for `button_recipe` (an older one, another recipe's, another action), it does nothing.
pub async fn handle_clear_note(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    button_recipe: i64,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let MaybeInaccessibleMessage::Regular(msg) = msg else {
        return Ok(());
    };
    let Some(RecipeDialogueState::EditingRecipeNote {
        recipe_id,
        recipe_name,
        creation: false,
        language_code: dialogue_lang_code,
    }) = dialogue.get().await?
    else {
        return Ok(());
    };
    if recipe_id != button_recipe {
        return Ok(());
    }
    crate::bot::note::clear(
        crate::bot::note::NoteAnswer {
            bot,
            pool: &pool,
            chat_id: msg.chat.id,
            telegram_id: crate::bot::telegram_id(from.id),
            recipe_id,
            recipe_name: &recipe_name,
            creation: false,
            localization,
            // The language of the question, as for a typed answer
            language_code: dialogue_lang_code
                .as_deref()
                .or(from.language_code.as_deref()),
        },
        dialogue,
    )
    .await
}

/// 🗑️ Supprimer on a card: the confirmation goes at the bottom, with the card's id in its
/// buttons.
async fn ask_delete_confirmation(
    action: &CardAction<'_>,
    msg: &MaybeInaccessibleMessage,
) -> Result<()> {
    let CardAction {
        bot,
        chat_id,
        recipe_id,
        language_code,
        localization,
        ..
    } = *action;
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
    let telegram_id = crate::bot::telegram_id(from.id);

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

    let card_action = CardAction {
        bot,
        chat_id,
        pool: &pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    };
    // Errors stay above the list until the next action
    let (keep, list_at) = match action {
        "confirm_delete_recipe" => delete_confirmed(&card_action, msg, original_message_id).await?,
        "cancel_delete_recipe" => {
            // An action started meanwhile keeps its messages: only the confirmation goes
            if dialogue.get().await?.is_some_and(|state| state.is_action()) {
                if let MaybeInaccessibleMessage::Regular(msg) = msg {
                    let _ = bot.delete_message(chat_id, msg.id).await;
                    crate::bot::cleanup::untrack(chat_id, msg.id).await;
                }
                return Ok(());
            }
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
    };

    // A delete done in the middle of another action (a rename, a review) leaves that action
    // running: its home message would hide the prompts the action still waits on
    if dialogue.get().await?.is_some_and(|state| state.is_action()) {
        // The running action keeps its messages; the error goes when it ends
        for id in &keep {
            crate::bot::cleanup::track(chat_id, *id).await;
        }
    } else {
        crate::bot::home::send_home_keeping_at(
            bot,
            &pool,
            chat_id,
            &keep,
            list_at,
            localization,
            language_code.as_deref(),
        )
        .await;
    }
    Ok(())
}

/// 🗑️ Oui, supprimer: the recipe goes, its card becomes the history line, its linked cards
/// are refreshed. Returns the answers to keep above the list, and where the list opens.
async fn delete_confirmed(
    action: &CardAction<'_>,
    msg: &MaybeInaccessibleMessage,
    original_message_id: Option<i32>,
) -> Result<(Vec<teloxide::types::MessageId>, crate::bot::home::ListAt)> {
    let CardAction {
        bot,
        chat_id,
        pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    } = *action;
    let mut keep = Vec::new();
    // The list on the recipe's page, unless it is gone
    let mut list_at = crate::bot::home::ListAt::Recipe(recipe_id);
    // The name, for the history line that replaces the card
    let name = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id)
        .await
        .ok()
        .flatten()
        .and_then(|recipe| recipe.recipe_name);
    // Read before the links go with the recipe: their cards name it (U9)
    let linked = crate::bot::cards::linked_ids(pool, telegram_id, recipe_id).await;
    let actor = crate::engine::writes::Actor::telegram(crate::engine::Owner::telegram(telegram_id));
    let result = match crate::engine::writes::delete(pool, actor, recipe_id).await {
        Ok(_) => Ok(true),
        Err(crate::engine::EngineError::NotFound) => Ok(false),
        Err(error) => Err(anyhow::Error::from(error)),
    };

    // The confirmation goes in every case
    if let MaybeInaccessibleMessage::Regular(msg) = msg
        && let Err(e) = bot.delete_message(chat_id, msg.id).await
    {
        error_logging::log_internal_error(
            &e,
            "handle_delete_recipe_confirmation",
            "Failed to delete confirmation message",
            Some(chat_id.0),
        );
    }
    let card = original_message_id.map(teloxide::types::MessageId);

    match result {
        Ok(true) => {
            crate::observability::action(crate::observability::Action::Delete);
            // Gone: the page of its neighbours
            list_at = crate::bot::home::ListAt::Gone(name.clone().unwrap_or_default());
            show_deleted(action, name.as_deref(), card, &linked).await?;
        }
        Ok(false) => {
            // Already gone: its card goes too, the message stays until the next action
            if let Some(card) = card {
                crate::bot::cleanup::untrack(chat_id, card).await;
                let _ = bot.delete_message(chat_id, card).await;
            }
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
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
    Ok((keep, list_at))
}

/// A recipe deleted: its card becomes the history line "🗑️ « X » supprimée" (or the line is
/// sent when the card cannot be edited), its other cards go and its linked cards are refreshed.
async fn show_deleted(
    action: &CardAction<'_>,
    name: Option<&str>,
    card: Option<teloxide::types::MessageId>,
    linked: &[i64],
) -> Result<()> {
    let CardAction {
        bot,
        chat_id,
        pool,
        telegram_id,
        recipe_id,
        language_code,
        localization,
    } = *action;
    // The card becomes the history line "🗑️ « X » supprimée", without buttons
    let line = t_args_lang(
        localization,
        "recipe-deleted-line",
        &[("name", name.unwrap_or(""))],
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
    crate::bot::cards::refresh_linked_cards(
        bot,
        pool,
        chat_id,
        telegram_id,
        linked,
        language_code.as_deref(),
        localization,
    )
    .await;
    if let Some(card) = card {
        crate::bot::cleanup::untrack(chat_id, card).await;
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
    let telegram_id = crate::bot::telegram_id(from.id);

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
            let answer = send_transient(bot, chat_id, message).await?;
            crate::bot::home::answer_at_bottom(
                bot,
                chat_id,
                answer.id,
                false,
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
        let answer = send_transient(bot, chat_id, message).await?;
        crate::bot::home::answer_at_bottom(
            bot,
            chat_id,
            answer.id,
            false,
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
        Some(recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe")),
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
            open_menu: None,
        })
        .await?;

    Ok(())
}
