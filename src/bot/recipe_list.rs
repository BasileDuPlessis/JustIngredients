//! The recipe list (`/liste`, the home message's "all my recipes", page buttons): one button per
//! recipe, newest first, [`RECIPES_PER_PAGE`] per page, labelled with the recipe name (and its
//! date only when the name appears twice on the page). A button carries the recipe id
//! (`recipe_instance:<id>`).

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};

use crate::db::{get_user_recipes_newest_first, RecipeSummary};
use crate::localization::{t_args_lang, t_lang, LocalizationManager};

use super::html::{edit_html, send_html, Html};
use super::ui_builder::format_recipe_date;
use super::ui_components::{create_pagination_buttons, truncate_text};

/// Recipes shown per page.
pub const RECIPES_PER_PAGE: i64 = 8;

/// Longest recipe name on a button, in characters, before the date.
const NAME_MAX_CHARS: usize = 30;

/// The labels of a page's buttons: the name alone, the day added for names that appear more
/// than once on the page ("Tarte · 27 oct. 2026"), and the time too when those were saved the
/// same day ("Tarte · 27 oct. 2026 21:36").
pub fn recipe_button_labels(
    recipes: &[RecipeSummary],
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Vec<String> {
    use chrono::Timelike;
    let names: Vec<String> = recipes
        .iter()
        .map(|recipe| truncate_text(&recipe.name, NAME_MAX_CHARS))
        .collect();
    let with_day: Vec<String> = recipes
        .iter()
        .zip(&names)
        .map(|(recipe, name)| {
            if names.iter().filter(|other| *other == name).count() > 1 {
                let day = format_recipe_date(recipe.created_at, language_code, true, localization);
                format!("{name} · {day}")
            } else {
                name.clone()
            }
        })
        .collect();
    recipes
        .iter()
        .zip(&names)
        .zip(&with_day)
        .map(|((recipe, name), label)| {
            if label != name && with_day.iter().filter(|other| *other == label).count() > 1 {
                let local = super::ui_builder::paris_time(recipe.created_at);
                format!("{label} {:02}:{:02}", local.hour(), local.minute())
            } else {
                label.clone()
            }
        })
        .collect()
}

/// One button per recipe of the page (see [`recipe_button_labels`]), then ⬅️ Page n of m ➡️
/// when there are several pages.
pub fn recipe_list_keyboard(
    recipes: &[RecipeSummary],
    page: usize,
    total: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> InlineKeyboardMarkup {
    let labels = recipe_button_labels(recipes, language_code, localization);
    let mut rows: Vec<Vec<InlineKeyboardButton>> = recipes
        .iter()
        .zip(labels)
        .map(|(recipe, label)| {
            vec![InlineKeyboardButton::callback(
                label,
                format!("recipe_instance:{}", recipe.id),
            )]
        })
        .collect();
    let total_pages = usize::try_from(total.max(0))
        .unwrap_or(0)
        .div_ceil(RECIPES_PER_PAGE as usize);
    if total_pages > 1 {
        rows.push(create_pagination_buttons(
            localization,
            page,
            total_pages,
            language_code,
        ));
    }
    InlineKeyboardMarkup::new(rows)
}

/// The list's text: its title, the number of recipes and how to add one. It is the last message
/// of the chat after every action (it replaced the home message in U5.2).
pub fn render_list_header(
    total: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Html {
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
    Html::bold(&format!(
        "📚 {}",
        t_lang(localization, "your-recipes", language_code)
    )) + Html::text(&format!(
        "\n{count}\n{}",
        t_lang(localization, "home-send-photo", language_code)
    ))
}

/// Sends page `page` of the user's recipes without notification and returns the new message,
/// or turns `edit` (the list being paged) into it. A page past the end shows the last one. The
/// bot only serves private chats, so the chat id is the user's Telegram id, the recipes' owner.
pub async fn show_recipe_list(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    page: usize,
    edit: Option<MessageId>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<Option<MessageId>> {
    let telegram_id = chat_id.0;
    // The query refuses offsets over 10,000: beyond, show the last page it can read
    let page = page.min(usize::try_from(10_000 / RECIPES_PER_PAGE).unwrap_or(0));
    let offset = |page: usize| i64::try_from(page).unwrap_or(0) * RECIPES_PER_PAGE;
    let (mut recipes, total) =
        get_user_recipes_newest_first(pool, telegram_id, RECIPES_PER_PAGE, offset(page)).await?;
    let mut page = page;
    if recipes.is_empty() && total > 0 {
        // Recipes were deleted since the page buttons were made
        page = usize::try_from((total - 1) / RECIPES_PER_PAGE).unwrap_or(0);
        (recipes, _) =
            get_user_recipes_newest_first(pool, telegram_id, RECIPES_PER_PAGE, offset(page))
                .await?;
    }

    let text = render_list_header(total, language_code, localization);
    // No recipe: the header alone tells how to start
    let keyboard = (!recipes.is_empty())
        .then(|| recipe_list_keyboard(&recipes, page, total, language_code, localization));
    match edit {
        Some(id) => {
            let request = edit_html(bot, chat_id, id, text);
            match keyboard {
                Some(keyboard) => request.reply_markup(keyboard).await?,
                None => request.await?,
            };
            Ok(None)
        }
        None => {
            let request = send_html(bot, chat_id, text).disable_notification(true);
            let sent = match keyboard {
                Some(keyboard) => request.reply_markup(keyboard).await?,
                None => request.await?,
            };
            Ok(Some(sent.id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn recipe(id: i64, name: &str) -> RecipeSummary {
        recipe_at(id, name, 20)
    }

    fn recipe_at(id: i64, name: &str, hour: u32) -> RecipeSummary {
        recipe_on(id, name, 27, hour)
    }

    fn recipe_on(id: i64, name: &str, day: u32, hour: u32) -> RecipeSummary {
        RecipeSummary {
            id,
            name: name.to_string(),
            created_at: chrono::Utc
                .with_ymd_and_hms(2026, 10, day, hour, 36, 0)
                .single()
                .unwrap_or_default(),
        }
    }

    fn manager() -> Arc<LocalizationManager> {
        crate::localization::create_localization_manager().unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn buttons_show_the_name_and_tell_homonyms_apart() {
        let localization = manager();
        let recipes = [
            recipe_at(12, "Recipe", 20),
            recipe_at(9, "Recipe", 7),
            recipe(5, "Tarte"),
        ];
        let keyboard = recipe_list_keyboard(&recipes, 0, 3, Some("fr"), &localization);
        let buttons: Vec<(String, String)> = keyboard
            .inline_keyboard
            .iter()
            .flatten()
            .map(|button| {
                let data = match &button.kind {
                    teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => data.clone(),
                    _ => String::new(),
                };
                (button.text.clone(), data)
            })
            .collect();
        assert_eq!(
            buttons,
            vec![
                (
                    "Recipe · 27 oct. 2026 21:36".to_string(),
                    "recipe_instance:12".to_string()
                ),
                (
                    "Recipe · 27 oct. 2026 08:36".to_string(),
                    "recipe_instance:9".to_string()
                ),
                ("Tarte".to_string(), "recipe_instance:5".to_string()),
            ],
            "a unique name alone; same name same day: day and time; one page: no page row"
        );
    }

    #[test]
    fn the_list_header_counts_recipes_and_tells_how_to_add_one() {
        let localization = manager();
        let header = render_list_header(12, Some("fr"), &localization);
        assert!(header.as_str().starts_with("<b>📚 "), "{}", header.as_str());
        assert!(
            header.as_str().contains("12 recettes"),
            "{}",
            header.as_str()
        );
        assert!(header.as_str().contains("📸"), "{}", header.as_str());
        let empty = render_list_header(0, Some("en"), &localization);
        assert!(!empty.as_str().contains('0'), "{}", empty.as_str());
    }

    #[test]
    fn several_pages_get_a_page_row() {
        let localization = manager();
        let recipes: Vec<RecipeSummary> = (1..=8).map(|id| recipe(id, "Tarte")).collect();
        let keyboard = recipe_list_keyboard(&recipes, 1, 20, Some("en"), &localization);
        assert_eq!(keyboard.inline_keyboard.len(), 9);
        let page_row: Vec<&str> = keyboard.inline_keyboard[8]
            .iter()
            .map(|button| button.text.as_str())
            .collect();
        assert!(
            page_row
                .iter()
                .any(|text| text.contains("2") && text.contains("3")),
            "{page_row:?}"
        );
    }

    #[test]
    fn homonyms_of_different_days_get_the_day_only() {
        let localization = manager();
        let recipes = [recipe_on(2, "Tarte", 28, 9), recipe_on(1, "Tarte", 27, 9)];
        assert_eq!(
            recipe_button_labels(&recipes, Some("en"), &localization),
            vec!["Tarte · Oct 28, 2026", "Tarte · Oct 27, 2026"]
        );
    }

    #[test]
    fn long_names_are_shortened() {
        let localization = manager();
        let labels = recipe_button_labels(&[recipe(1, &"Gâteau ".repeat(10))], None, &localization);
        assert!(labels[0].chars().count() <= NAME_MAX_CHARS, "{}", labels[0]);
        assert!(labels[0].ends_with("..."));
    }
}
