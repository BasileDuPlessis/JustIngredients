//! The buttons on one line of the list: edit, delete, add.

use anyhow::{Context, Result};

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{IngredientField, RecipeDialogueState};

// Import UI components for the focused editing interface
use crate::bot::ui_builder::{IngredientAction, parse_ingredient_action, render_review};
use crate::bot::ui_components::create_ingredient_editing_keyboard;

// Import callback types module
use crate::bot::callbacks::callback_types::ReviewIngredientsParams;

// Import dialogue manager functions

use super::*;

/// Handle edit button in review ingredients state: the review list is replaced by a prompt for
/// the chosen field of the ingredient (see [`open_line_prompt`]).
pub(super) async fn handle_edit_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ingredients = params
        .ingredients_slice
        .context("Ingredients slice should be provided for edit callback")?;
    let Some(IngredientAction::Edit(field, index)) = params.data.and_then(parse_ingredient_action)
    else {
        return Ok(());
    };
    let Some(ingredient) = ingredients.get(index) else {
        return Ok(());
    };
    crate::observability::action(crate::observability::Action::IngredientEdited);

    let prompt = crate::bot::callbacks::ingredient_menu::edit_prompt_text(
        ingredient,
        field,
        params.dialogue_lang_code.as_deref(),
        params.ctx.localization,
    );
    open_line_prompt(params, index, field, prompt).await
}

/// ➕ Add ingredient (and the "add more" button of an emptied list): the typed line becomes a
/// new last ingredient, through the same prompt as an edit.
pub(super) async fn handle_add_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ingredients = params
        .ingredients_slice
        .context("Ingredients slice should be provided for add callback")?;
    let lang = params.dialogue_lang_code.as_deref();
    let prompt = format!(
        "➕ {}\n\n{}",
        t_lang(params.ctx.localization, "add-ingredient", lang),
        t_lang(params.ctx.localization, "add-ingredient-prompt", lang)
    );
    let new_index = ingredients.len();
    open_line_prompt(params, new_index, IngredientField::Line, prompt).await
}

/// Focused editing: the review list is replaced by `prompt` with a single Cancel button, and the
/// typed answer replaces `field` of ingredient `index` (a new ingredient when `index` is the
/// list's length). After the answer or Cancel the list comes back in the same message.
async fn open_line_prompt(
    params: ReviewIngredientsParams<'_>,
    index: usize,
    field: IngredientField,
    prompt: String,
) -> Result<()> {
    let ReviewIngredientsParams {
        ctx,
        q,
        ingredients_slice,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        ..
    } = params;
    let ingredients =
        ingredients_slice.context("Ingredients slice should be provided for edit callback")?;
    let Some(chat_id) = q.message.as_ref().map(|message| message.chat().id) else {
        return Ok(());
    };

    let keyboard =
        create_ingredient_editing_keyboard(dialogue_lang_code.as_deref(), ctx.localization);

    // Replace the original recipe display message with the focused prompt
    let edited = match message_id {
        Some(id) => ctx
            .bot
            .edit_message_text(chat_id, teloxide::types::MessageId(id), prompt.clone())
            .reply_markup(keyboard.clone())
            .await
            .map_err(|e| {
                error_logging::log_internal_error(
                    &e,
                    "open_line_prompt",
                    "Failed to replace recipe display with editing prompt",
                    Some(crate::bot::telegram_id(q.from.id)),
                );
            })
            .ok(),
        None => None,
    };
    // Fallback: send a new message if the list could not be edited
    let prompt_message = match edited {
        Some(message) => message,
        None => {
            ctx.bot
                .send_message(chat_id, prompt)
                .reply_markup(keyboard)
                .await?
        }
    };

    // Transition to editing state with updated message tracking
    dialogue
        .update(RecipeDialogueState::EditingIngredient {
            recipe_name: recipe_name.to_string(),
            ingredients: ingredients.to_vec(),
            editing_index: index,
            field,
            language_code: dialogue_lang_code.clone(),
            message_id: Some(prompt_message.id.0 as i32), // Track the editing prompt message
            original_message_id: message_id, // Original recipe display message to replace
            extracted_text: extracted_text.to_string(),
            recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info
        })
        .await?;
    Ok(())
}

/// Handle delete button in review ingredients state
pub(super) async fn handle_delete_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ReviewIngredientsParams {
        ctx,
        q,
        data,
        ingredients,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        ..
    } = params;

    let data = data.unwrap_or("");
    let ingredients = ingredients.context("Ingredients should be provided for delete callback")?;
    let index: usize = data
        .strip_prefix("delete_")
        .context("Delete callback data should start with 'delete_'")?
        .parse()
        .unwrap_or(0);

    if index < ingredients.len() {
        crate::observability::action(crate::observability::Action::IngredientDeleted);

        ingredients.remove(index);

        // The list shows what is left, or that nothing is left
        crate::bot::callbacks::ingredient_menu::show_list_after_delete(
            ctx.bot,
            q,
            ingredients,
            render_review(ingredients, dialogue_lang_code.as_deref(), ctx.localization),
            "add_more",
            dialogue_lang_code.as_deref(),
            ctx.localization,
        )
        .await?;

        // Update dialogue state with modified ingredients
        match dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name: recipe_name.to_string(),
                ingredients: ingredients.clone(),
                language_code: dialogue_lang_code.clone(),
                message_id,
                extracted_text: extracted_text.to_string(),
                recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info
                open_menu: None,
            })
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "callback_handler",
                    "Failed to update dialogue state after deletion",
                    Some(crate::bot::telegram_id(q.from.id)),
                );
            }
        }
    }
    Ok(())
}
