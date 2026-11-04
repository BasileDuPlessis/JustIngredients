//! Editing a saved recipe's ingredients: typed answers, one line, an added ingredient.

use crate::bot::html::{edit_html, send_html};
use crate::localization::t_lang;
use anyhow::Result;
use std::sync::Arc;
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import validation functions
use crate::validation::parse_ingredient_from_text;

// Import UI builder functions
use crate::bot::ui_builder::{create_ingredient_review_keyboard, render_saved_editing};

// Import HandlerContext
use crate::bot::cleanup::{self, send_transient};

use super::*;

/// Handle adding new ingredient input for saved recipes
pub async fn handle_add_ingredient_input(
    ctx: DialogueContext<'_>,
    params: AddIngredientInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let AddIngredientInputParams {
        pool: _pool,
        add_input,
        recipe_id,
        original_ingredients,
        current_matches,
        ctx: handler_ctx,
        message_id,
    } = params;

    let input = add_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        // Return to editing saved ingredients state without changes
        return_to_saved_ingredients_review(ReturnToSavedIngredientsReviewParams {
            bot,
            msg,
            dialogue,
            localization: handler_ctx.localization,
            recipe_id,
            original_ingredients,
            current_matches,
            language_code: handler_ctx.language_code,
            message_id,
            user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
        })
        .await?;
        return Ok(());
    }

    // Parse and validate the user input
    match parse_ingredient_from_text(add_input) {
        Ok(new_ingredient) => {
            // Add the new ingredient to current matches
            let mut updated_matches = current_matches.to_vec();
            updated_matches.push(new_ingredient);

            // Return to editing state with updated ingredients
            return_to_saved_ingredients_review(ReturnToSavedIngredientsReviewParams {
                bot,
                msg,
                dialogue,
                localization: handler_ctx.localization,
                recipe_id,
                original_ingredients,
                current_matches: &updated_matches,
                language_code: handler_ctx.language_code,
                message_id,
                user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
            })
            .await?;
        }
        Err(error) => {
            handle_edit_error(
                bot,
                msg,
                handler_ctx.localization,
                error,
                handler_ctx.language_code,
            )
            .await?
        }
    }

    Ok(())
}

/// Handle editing individual ingredient input for saved recipes
pub async fn handle_saved_ingredient_edit_input(
    ctx: DialogueContext<'_>,
    params: SavedIngredientEditInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let SavedIngredientEditInputParams {
        pool: _pool,
        edit_input,
        recipe_id,
        original_ingredients,
        current_matches,
        ctx: handler_ctx,
        message_id: _,
        editing_index,
        field,
        original_message_id,
        user_input_message_id,
    } = params;

    let input = edit_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        // Return to editing saved ingredients state without changes
        return_to_saved_ingredients_review(ReturnToSavedIngredientsReviewParams {
            bot,
            msg,
            dialogue,
            localization: handler_ctx.localization,
            recipe_id,
            original_ingredients,
            current_matches,
            language_code: handler_ctx.language_code,
            message_id: original_message_id, // Use original message ID for editing
            user_input_message_id,
        })
        .await?;
        return Ok(());
    }

    // Parse and validate the user input
    match crate::ingredient_editing::apply_edit(
        field,
        current_matches.get(editing_index),
        edit_input,
    ) {
        Ok(new_ingredient) => {
            // Update the ingredient at the editing index
            if editing_index < current_matches.len() {
                let mut updated_matches = current_matches.to_vec();
                updated_matches[editing_index] = new_ingredient;

                // Return to editing state with updated ingredients
                return_to_saved_ingredients_review(ReturnToSavedIngredientsReviewParams {
                    bot,
                    msg,
                    dialogue,
                    localization: handler_ctx.localization,
                    recipe_id,
                    original_ingredients,
                    current_matches: &updated_matches,
                    language_code: handler_ctx.language_code,
                    message_id: original_message_id, // Use original message ID for editing
                    user_input_message_id,
                })
                .await?;
            } else {
                // Invalid index
                send_transient(
                    bot,
                    msg.chat.id,
                    t_lang(
                        handler_ctx.localization,
                        "error-invalid-edit",
                        handler_ctx.language_code,
                    ),
                )
                .await?;
                return_to_saved_ingredients_review(ReturnToSavedIngredientsReviewParams {
                    bot,
                    msg,
                    dialogue,
                    localization: handler_ctx.localization,
                    recipe_id,
                    original_ingredients,
                    current_matches,
                    language_code: handler_ctx.language_code,
                    message_id: original_message_id, // Use original message ID for editing
                    user_input_message_id,
                })
                .await?;
            }
        }
        Err(error) => {
            handle_edit_error(
                bot,
                msg,
                handler_ctx.localization,
                error,
                handler_ctx.language_code,
            )
            .await?
        }
    }

    Ok(())
}

