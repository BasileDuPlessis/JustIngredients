//! Editing Callbacks module for handling EditingSavedIngredients dialogue state

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import HandlerContext
use crate::bot::callbacks::ingredient_menu::show_ingredient_keyboard;
use crate::bot::ui_builder::{parse_ingredient_action, IngredientAction};
use crate::bot::HandlerContext;

// Import callback types module
use super::callback_types::SavedIngredientsParams;

mod buttons;
mod end;

use buttons::*;
pub use end::*;

/// Handle callbacks when in EditingSavedIngredients dialogue state
pub async fn handle_editing_saved_ingredients_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let dialogue_state = dialogue.get().await?;
    if let Some(RecipeDialogueState::EditingSavedIngredients {
        recipe_id,
        original_ingredients,
        mut current_matches,
        language_code,
        message_id,
        open_menu: _,
    }) = dialogue_state
    {
        if q.message.is_some() {
            let action = parse_ingredient_action(data);
            // What every line button gets; each one adds the ingredients it reads or changes
            let ctx = HandlerContext {
                bot,
                localization,
                language_code: language_code.as_deref(),
            };
            let params = || SavedIngredientsParams {
                ctx: &ctx,
                q,
                data: Some(data),
                current_matches: None,
                current_matches_slice: None,
                recipe_id,
                original_ingredients: &original_ingredients,
                language_code: &language_code,
                message_id,
                dialogue,
                pool: None,
            };
            if let Some(action @ (IngredientAction::Menu(_) | IngredientAction::Back)) = action {
                show_ingredient_keyboard(
                    bot,
                    q,
                    &current_matches,
                    action,
                    language_code.as_deref(),
                    localization,
                )
                .await?;
                // A typed answer applies to the ingredient whose menu is open
                let open_menu = match action {
                    IngredientAction::Menu(index) if index < current_matches.len() => Some(index),
                    _ => None,
                };
                dialogue
                    .update(RecipeDialogueState::EditingSavedIngredients {
                        recipe_id,
                        original_ingredients,
                        current_matches,
                        language_code,
                        message_id,
                        open_menu,
                    })
                    .await?;
            } else if let Some(IngredientAction::Edit(..)) = action {
                handle_edit_saved_ingredient_button(SavedIngredientsParams {
                    current_matches_slice: Some(&current_matches),
                    ..params()
                })
                .await?;
            } else if data.starts_with("delete_") {
                handle_delete_saved_ingredient_button(SavedIngredientsParams {
                    current_matches: Some(&mut current_matches),
                    ..params()
                })
                .await?;
            } else if data == "confirm" {
                handle_confirm_saved_ingredients_button(SavedIngredientsParams {
                    data: None,
                    current_matches_slice: Some(&current_matches),
                    pool: Some(&pool),
                    ..params()
                })
                .await?;
            } else if data == "add_ingredient" || data == "add_more" {
                handle_add_ingredient_button(
                    bot,
                    q,
                    language_code.as_deref(),
                    dialogue,
                    localization,
                )
                .await?;
            } else if data == "cancel_review" {
                handle_cancel_saved_ingredients_button(
                    bot,
                    q,
                    language_code.as_deref(),
                    dialogue,
                    localization,
                    pool.clone(),
                )
                .await?;
            }
        }
    }

    Ok(())
}
