//! The text typed while an action runs, passed to that action's handler: a recipe name after
//! ✅, an answer in a review, a line being edited, a new name, an added ingredient, a saved
//! recipe's edit. One arm per dialogue state.

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import dialogue manager functions
use super::dialogue_manager::{
    handle_add_ingredient_input, handle_ingredient_edit_input, handle_ingredient_review_input,
    handle_recipe_name_after_confirm_input, handle_recipe_rename_input,
    handle_saved_ingredient_edit_input, AddIngredientInputParams, DialogueContext,
    IngredientEditInputParams, IngredientReviewInputParams, RecipeNameAfterConfirmInputParams,
    RecipeRenameInputParams, SavedIngredientEditInputParams,
};

// Import HandlerContext
use super::HandlerContext;

/// A text typed during an action (a name, an answer to a prompt, an edit): passed to that
/// action's handler. `false` when no action runs (the text is free text).
pub(super) async fn answer_in_action(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
    dialogue_state: Option<RecipeDialogueState>,
) -> Result<bool> {
    let Some(text) = msg.text() else {
        return Ok(false);
    };
    // The same for every action; each arm passes it on
    let dialogue_ctx = DialogueContext {
        bot,
        msg,
        dialogue,
        localization,
    };
    match dialogue_state {
        Some(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
            ingredients,
            language_code: dialogue_lang_code,
            extracted_text,
            recipe_name_from_caption: _,
            message_id,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle recipe name input after ingredient confirmation
            return handle_recipe_name_after_confirm_input(
                dialogue_ctx,
                RecipeNameAfterConfirmInputParams {
                    pool,
                    recipe_name_input: text,
                    ingredients,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                    extracted_text,
                    message_id,
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::ReviewIngredients {
            recipe_name,
            ingredients,
            language_code: dialogue_lang_code,
            message_id: review_message_id,
            extracted_text,
            recipe_name_from_caption,
            open_menu,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle ingredient review commands
            return handle_ingredient_review_input(
                dialogue_ctx,
                IngredientReviewInputParams {
                    pool,
                    review_input: text,
                    recipe_name_from_caption,
                    ingredients,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                    extracted_text,
                    review_message_id,
                    recipe_name,
                    open_menu,
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::EditingIngredient {
            recipe_name,
            ingredients,
            editing_index,
            field,
            language_code: dialogue_lang_code,
            message_id,
            original_message_id: _original_message_id,
            extracted_text,
            recipe_name_from_caption,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle ingredient edit input
            return handle_ingredient_edit_input(
                dialogue_ctx,
                IngredientEditInputParams {
                    edit_input: text,
                    recipe_name,
                    ingredients,
                    editing_index,
                    field,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                    message_id,
                    extracted_text,
                    user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
                    recipe_name_from_caption,
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::RenamingRecipe {
            recipe_id,
            current_name,
            language_code: dialogue_lang_code,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle recipe rename input
            return handle_recipe_rename_input(
                dialogue_ctx,
                RecipeRenameInputParams {
                    pool: &pool,
                    new_name_input: text,
                    recipe_id,
                    current_name,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::AddingIngredientToSavedRecipe {
            recipe_id,
            original_ingredients,
            current_matches,
            language_code: dialogue_lang_code,
            message_id,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle adding new ingredient input for saved recipes
            return handle_add_ingredient_input(
                dialogue_ctx,
                AddIngredientInputParams {
                    pool: &pool,
                    add_input: text,
                    recipe_id,
                    original_ingredients: &original_ingredients,
                    current_matches: &current_matches,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                    message_id,
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::EditingSavedIngredient {
            recipe_id,
            original_ingredients,
            current_matches,
            editing_index,
            field,
            language_code: dialogue_lang_code,
            message_id,
            original_message_id,
        }) => {
            // Use dialogue language code if available, otherwise fall back to message language
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

            // Handle editing individual ingredient input for saved recipes
            return handle_saved_ingredient_edit_input(
                dialogue_ctx,
                SavedIngredientEditInputParams {
                    pool: &pool,
                    edit_input: text,
                    recipe_id,
                    original_ingredients: &original_ingredients,
                    current_matches: &current_matches,
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: effective_language_code,
                    },
                    message_id,
                    editing_index,
                    field,
                    original_message_id,
                    user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
                },
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::EditingSavedIngredients {
            language_code: dialogue_lang_code,
            ..
        }) if super::dialogue_manager::is_cancellation_command(&text.trim().to_lowercase()) => {
            // Exactly what ❌ does: back to the recipe card as saved
            let effective_language_code = dialogue_lang_code.as_deref().or(language_code);
            return super::callbacks::editing_callbacks::cancel_saved_editing(
                bot,
                localization,
                &pool,
                &dialogue_ctx.dialogue,
                msg.chat.id,
                msg.chat.id.0,
                effective_language_code,
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
            // Other text: an answer to the open ingredient menu, or not understood
            return super::dialogue_manager::handle_saved_editing_text(
                bot,
                msg,
                &dialogue_ctx.dialogue,
                localization,
                language_code,
                text,
            )
            .await
            .map(|()| true);
        }
        Some(RecipeDialogueState::Start) | None => {}
    }
    Ok(false)
}
