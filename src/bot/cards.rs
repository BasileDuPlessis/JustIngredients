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

/// Opens a recipe: its earlier card goes (one card per recipe), the card is sent and the recipe
/// list moves below it. A recipe that is gone answers `recipe-not-found` until the next action.
pub async fn show_card(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    telegram_id: i64,
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let Some(card) = render(pool, telegram_id, recipe_id, language_code, localization).await?
    else {
        let message = t_lang(localization, "recipe-not-found", language_code);
        cleanup::send_transient(bot, chat_id, message).await?;
        return Ok(());
    };
    cleanup::remove_recipe_cards(bot, chat_id, recipe_id, None).await;
    let keyboard = create_recipe_details_keyboard(recipe_id, language_code, localization);
    let sent = send_html(bot, chat_id, card).reply_markup(keyboard).await?;
    cleanup::record_card(chat_id, sent.id, recipe_id).await;
    super::home::move_list_down(bot, pool, chat_id, localization, language_code).await;
    Ok(())
}

/// Updates every recorded card of a recipe in place, after a rename or an edit. Returns whether
/// there was one (cards of versions before U5.4 are not recorded).
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
    for (id, _) in cards {
        let keyboard = create_recipe_details_keyboard(recipe_id, language_code, localization);
        if let Err(e) = edit_html(bot, chat_id, id, card.clone())
            .reply_markup(keyboard)
            .await
        {
            debug!(error = %e, "Could not update a recipe card");
        }
    }
    true
}
