//! The state every finished action returns to (save, cancel, delete, rename, edit of a saved
//! recipe): the messages the action left behind are deleted and the recipe list is sent again as
//! the last message of the chat. Since U5.2 the list replaces the former home message.

use std::sync::Arc;
use teloxide::prelude::*;
use tracing::warn;

use super::cleanup;
use super::html::send_html;
use super::ui_builder::{create_recipe_details_keyboard, render_saved_recipe};
use crate::localization::LocalizationManager;

/// Sends the card of a recipe that was just saved (name, ingredients, card buttons). Never fails:
/// the recipe is saved, so errors are only logged.
pub async fn send_saved_recipe(
    bot: &Bot,
    chat_id: ChatId,
    recipe_id: i64,
    recipe_name: &str,
    ingredients: &[crate::text_processing::MeasurementMatch],
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    let card = render_saved_recipe(recipe_name, ingredients, language_code, localization);
    let keyboard = create_recipe_details_keyboard(recipe_id, language_code, localization);
    match send_html(bot, chat_id, card).reply_markup(keyboard).await {
        Ok(sent) => cleanup::record_card(chat_id, sent.id, recipe_id).await,
        Err(e) => warn!(error = %e, "Failed to send the saved recipe card"),
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
