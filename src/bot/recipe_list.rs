//! The recipe list (`/liste`, the home message's "all my recipes", page buttons): one button per
//! recipe, newest first, [`RECIPES_PER_PAGE`] per page, labelled "name · date". A button carries
//! the recipe id (`recipe_instance:<id>`), so recipes with the same name are told apart by date.

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};

use crate::db::{get_user_recipes_newest_first, RecipeSummary};
use crate::localization::{t_lang, LocalizationManager};

use super::ui_builder::format_recipe_date;
use super::ui_components::{create_pagination_buttons, truncate_text};

/// Recipes shown per page.
pub const RECIPES_PER_PAGE: i64 = 8;

/// Longest recipe name on a button, in characters, before the date.
const NAME_MAX_CHARS: usize = 30;

/// "Tarte aux pommes · 27 oct. 2026"
pub fn recipe_button_label(
    recipe: &RecipeSummary,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> String {
    format!(
        "{} · {}",
        truncate_text(&recipe.name, NAME_MAX_CHARS),
        format_recipe_date(recipe.created_at, language_code, true, localization)
    )
}

/// One button per recipe of the page, then ⬅️ Page n of m ➡️ when there are several pages.
pub fn recipe_list_keyboard(
    recipes: &[RecipeSummary],
    page: usize,
    total: i64,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = recipes
        .iter()
        .map(|recipe| {
            vec![InlineKeyboardButton::callback(
                recipe_button_label(recipe, language_code, localization),
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

/// Sends page `page` of the user's recipes, or turns `edit` (the list being paged) into it. A
/// page past the end shows the last one. The bot only serves private chats, so the chat id is
/// the user's Telegram id, the recipes' owner.
pub async fn show_recipe_list(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    page: usize,
    edit: Option<MessageId>,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let telegram_id = chat_id.0;
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

    if recipes.is_empty() {
        let text = format!(
            "📚 {}\n\n{}",
            t_lang(localization, "no-recipes-found", language_code),
            t_lang(localization, "no-recipes-suggestion", language_code)
        );
        match edit {
            Some(id) => bot.edit_message_text(chat_id, id, text).await.map(|_| ())?,
            None => bot.send_message(chat_id, text).await.map(|_| ())?,
        }
        return Ok(());
    }

    let text = format!(
        "📚 {}\n\n{}",
        t_lang(localization, "your-recipes", language_code),
        t_lang(localization, "select-recipe", language_code)
    );
    let keyboard = recipe_list_keyboard(&recipes, page, total, language_code, localization);
    match edit {
        Some(id) => {
            bot.edit_message_text(chat_id, id, text)
                .reply_markup(keyboard)
                .await?;
        }
        None => {
            bot.send_message(chat_id, text)
                .reply_markup(keyboard)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn recipe(id: i64, name: &str) -> RecipeSummary {
        RecipeSummary {
            id,
            name: name.to_string(),
            created_at: chrono::Utc
                .with_ymd_and_hms(2026, 10, 27, 20, 36, 0)
                .single()
                .unwrap_or_default(),
        }
    }

    fn manager() -> Arc<LocalizationManager> {
        crate::localization::create_localization_manager().unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn each_recipe_has_its_own_button_with_name_and_date() {
        let localization = manager();
        let recipes = [recipe(12, "Recipe"), recipe(9, "Recipe")];
        let keyboard = recipe_list_keyboard(&recipes, 0, 2, Some("fr"), &localization);
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
                    "Recipe · 27 oct. 2026".to_string(),
                    "recipe_instance:12".to_string()
                ),
                (
                    "Recipe · 27 oct. 2026".to_string(),
                    "recipe_instance:9".to_string()
                ),
            ],
            "same names stay two entries, one page: no page row"
        );
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
    fn long_names_are_shortened_before_the_date() {
        let localization = manager();
        let long = recipe(1, &"Gâteau ".repeat(10));
        let label = recipe_button_label(&long, Some("en"), &localization);
        assert!(label.ends_with("· Oct 27, 2026"), "{label}");
        assert!(label.chars().count() <= NAME_MAX_CHARS + " · Oct 27, 2026".len());
    }
}