/// Helper function to return to saved ingredients review state
/// A text typed while a saved recipe's ingredients are edited (not a cancel word): with an
/// ingredient's menu open, it is that ingredient's new quantity, or else its whole new line;
/// anything else gets `editing-not-understood`. Either way the editing view comes down below
/// the answer with its buttons, and the one above goes (user report of 2026-11-03, as for the
/// review).
pub async fn handle_saved_editing_text(
    bot: &Bot,
    msg: &Message,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    message_language: Option<&str>,
    text: &str,
) -> Result<()> {
    let Some(RecipeDialogueState::EditingSavedIngredients {
        recipe_id,
        original_ingredients,
        mut current_matches,
        language_code,
        message_id,
        open_menu,
    }) = dialogue.get().await?
    else {
        return Ok(());
    };
    let language = language_code.as_deref().or(message_language);
    let edited = open_menu
        .and_then(|index| Some((index, current_matches.get(index)?)))
        .and_then(|(index, current)| {
            Some((
                index,
                crate::ingredient_editing::menu_answer(current, text)?,
            ))
        });
    match edited {
        Some((index, ingredient)) => current_matches[index] = ingredient,
        None => {
            send_transient(
                bot,
                msg.chat.id,
                t_lang(localization, "editing-not-understood", language),
            )
            .await?;
        }
    }
    let view = send_html(
        bot,
        msg.chat.id,
        render_saved_editing(None, &current_matches, language, localization),
    )
    .reply_markup(create_ingredient_review_keyboard(
        &current_matches,
        language,
        localization,
    ))
    .await?;
    // Like the view it replaces: it goes when the edit ends (the recipe's card is updated)
    cleanup::track(msg.chat.id, view.id).await;
    if let Some(above) = message_id.map(teloxide::types::MessageId) {
        if bot.delete_message(msg.chat.id, above).await.is_ok() {
            cleanup::untrack(msg.chat.id, above).await;
        } else if let Err(error) = bot.edit_message_reply_markup(msg.chat.id, above).await {
            debug!(error = %error, "Could not remove the buttons of the editing view above");
        }
    }
    dialogue
        .update(RecipeDialogueState::EditingSavedIngredients {
            recipe_id,
            original_ingredients,
            current_matches,
            language_code,
            message_id: Some(view.id.0),
            open_menu: None,
        })
        .await?;
    Ok(())
}

async fn return_to_saved_ingredients_review(
    params: ReturnToSavedIngredientsReviewParams<'_>,
) -> Result<()> {
    let ReturnToSavedIngredientsReviewParams {
        bot,
        msg,
        dialogue,
        localization,
        recipe_id,
        original_ingredients,
        current_matches,
        language_code,
        message_id,
        user_input_message_id,
    } = params;
    // Send updated ingredient list message
    let review_message = render_saved_editing(None, current_matches, language_code, localization);

    let keyboard = create_ingredient_review_keyboard(current_matches, language_code, localization);

    // If we have a message_id, edit the existing message; otherwise send a new one
    if let Some(msg_id) = message_id {
        match edit_html(
            bot,
            msg.chat.id,
            teloxide::types::MessageId(msg_id),
            review_message,
        )
        .reply_markup(keyboard)
        .await
        {
            Ok(_) => (),
            Err(e) if is_message_not_modified_error(&e) => {
                debug!("Message edit skipped - content unchanged (saved ingredients)");
            }
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "return_to_saved_ingredients_review",
                    "Failed to edit message for saved ingredients review",
                    Some(msg.chat.id.0),
                );
            }
        }
    } else {
        // Send new message with reply to user's input if available
        let mut send_request = send_html(bot, msg.chat.id, review_message).reply_markup(keyboard);

        if let Some(input_msg_id) = user_input_message_id {
            send_request = send_request.reply_parameters(teloxide::types::ReplyParameters::new(
                teloxide::types::MessageId(input_msg_id),
            ));
        }

        send_request.await?;
    }

    // Update dialogue state
    dialogue
        .update(RecipeDialogueState::EditingSavedIngredients {
            recipe_id,
            original_ingredients: original_ingredients.to_vec(),
            current_matches: current_matches.to_vec(),
            language_code: language_code.map(|s| s.to_string()),
            message_id,
            open_menu: None,
        })
        .await?;

    Ok(())
}
