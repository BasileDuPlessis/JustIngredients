//! Recipe cards in the chat history (UX plan U5.4): one card per recipe, always current. Opening
//! a recipe replaces its earlier card and moves the list below it; a rename or an edit updates
//! the recorded cards in place (a bot can edit its messages at any time).

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use tracing::debug;

use crate::localization::{t_lang, LocalizationManager};

use super::cleanup;
use super::html::{edit_html, send_html, Html};
use super::ui_builder::{create_recipe_details_keyboard, render_recipe_card};

/// The card of a recipe of `telegram_id` as read now, or `None` when it is gone or another
/// user's.
async fn render(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<Option<Html>> {
    let Some(recipe) = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await? else {
        return Ok(None);
    };
    let ingredients = crate::db::get_recipe_ingredients(pool, telegram_id, recipe_id).await?;
    Ok(Some(render_recipe_card(
        recipe
            .recipe_name
            .as_ref()
            .map(|name| name.as_str())
            .unwrap_or("Unnamed Recipe"),
        recipe.created_at,
        &ingredients,
        language_code,
        localization,
    )))
}

/// Opens a recipe from the button on `pressed`: the card replaces the list at the bottom, with
/// its buttons (U6.1). Earlier cards of the recipe stay in the history (every consultation is
/// kept). A recipe that is gone answers `recipe-not-found` until the next action.
pub async fn show_card(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    pressed: Option<teloxide::types::MessageId>,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    // Private chats only: the chat id is the user's id, the recipe's owner
    let telegram_id = chat_id.0;
    let Some(card) = render(pool, telegram_id, recipe_id, language_code, localization).await?
    else {
        let message = t_lang(localization, "recipe-not-found", language_code);
        let answer = cleanup::send_transient(bot, chat_id, message).await?;
        super::home::answer_at_bottom(bot, chat_id, answer.id, localization, language_code).await;
        return Ok(());
    };
    // Opened from a home message or list of an older version: that message goes
    if let Some(pressed) = pressed {
        cleanup::remove_legacy_message(bot, chat_id, pressed).await;
    }
    // One message with buttons at the bottom: the list goes, an active card becomes history
    cleanup::delete_lists(bot, chat_id).await;
    let keyboard = create_recipe_details_keyboard(recipe_id, language_code, localization);
    let sent = send_html(bot, chat_id, card).reply_markup(keyboard).await?;
    cleanup::record_active_card(chat_id, sent.id, recipe_id).await;
    Ok(())
}

/// Updates every recorded card of a recipe in place, after a rename or an edit: the history
/// ones without buttons, the active one with them. Returns whether there was one (cards of
/// versions before U5.4 are not recorded).
pub async fn refresh_cards(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    telegram_id: i64,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> bool {
    let cards = cleanup::recipe_cards(chat_id, recipe_id).await;
    if cards.is_empty() {
        return false;
    }
    let card = match render(pool, telegram_id, recipe_id, language_code, localization).await {
        Ok(Some(card)) => card,
        Ok(None) => return false,
        Err(e) => {
            debug!(error = %e, "Could not read the recipe to update its cards");
            return false;
        }
    };
    for recorded in cards {
        let request = edit_html(bot, chat_id, recorded.id, card.clone());
        let result = if recorded.active {
            request
                .reply_markup(create_recipe_details_keyboard(
                    recipe_id,
                    language_code,
                    localization,
                ))
                .await
        } else {
            request.await
        };
        if let Err(e) = result {
            debug!(error = %e, "Could not update a recipe card");
        }
    }
    true
}
