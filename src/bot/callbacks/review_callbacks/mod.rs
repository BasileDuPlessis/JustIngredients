//! Review Ingredients Callbacks Module
//!
//! This module handles all callback queries that occur when the user is in the
//! ReviewIngredients dialogue state. This includes editing, deleting, confirming,
//! and canceling ingredient reviews.

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI components for the focused editing interface
use crate::bot::callbacks::ingredient_menu::show_ingredient_keyboard;
use crate::bot::ui_builder::{IngredientAction, parse_ingredient_action};

// Import HandlerContext
use crate::bot::HandlerContext;

// Import callback types module
use super::callback_types::ReviewIngredientsParams;

mod buttons;
mod end;

use buttons::*;
pub use end::*;

/// Handle callbacks when in ReviewIngredients dialogue state
pub async fn handle_review_ingredients_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let dialogue_state = dialogue.get().await?;
    if let Some(RecipeDialogueState::ReviewIngredients {
        recipe_name,
        mut ingredients,
        language_code: dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        open_menu: _,
    }) = dialogue_state
        && q.message.is_some()
    {
        let action = parse_ingredient_action(data);
        // What every line button gets; each one adds the ingredients it reads or changes
        let ctx = HandlerContext {
            bot,
            localization,
            language_code: dialogue_lang_code.as_deref(),
        };
        let params = || ReviewIngredientsParams {
            ctx: &ctx,
            q,
            data: Some(data),
            ingredients: None,
            ingredients_slice: None,
            recipe_name: &recipe_name,
            dialogue_lang_code: &dialogue_lang_code,
            message_id,
            extracted_text: &extracted_text,
            recipe_name_from_caption: Some(&recipe_name_from_caption),
            dialogue,
            pool: None,
        };
        if let Some(action @ (IngredientAction::Menu(_) | IngredientAction::Back)) = action {
            show_ingredient_keyboard(
                bot,
                q,
                &ingredients,
                action,
                dialogue_lang_code.as_deref(),
                localization,
            )
            .await?;
            // A typed answer applies to the ingredient whose menu is open
            let open_menu = match action {
                IngredientAction::Menu(index) if index < ingredients.len() => Some(index),
                _ => None,
            };
            dialogue
                .update(RecipeDialogueState::ReviewIngredients {
                    recipe_name,
                    ingredients,
                    language_code: dialogue_lang_code,
                    message_id,
                    extracted_text,
                    recipe_name_from_caption,
                    open_menu,
                })
                .await?;
        } else if let Some(IngredientAction::Edit(..)) = action {
            handle_edit_button(ReviewIngredientsParams {
                ingredients_slice: Some(&ingredients),
                ..params()
            })
            .await?;
        } else if data.starts_with("delete_") {
            handle_delete_button(ReviewIngredientsParams {
                ingredients: Some(&mut ingredients),
                ..params()
            })
            .await?;
        } else if data == "confirm" {
            handle_confirm_button(ReviewIngredientsParams {
                data: None,
                ingredients_slice: Some(&ingredients),
                pool: Some(&pool),
                ..params()
            })
            .await?;
        } else if data == "add_ingredient" || data == "add_more" {
            handle_add_button(ReviewIngredientsParams {
                data: None,
                ingredients_slice: Some(&ingredients),
                ..params()
            })
            .await?;
        } else if data == "cancel_review" {
            handle_cancel_review_button(
                bot,
                q,
                dialogue_lang_code.as_deref(),
                dialogue,
                localization,
                &pool,
            )
            .await?;
        }
        // workflow_* buttons are handled once, by the prefix handler of callback_handler
    }

    Ok(())
}
