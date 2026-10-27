//! Ingredient list buttons shared by the review of a new recipe and the editing of a saved one:
//! a button opens the ingredient's menu (quantity, name, whole line, delete) in place of the list
//! keyboard, and "back" puts the list keyboard again.

use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

use crate::bot::ui_builder::{
    create_ingredient_menu_keyboard, create_ingredient_review_keyboard, IngredientAction,
};
use crate::localization::LocalizationManager;
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
