//! The buttons on one line of the list: edit, delete, add.

use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use std::sync::Arc;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import HandlerContext
use crate::bot::cleanup::send_transient;
use crate::bot::ui_builder::{edit_instruction_key, parse_ingredient_action, IngredientAction};

// Import callback types module
use crate::bot::callbacks::callback_types::SavedIngredientsParams;

use super::*;

/// Handle edit button for saved ingredients
///
/// This function implements the same "focused editing interface" approach as the initial recipe editing:
/// - Replaces the full recipe display with a clean editing prompt to eliminate inactive button confusion
/// - Only shows a cancel button during editing for clarity
/// - Tracks the original message ID to restore the recipe display after editing/canceling
/// - Provides consistent UX across both initial recipe creation and saved recipe editing workflows
pub(super) async fn handle_edit_saved_ingredient_button(
    params: SavedIngredientsParams<'_>,
) -> Result<()> {
    let SavedIngredientsParams {
        ctx,
        q,
        data,
        current_matches_slice,
        recipe_id,
        original_ingredients,
        language_code,
        message_id,
        dialogue,
        ..
    } = params;

    let current_matches = current_matches_slice
        .context("Current matches slice should be provided for edit callback")?;
    let Some(IngredientAction::Edit(field, index)) = data.and_then(parse_ingredient_action) else {
        return Ok(());
    };
    if index < current_matches.len() {
        crate::observability::action(crate::observability::Action::IngredientEdited);

        let ingredient = &current_matches[index];
        let edit_prompt = format!(
            "✏️ {}\n\n{}: {}\n\n{}",
            t_lang(
                ctx.localization,
                "edit-ingredient-title",
                language_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "edit-ingredient-current",
                language_code.as_deref()
            ),
            // Same rendering as the list ("sel" without quantity, not "  sel")
            crate::bot::ui_builder::ingredient_line(ingredient, &ingredient.ingredient_name),
            t_lang(
                ctx.localization,
                edit_instruction_key(field),
                language_code.as_deref()
            )
        );

        let keyboard = crate::bot::ui_components::create_ingredient_editing_keyboard(
            language_code.as_deref(),
            ctx.localization,
        );

        // Replace the current recipe display with the focused editing prompt
        match ctx
            .bot
            .edit_message_text(
                q.message
                    .as_ref()
                    .context("Callback query should have a message")?
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .context("Callback query should have a message")?
                    .id(),
                edit_prompt.clone(),
            )
            .reply_markup(keyboard.clone())
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_edit_saved_ingredient_button",
                    "Failed to edit message for ingredient editing prompt",
                    Some(crate::bot::telegram_id(q.from.id)),
                );
                // Fallback: send new message if editing fails
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .context("Callback query should have a message")?
                            .chat()
                            .id,
                        edit_prompt,
                    )
                    .reply_markup(keyboard)
                    .await?;
            }
        }

        // Transition to editing state with original message ID tracking
        dialogue
            .update(RecipeDialogueState::EditingSavedIngredient {
                recipe_id,
                original_ingredients: original_ingredients.to_vec(),
                current_matches: current_matches.to_vec(),
                editing_index: index,
                field,
                language_code: language_code.clone(),
                message_id,
                original_message_id: Some(
                    q.message
                        .as_ref()
                        .context("Callback query should have a message")?
                        .id()
                        .0,
                ),
            })
            .await?;
    }
    Ok(())
}

/// Handle delete button for saved ingredients
pub(super) async fn handle_delete_saved_ingredient_button(
    params: SavedIngredientsParams<'_>,
) -> Result<()> {
    let SavedIngredientsParams {
        ctx,
        q,
        data,
        current_matches,
        recipe_id,
        original_ingredients,
        language_code,
        message_id,
        dialogue,
        ..
    } = params;

    let data = data.unwrap_or("");
    let current_matches =
        current_matches.context("Current matches should be provided for delete callback")?;

    let index: usize = data
        .strip_prefix("delete_")
        .context("Delete callback data should start with 'delete_'")?
        .parse()
        .unwrap_or(0);

    if index < current_matches.len() {
        crate::observability::action(crate::observability::Action::IngredientDeleted);

        current_matches.remove(index);

        // The list shows what is left, or that nothing is left
        crate::bot::callbacks::ingredient_menu::show_list_after_delete(
            ctx.bot,
            q,
            current_matches,
            crate::bot::ui_builder::render_saved_editing(
                None,
                current_matches,
                language_code.as_deref(),
                ctx.localization,
            ),
            "add_ingredient",
            language_code.as_deref(),
            ctx.localization,
        )
        .await?;

        // Update dialogue state with modified ingredients
        match dialogue
            .update(RecipeDialogueState::EditingSavedIngredients {
                recipe_id,
                original_ingredients: original_ingredients.to_vec(),
                current_matches: current_matches.clone(),
                language_code: language_code.clone(),
                message_id,
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

/// Handle cancel button for saved ingredients editing
pub(super) async fn handle_cancel_saved_ingredients_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: Option<&str>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: Arc<PgPool>,
) -> Result<()> {
    let Some(message) = &q.message else {
        return Ok(());
    };
    // The recipe owner is the sender of the callback query, never a value from callback data
    cancel_saved_editing(
        bot,
        localization,
        &pool,
        dialogue,
        message.chat().id,
        crate::bot::telegram_id(q.from.id),
        language_code,
    )
    .await
}

/// Handle add ingredient button in editing saved ingredients state
pub(super) async fn handle_add_ingredient_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: Option<&str>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Get current dialogue state to preserve context
    let dialogue_state = dialogue.get().await?;
    if let Some(RecipeDialogueState::EditingSavedIngredients {
        recipe_id,
        original_ingredients,
        current_matches,
        message_id,
        ..
    }) = dialogue_state
    {
        send_transient(
            bot,
            q.message
                .as_ref()
                .context("Callback query should have a message")?
                .chat()
                .id,
            t_lang(localization, "add-ingredient-prompt", language_code),
        )
        .await?;

        // Transition to adding ingredient state
        dialogue
            .update(RecipeDialogueState::AddingIngredientToSavedRecipe {
                recipe_id,
                original_ingredients,
                current_matches,
                language_code: language_code.map(str::to_string),
                message_id,
            })
            .await?;
    }

    Ok(())
}
