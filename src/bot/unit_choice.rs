//! The `/unites` chooser (UX plan U8): how the user's recipe cards show measures. The choice is
//! saved for the user; the chooser becomes a history line, and the card that was at the bottom
//! comes back in the new view (or the recipe list).

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage};
use tracing::warn;

use crate::localization::{t_args_lang, t_lang, LocalizationManager};
use crate::unit_view::UnitView;

/// Callback data of a view button: `unit_view:<view>:<recipe to show again, 0 for none>`.
const CALLBACK_PREFIX: &str = "unit_view:";

/// The locale key naming a view.
pub fn view_key(view: UnitView) -> &'static str {
    match view {
        UnitView::Recipe => "unit-view-recipe",
        UnitView::Weight => "unit-view-weight",
        UnitView::French => "unit-view-french",
        UnitView::Us => "unit-view-us",
    }
}

/// The chooser: title and explanation, one button per view (the current one ticked), then
/// "📚 Liste des recettes". `recipe` is the card to show again after the choice.
pub fn render_chooser(
    current: UnitView,
    recipe: Option<i64>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> (String, InlineKeyboardMarkup) {
    let text = format!(
        "{}\n{}",
        t_lang(localization, "unit-view-title", language_code),
        t_lang(localization, "unit-view-explain", language_code)
    );
    let mut rows: Vec<Vec<InlineKeyboardButton>> = UnitView::ALL
        .into_iter()
        .map(|view| {
            let label = t_lang(localization, view_key(view), language_code);
            let label = if view == current {
                format!("✓ {label}")
            } else {
                label
            };
            vec![InlineKeyboardButton::callback(
                label,
                format!("{CALLBACK_PREFIX}{}:{}", view.as_str(), recipe.unwrap_or(0)),
            )]
        })
        .collect();
    rows.push(vec![super::ui_builder::recipes_list_button(
        language_code,
        localization,
    )]);
    (text, InlineKeyboardMarkup::new(rows))
}

/// Whether callback data is a view button.
pub fn is_choice(data: &str) -> bool {
    data.starts_with(CALLBACK_PREFIX)
}

/// "unit_view:weight:12" → the view and the recipe to show again (none for 0).
pub fn parse_choice(data: &str) -> Option<(UnitView, Option<i64>)> {
    let (view, recipe) = data.strip_prefix(CALLBACK_PREFIX)?.split_once(':')?;
    let recipe: i64 = recipe.parse().ok()?;
    Some((UnitView::parse(view)?, (recipe > 0).then_some(recipe)))
}

/// `/unites`: the chooser at the bottom, in place of the list or of the card shown (which comes
/// back after the choice).
pub async fn show_chooser(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    // Private chats only: the chat id is the user's id
    let current = crate::db::unit_view(pool, chat_id.0)
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "Could not read the unit view");
            UnitView::default()
        });
    let recipe = super::cleanup::recipe_at_bottom(chat_id).await;
    let (text, keyboard) = render_chooser(current, recipe, language_code, localization);
    let sent = bot
        .send_message(chat_id, text)
        .reply_markup(keyboard)
        .await?;
    super::home::keep_at_bottom(bot, chat_id, sent.id).await;
    // Recorded again with its recipe, which a second `/unites` brings back too
    super::cleanup::track_chooser(chat_id, sent.id, recipe).await;
    Ok(())
}

/// A view chosen: saved, the chooser becomes the line "⚖️ Unités : …" in the history, and the
/// card that was at the bottom before `/unites` comes back in that view, or the list.
pub async fn handle_choice(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: &PgPool,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let MaybeInaccessibleMessage::Regular(msg) = msg else {
        return Ok(());
    };
    let Some((view, recipe)) = parse_choice(data) else {
        return Ok(());
    };
    let chat_id = msg.chat.id;
    // A chooser already used (a second tap) or cleared by a later action does nothing
    if super::cleanup::is_recorded(chat_id, msg.id).await == Some(false) {
        return Ok(());
    }
    crate::db::set_unit_view(pool, chat_id.0, language_code, view).await?;
    let line = t_args_lang(
        localization,
        "unit-view-chosen",
        &[("view", &t_lang(localization, view_key(view), language_code))],
        language_code,
    );
    if let Err(e) = bot.edit_message_text(chat_id, msg.id, line).await {
        warn!(error = %e, "Could not turn the unit chooser into its line");
        // Its buttons must not stay usable in the history
        if let Err(e) = bot.edit_message_reply_markup(chat_id, msg.id).await {
            warn!(error = %e, "Could not remove the buttons of the unit chooser");
        }
    }
    // It stays in the history
    super::cleanup::untrack(chat_id, msg.id).await;
    match recipe {
        Some(recipe_id) => {
            super::cards::show_card(
                bot,
                pool,
                chat_id,
                None,
                recipe_id,
                language_code,
                localization,
            )
            .await?
        }
        None => {
            super::home::send_home(bot, pool, chat_id, localization, language_code).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> Arc<LocalizationManager> {
        crate::localization::create_localization_manager().unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_chooser_ticks_the_current_view_and_ends_with_the_list() {
        let localization = manager();
        let (text, keyboard) =
            render_chooser(UnitView::French, Some(12), Some("fr"), &localization);
        assert!(text.starts_with("⚖️ Affichage des unités"), "{text}");
        let buttons: Vec<(String, String)> = keyboard
            .inline_keyboard
            .iter()
            .map(|row| {
                let button = &row[0];
                let data = match &button.kind {
                    teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => data.clone(),
                    other => format!("{other:?}"),
                };
                (button.text.clone(), data)
            })
            .collect();
        assert_eq!(
            buttons,
            [
                ("Comme dans la recette", "unit_view:recipe:12"),
                ("Tout en poids (g)", "unit_view:weight:12"),
                ("✓ Unités françaises (g, cl)", "unit_view:french:12"),
                ("Unités américaines (cups, oz)", "unit_view:us:12"),
                ("📚 Liste des recettes", "show_list"),
            ]
            .map(|(text, data)| (text.to_string(), data.to_string()))
        );
        let (_, keyboard) = render_chooser(UnitView::Us, None, Some("en"), &localization);
        assert!(keyboard.inline_keyboard[3][0]
            .text
            .starts_with("✓ US units"));
    }

    #[test]
    fn choices_parse_back_and_stay_short() {
        for view in UnitView::ALL {
            let data = format!("{CALLBACK_PREFIX}{}:{}", view.as_str(), i64::MAX);
            assert!(data.len() <= 64, "{data}");
            assert_eq!(parse_choice(&data), Some((view, Some(i64::MAX))));
        }
        assert_eq!(
            parse_choice("unit_view:weight:0"),
            Some((UnitView::Weight, None))
        );
        assert_eq!(parse_choice("unit_view:metric:0"), None);
        assert_eq!(parse_choice("unit_view:weight"), None);
        assert!(is_choice("unit_view:us:3") && !is_choice("show_list"));
    }
}
