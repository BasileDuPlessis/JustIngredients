//! Home message: the state every finished action returns to (save, cancel, delete, rename,
//! edit of a saved recipe). It gives the number of recipes, the latest ones as buttons, the
//! commands and a hint to send a photo. Sending it first deletes what the action left behind.

use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use tracing::warn;

use super::cleanup;
use super::commands::command_lines;
use super::html::{send_html, Html};
use super::ui_components::truncate_text;
use crate::db::{get_user_recipes_newest_first, RecipeSummary};
use crate::localization::{t_args_lang, t_lang, LocalizationManager};

/// Number of latest recipes offered as buttons.
const LATEST_RECIPES: i64 = 3;

/// Longest recipe name shown on a button, in characters.
const BUTTON_NAME_MAX_CHARS: usize = 40;

/// Text and keyboard of the home message; no keyboard when the user has no recipe.
///
/// `total` counts all the user's recipes, `latest` holds the newest ones (newest first).
pub fn render_home(
    total: i64,
    latest: &[RecipeSummary],
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> (Html, Option<InlineKeyboardMarkup>) {
    let count = match total {
        0 => t_lang(localization, "home-no-recipes", language_code),
        1 => t_lang(localization, "home-one-recipe", language_code),
        n => t_args_lang(
            localization,
            "home-recipe-count",
            &[("count", &n.to_string())],
            language_code,
        ),
    };
    let text = Html::bold(&t_lang(localization, "home-title", language_code))
        + Html::text(&format!(
            "\n{}\n{}\n\n{}",
            count,
            t_lang(localization, "home-send-photo", language_code),
            command_lines(localization, language_code)
        ));

    let mut rows: Vec<Vec<InlineKeyboardButton>> = latest
        .iter()
        .map(|recipe| {
            vec![InlineKeyboardButton::callback(
                format!("📖 {}", truncate_text(&recipe.name, BUTTON_NAME_MAX_CHARS)),
                format!("recipe_instance:{}", recipe.id),
            )]
        })
        .collect();
    if total > latest.len() as i64 {
        rows.push(vec![InlineKeyboardButton::callback(
            t_lang(localization, "home-all-recipes", language_code),
            "workflow_list_recipes",
        )]);
    }
    let keyboard = (!rows.is_empty()).then(|| InlineKeyboardMarkup::new(rows));
    (text, keyboard)
}

/// Ends an action in a private chat (the chat id is the recipe owner): deletes the messages the
/// action left behind and the previous home message (see `cleanup`), then sends the home message.
///
/// Never fails: the action that ends here has already succeeded, so errors are only logged.
pub async fn send_home(
    bot: &Bot,
    pool: &sqlx::PgPool,
    chat_id: ChatId,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    cleanup::delete_tracked(bot, chat_id).await;
    let (latest, total) =
        match get_user_recipes_newest_first(pool, chat_id.0, LATEST_RECIPES, 0).await {
            Ok(recipes) => recipes,
            Err(e) => {
                warn!(error = %e, "Failed to read the recipes of the home message");
                return;
            }
        };
    let (text, keyboard) = render_home(total, &latest, localization, language_code);
    let request = send_html(bot, chat_id, text);
    let result = match keyboard {
        Some(keyboard) => request.reply_markup(keyboard).await,
        None => request.await,
    };
    match result {
        Ok(sent) => cleanup::set_home(chat_id, sent.id),
        Err(e) => warn!(error = %e, "Failed to send the home message"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::create_localization_manager;
    use teloxide::types::InlineKeyboardButtonKind;

    fn recipe(id: i64, name: &str) -> RecipeSummary {
        RecipeSummary {
            id,
            name: name.to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Button labels and callback data, row by row.
    fn buttons(keyboard: &InlineKeyboardMarkup) -> Vec<(String, String)> {
        keyboard
            .inline_keyboard
            .iter()
            .flatten()
            .map(|b| match &b.kind {
                InlineKeyboardButtonKind::CallbackData(data) => (b.text.clone(), data.clone()),
                other => panic!("unexpected button kind {other:?}"),
            })
            .collect()
    }

    #[test]
    fn no_recipe_gives_the_hint_and_no_buttons() {
        let localization = create_localization_manager().expect("localization");
        let (html, keyboard) = render_home(0, &[], &localization, Some("fr"));
        let text = html.as_str();
        assert!(text.contains("pas encore de recette"), "{text}");
        assert!(text.contains("photo"), "{text}");
        assert!(
            text.contains("/liste - ") && text.contains("/aide - "),
            "{text}"
        );
        assert!(keyboard.is_none());
    }

    #[test]
    fn one_recipe_is_a_button_without_the_all_recipes_button() {
        let localization = create_localization_manager().expect("localization");
        let (html, keyboard) = render_home(1, &[recipe(7, "Tarte")], &localization, Some("en"));
        let text = html.as_str();
        assert!(text.contains("1 recipe."), "{text}");
        assert!(
            text.contains("/list - ") && text.contains("/help - "),
            "{text}"
        );
        let keyboard = keyboard.expect("keyboard");
        assert_eq!(
            buttons(&keyboard),
            vec![("📖 Tarte".to_string(), "recipe_instance:7".to_string())]
        );
    }

    #[test]
    fn many_recipes_show_the_latest_and_the_all_recipes_button() {
        let localization = create_localization_manager().expect("localization");
        let latest = [
            recipe(12, "Crêpes"),
            recipe(11, "Tarte"),
            recipe(10, "Pizza"),
        ];
        for (language, count, all) in [
            ("fr", "12", "Toutes mes recettes"),
            ("en", "12", "All my recipes"),
        ] {
            let (html, keyboard) = render_home(12, &latest, &localization, Some(language));
            let text = html.as_str();
            assert!(text.contains(count), "{language}: {text}");
            assert!(!text.contains("Missing translation"), "{language}: {text}");
            let buttons = buttons(&keyboard.expect("keyboard"));
            assert_eq!(buttons.len(), 4, "{language}");
            assert_eq!(buttons[0].1, "recipe_instance:12");
            assert_eq!(buttons[2].1, "recipe_instance:10");
            assert!(buttons[3].0.contains(all), "{language}: {:?}", buttons[3]);
            assert_eq!(buttons[3].1, "workflow_list_recipes");
        }
    }

    #[test]
    fn long_names_are_shortened_and_callback_data_fits_telegram() {
        let localization = create_localization_manager().expect("localization");
        let name = "Gâteau ".repeat(40);
        let (_, keyboard) = render_home(1, &[recipe(i64::MAX, &name)], &localization, Some("fr"));
        let (label, data) = buttons(&keyboard.expect("keyboard")).remove(0);
        assert!(
            label.chars().count() <= BUTTON_NAME_MAX_CHARS + 2,
            "{label}"
        );
        assert!(data.len() <= 64, "{data}");
    }
}
