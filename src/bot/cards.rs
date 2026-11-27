//! Recipe cards in the chat history (UX plan U5.4): one card per recipe, always current. Opening
//! a recipe replaces its earlier card and moves the list below it; a rename or an edit updates
//! the recorded cards in place (a bot can edit its messages at any time). A card may show its
//! quantities scaled (`specs/tasks/recipe-scaling.md`): its record keeps the scale, so an update
//! keeps it; the recipe itself never changes.

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use tracing::{debug, info};

use crate::engine::scaling::{Ratio, Scale};
use crate::localization::{LocalizationManager, t_lang};

use super::cleanup;
use super::html::{Html, edit_html, send_html};
use super::ui_builder::{create_recipe_details_keyboard, render_recipe_card};

/// Cards of a recipe updated after a rename or an edit, the most recent first.
const MAX_CARDS_REFRESHED: usize = 20;

/// The card of a recipe of `telegram_id` as the engine reads it now, or `None` when it is gone
/// or another user's.
async fn render(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    scale: Option<&Scale>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<Option<Html>> {
    let owner = crate::engine::Owner::telegram(telegram_id);
    let lang = crate::engine::Lang::from_code(language_code);
    // The user's own unit view (U8)
    match crate::engine::recipes::scaled_recipe(pool, owner, recipe_id, None, lang, scale).await {
        Ok(recipe) => Ok(Some(render_recipe_card(
            &recipe,
            language_code,
            localization,
        ))),
        Err(crate::engine::EngineError::NotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// The card of a recipe of `telegram_id` as it would be sent (Telegram HTML); `None` when the
/// recipe is gone or another user's. The golden tests of the card read it: its result must not
/// change when the way the card is built does (`specs/tasks/engine-api.md`).
pub async fn card_html(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<Option<String>> {
    Ok(render(
        pool,
        telegram_id,
        recipe_id,
        None,
        language_code,
        localization,
    )
    .await?
    .map(|card| card.as_str().to_string()))
}

/// The recipes linked to a card's recipe (U9); none when they cannot be read.
async fn linked_recipes(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
) -> Vec<crate::db::RecipeSummary> {
    crate::db::linked_recipes(pool, telegram_id, recipe_id)
        .await
        .unwrap_or_else(|e| {
            debug!(error = %e, "Could not read the linked recipes");
            Vec::new()
        })
}

/// Updates the cards of the recipes linked to one that was renamed or deleted: their "🔗 Recettes
/// liées" line names it (U9). `linked` is read before a delete.
pub async fn refresh_linked_cards(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    telegram_id: i64,
    linked: &[i64],
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) {
    for &recipe_id in linked {
        refresh_cards(
            bot,
            pool,
            chat_id,
            telegram_id,
            recipe_id,
            language_code,
            localization,
        )
        .await;
    }
}

/// The ids of the recipes linked to `recipe_id`.
pub async fn linked_ids(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Vec<i64> {
    linked_recipes(pool, telegram_id, recipe_id)
        .await
        .iter()
        .map(|recipe| recipe.id)
        .collect()
}

/// The keyboard of a recipe's card, with a button per linked recipe; a scaled card's opens them
/// at its scale.
pub async fn card_keyboard(
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    scale: Option<Ratio>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> teloxide::types::InlineKeyboardMarkup {
    let linked = linked_recipes(pool, telegram_id, recipe_id).await;
    create_recipe_details_keyboard(recipe_id, &linked, scale, language_code, localization)
}

/// Shows card `card` of a recipe at `scale` (`None`: as saved), in place, with its buttons, and
/// records it. Returns `false` when the recipe is gone. A card Telegram did not edit (the same
/// scale tapped twice: "message is not modified") stays as it is.
#[allow(clippy::too_many_arguments)]
pub async fn show_scaled_in_place(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    card: teloxide::types::MessageId,
    recipe_id: i64,
    scale: Option<&Scale>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<bool> {
    let telegram_id = chat_id.0;
    let Some(text) = render(
        pool,
        telegram_id,
        recipe_id,
        scale,
        language_code,
        localization,
    )
    .await?
    else {
        return Ok(false);
    };
    let ratio = scale
        .map(|scale| scale.ratio)
        .filter(|ratio| !ratio.is_one());
    let keyboard = card_keyboard(
        pool,
        telegram_id,
        recipe_id,
        ratio,
        language_code,
        localization,
    )
    .await;
    match edit_html(bot, chat_id, card, text)
        .reply_markup(keyboard)
        .await
    {
        Ok(_) => {
            cleanup::set_card_scale(chat_id, card, ratio).await;
            Ok(true)
        }
        Err(e) => {
            debug!(error = %e, "Could not show a card scaled");
            Ok(true)
        }
    }
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
    show_scaled_card(
        bot,
        pool,
        chat_id,
        pressed,
        recipe_id,
        None,
        language_code,
        localization,
    )
    .await
}

/// [`show_card`], its quantities at `scale` (a linked recipe opened from a scaled card, a scale
/// typed by the user).
#[allow(clippy::too_many_arguments)]
pub async fn show_scaled_card(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    pressed: Option<teloxide::types::MessageId>,
    recipe_id: i64,
    scale: Option<&Scale>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let started = std::time::Instant::now();
    // Private chats only: the chat id is the user's id, the recipe's owner
    let telegram_id = chat_id.0;
    let ratio = scale
        .map(|scale| scale.ratio)
        .filter(|ratio| !ratio.is_one());
    let Some(card) = render(
        pool,
        telegram_id,
        recipe_id,
        scale,
        language_code,
        localization,
    )
    .await?
    else {
        let message = t_lang(localization, "recipe-not-found", language_code);
        let answer = cleanup::send_transient(bot, chat_id, message).await?;
        super::home::answer_at_bottom(bot, chat_id, answer.id, false, localization, language_code)
            .await;
        return Ok(());
    };
    let rendered = started.elapsed();
    // A second tap on the list's button reaches the card it became: nothing more to show
    if let Some(pressed) = pressed
        && cleanup::is_card(chat_id, pressed).await
    {
        return Ok(());
    }
    let keyboard = card_keyboard(
        pool,
        telegram_id,
        recipe_id,
        ratio,
        language_code,
        localization,
    )
    .await;

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
        Some(card_id) => {
            cleanup::list_became_card(bot, chat_id, card_id, recipe_id).await;
            cleanup::set_card_scale(chat_id, card_id, ratio).await;
        }
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
            cleanup::set_card_scale(chat_id, sent.id, ratio).await;
        }
    }
    crate::observability::card_shown(started.elapsed());
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
    // Each card at its own scale: read once per scale
    let mut rendered: Vec<(Option<Ratio>, Html)> = Vec::new();
    // Every consultation stays in the history: only the most recent ones are updated, so a recipe
    // opened many times does not make a rename slow or hit Telegram's rate limit
    for recorded in cards.into_iter().rev().take(MAX_CARDS_REFRESHED) {
        let card = match rendered.iter().find(|(scale, _)| *scale == recorded.scale) {
            Some((_, card)) => card.clone(),
            None => {
                let scale = recorded.scale.map(Scale::factor);
                match render(
                    pool,
                    telegram_id,
                    recipe_id,
                    scale.as_ref(),
                    language_code,
                    localization,
                )
                .await
                {
                    Ok(Some(card)) => {
                        rendered.push((recorded.scale, card.clone()));
                        card
                    }
                    Ok(None) => return false,
                    Err(e) => {
                        debug!(error = %e, "Could not read the recipe to update its cards");
                        return false;
                    }
                }
            }
        };
        let request = edit_html(bot, chat_id, recorded.id, card);
        let result = if recorded.active {
            request
                .reply_markup(
                    card_keyboard(
                        pool,
                        telegram_id,
                        recipe_id,
                        recorded.scale,
                        language_code,
                        localization,
                    )
                    .await,
                )
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
