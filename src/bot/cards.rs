//! Recipe cards in the chat history (UX plan U5.4): one card per recipe, always current. Opening
//! a recipe replaces its earlier card and moves the list below it; a rename or an edit updates
//! the recorded cards in place (a bot can edit its messages at any time).

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use tracing::{debug, info};

use crate::localization::{t_lang, LocalizationManager};

use super::cleanup;
use super::html::{edit_html, send_html, Html};
use super::ui_builder::{create_recipe_details_keyboard, render_recipe_card};

/// Cards of a recipe updated after a rename or an edit, the most recent first.
const MAX_CARDS_REFRESHED: usize = 20;

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
    // Measures in the user's unit view (U8); the default when it cannot be read
    let view = crate::db::unit_view(pool, telegram_id)
        .await
        .unwrap_or_else(|e| {
            debug!(error = %e, "Could not read the unit view");
            crate::unit_view::UnitView::default()
        });
    // "As in the recipe" names units in the recipe's language
    let english_recipe = crate::unit_view::recipe_is_english(
        &recipe.content,
        ingredients
            .iter()
            .map(|ingredient| ingredient.name.as_str()),
    );
    Ok(Some(render_recipe_card(
        recipe
            .recipe_name
            .as_ref()
            .map(|name| name.as_str())
            .unwrap_or("Unnamed Recipe"),
        recipe.created_at,
        &ingredients,
        view,
        english_recipe,
        language_code,
        localization,
    )))
}

/// Opens a recipe from the button on `pressed`: the card replaces the list at the bottom, with
/// its buttons (U6.1). Pressed on the list, the list message itself becomes the card: one
/// request, and nothing disappears before the card shows. Earlier cards of the recipe stay in
/// the history (every consultation is kept). A recipe that is gone answers `recipe-not-found`
/// until the next action.
pub async fn show_card(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    pressed: Option<teloxide::types::MessageId>,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let started = std::time::Instant::now();
    // Private chats only: the chat id is the user's id, the recipe's owner
    let telegram_id = chat_id.0;
    let Some(card) = render(pool, telegram_id, recipe_id, language_code, localization).await?
    else {
        let message = t_lang(localization, "recipe-not-found", language_code);
        let answer = cleanup::send_transient(bot, chat_id, message).await?;
        super::home::answer_at_bottom(bot, chat_id, answer.id, false, localization, language_code)
            .await;
        return Ok(());
    };
    let rendered = started.elapsed();
    // A second tap on the list's button reaches the card it became: nothing more to show
    if let Some(pressed) = pressed {
        if cleanup::is_card(chat_id, pressed).await {
            return Ok(());
        }
    }
    let keyboard = create_recipe_details_keyboard(recipe_id, language_code, localization);

    // The list pressed becomes the card
    let in_place = match pressed {
        Some(list) if cleanup::is_list(chat_id, list).await => {
            match edit_html(bot, chat_id, list, card.clone())
                .reply_markup(keyboard.clone())
                .await
            {
                Ok(_) => Some(list),
                Err(e) => {
                    debug!(error = %e, "Could not turn the list into the card");
                    None
                }
            }
        }
        _ => None,
    };
    let shown = started.elapsed();
    match in_place {
        Some(card_id) => cleanup::list_became_card(bot, chat_id, card_id, recipe_id).await,
        None => {
            // Opened from a home message or list of an older version: that message goes
            if let Some(pressed) = pressed {
                cleanup::remove_legacy_message(bot, chat_id, pressed).await;
            }
            // One message with buttons at the bottom: the list goes, an active card becomes
            // history
            cleanup::delete_lists(bot, chat_id).await;
            let sent = send_html(bot, chat_id, card).reply_markup(keyboard).await?;
            cleanup::record_active_card(chat_id, sent.id, recipe_id).await;
        }
    }
    info!(
        in_place = in_place.is_some(),
        read_ms = rendered.as_millis() as u64,
        shown_ms = if in_place.is_some() {
            shown
        } else {
            started.elapsed()
        }
        .as_millis() as u64,
        total_ms = started.elapsed().as_millis() as u64,
        "Recipe card shown"
    );
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
    // Every consultation stays in the history: only the most recent ones are updated, so a recipe
    // opened many times does not make a rename slow or hit Telegram's rate limit
    for recorded in cards.into_iter().rev().take(MAX_CARDS_REFRESHED) {
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
