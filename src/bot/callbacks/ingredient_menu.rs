//! Ingredient list buttons shared by the review of a new recipe and the editing of a saved one:
//! a button opens the ingredient's menu (quantity, name, whole line, delete) in place of the list
//! keyboard, and "back" puts the list keyboard again.

use anyhow::{Context, Result};
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

use crate::bot::ui_builder::{
    IngredientAction, create_ingredient_menu_keyboard, create_ingredient_review_keyboard,
};
use crate::errors::error_logging;
use crate::localization::{LocalizationManager, t_lang};
use crate::text_processing::MeasurementMatch;

/// Swaps the keyboard of the ingredient list message for `IngredientAction::Menu` and
/// `IngredientAction::Back`; other actions are not handled here.
pub async fn show_ingredient_keyboard(
    bot: &Bot,
    q: &CallbackQuery,
    ingredients: &[MeasurementMatch],
    action: IngredientAction,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let Some(message) = q.message.as_ref() else {
        return Ok(());
    };
    let keyboard = match action {
        IngredientAction::Menu(index) => match ingredients.get(index) {
            Some(ingredient) => {
                create_ingredient_menu_keyboard(ingredient, index, language_code, localization)
            }
            // A button of an older list: show the current list again
            None => create_ingredient_review_keyboard(ingredients, language_code, localization),
        },
        IngredientAction::Back => {
            create_ingredient_review_keyboard(ingredients, language_code, localization)
        }
        IngredientAction::Edit(..) | IngredientAction::Delete(_) => return Ok(()),
    };
    if let Err(error) = bot
        .edit_message_reply_markup(message.chat().id, message.id())
        .reply_markup(keyboard)
        .await
    {
        debug!(error = %error, "Could not swap the ingredient keyboard");
    }
    Ok(())
}

/// After an ingredient was deleted from a review or a saved recipe's edit, the list message
/// shows what is left (`list`, already rendered) with its buttons; when nothing is left, it says
/// so, with ➕ (`add_data`) above the single exit ❌ (U6.2). A failed edit is logged.
pub async fn show_list_after_delete(
    bot: &Bot,
    q: &CallbackQuery,
    ingredients: &[MeasurementMatch],
    list: crate::bot::html::Html,
    add_data: &str,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let message = q
        .message
        .as_ref()
        .context("Callback query should have a message")?;
    if ingredients.is_empty() {
        let empty_message = format!(
            "🗑️ {}\n\n{}\n\n{}",
            t_lang(localization, "review-title", language_code),
            t_lang(localization, "review-no-ingredients", language_code),
            t_lang(localization, "review-no-ingredients-help", language_code)
        );
        let keyboard = vec![
            vec![teloxide::types::InlineKeyboardButton::callback(
                t_lang(localization, "review-add-more", language_code),
                add_data,
            )],
            vec![teloxide::types::InlineKeyboardButton::callback(
                format!("❌ {}", t_lang(localization, "cancel", language_code)),
                "cancel_review",
            )],
        ];
        if let Err(e) = bot
            .edit_message_text(message.chat().id, message.id(), empty_message)
            .reply_markup(teloxide::types::InlineKeyboardMarkup::new(keyboard))
            .await
        {
            error_logging::log_internal_error(
                &e,
                "callback_handler",
                "Failed to edit message for empty ingredients",
                Some(crate::bot::telegram_id(q.from.id)),
            );
        }
    } else if let Err(e) = crate::bot::html::edit_html(bot, message.chat().id, message.id(), list)
        .reply_markup(create_ingredient_review_keyboard(
            ingredients,
            language_code,
            localization,
        ))
        .await
    {
        error_logging::log_internal_error(
            &e,
            "callback_handler",
            "Failed to edit message after ingredient deletion",
            Some(crate::bot::telegram_id(q.from.id)),
        );
    }
    Ok(())
}

/// The prompt of a line's edit (review or saved recipe): the title, the line as listed ("sel"
/// without quantity), and what to type for `field`.
pub fn edit_prompt_text(
    ingredient: &MeasurementMatch,
    field: crate::dialogue::IngredientField,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> String {
    format!(
        "✏️ {}\n\n{}: {}\n\n{}",
        t_lang(localization, "edit-ingredient-title", language_code),
        t_lang(localization, "edit-ingredient-current", language_code),
        crate::bot::ui_builder::ingredient_line(ingredient, &ingredient.ingredient_name),
        t_lang(
            localization,
            crate::bot::ui_builder::edit_instruction_key(field),
            language_code
        )
    )
}
