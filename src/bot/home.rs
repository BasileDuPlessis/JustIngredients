//! The state every finished action returns to (save, cancel, delete, rename, edit of a saved
//! recipe): the messages the action left behind are deleted and the recipe list is sent again as
//! the last message of the chat. Since U5.2 the list replaces the former home message.

use std::sync::Arc;
use teloxide::prelude::*;
use tracing::warn;

use super::cleanup;
use crate::localization::{t_args_lang, LocalizationManager};

/// Sends the history line of a recipe that was just saved: "✅ Recette « X » créée", without
/// notification (the card shows when the recipe is opened from the list). Never fails: the
/// recipe is saved, so errors are only logged.
pub async fn send_created_line(
    bot: &Bot,
    chat_id: ChatId,
    recipe_name: &str,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    let line = t_args_lang(
        localization,
        "recipe-created-line",
        &[("name", recipe_name)],
        language_code,
    );
    if let Err(e) = bot
        .send_message(chat_id, line)
        .disable_notification(true)
        .await
    {
        warn!(error = %e, "Failed to send the created recipe line");
    }
}

/// Ends an action in a private chat (the chat id is the recipe owner): deletes the messages the
/// action left behind and the previous recipe list (see `cleanup`), then sends the list again
/// as the last message of the chat, without notification.
///
/// Never fails: the action that ends here has already succeeded, so errors are only logged.
pub async fn send_home(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    send_home_keeping(bot, pool, chat_id, &[], localization, language_code).await;
}

/// Like [`send_home`], but the messages in `keep` (an error the user must read, the photo it is
/// about) stay above the new list, and go when the next action ends.
pub async fn send_home_keeping(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    keep: &[teloxide::types::MessageId],
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    cleanup::delete_tracked(bot, chat_id).await;
    for id in keep {
        cleanup::track(chat_id, *id).await;
    }
    send_list(bot, pool, chat_id, localization, language_code).await;
}

/// "❌ Annuler" of an action started from a recipe card (rename, edit, delete): the action's
/// messages go and the card is the bottom message again, with its buttons (U6.2). Without a
/// recorded card (shown before U5.4), the list comes back instead.
pub async fn back_to_card(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    recipe_id: i64,
    card: Option<teloxide::types::MessageId>,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    cleanup::delete_action_messages(bot, chat_id).await;
    let keyboard =
        super::ui_builder::create_recipe_details_keyboard(recipe_id, language_code, localization);
    if !cleanup::reactivate_card(bot, chat_id, recipe_id, card, keyboard).await {
        send_home(bot, pool, chat_id, localization, language_code).await;
    }
}

/// Moves the recipe list to the bottom without ending anything: the messages above it stay
/// (`/start` shows its welcome this way).
pub async fn move_list_down(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    cleanup::delete_lists(bot, chat_id).await;
    send_list(bot, pool, chat_id, localization, language_code).await;
}

async fn send_list(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    match super::recipe_list::show_recipe_list(
        bot,
        pool,
        chat_id,
        0,
        None,
        language_code,
        localization,
    )
    .await
    {
        Ok(Some(id)) => cleanup::set_list(chat_id, id).await,
        Ok(None) => {}
        Err(e) => warn!(error = %e, "Failed to send the recipe list"),
    }
}
