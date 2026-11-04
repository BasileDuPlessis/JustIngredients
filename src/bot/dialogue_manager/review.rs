//! The review of a photo's ingredients: typed answers, and the edit of one line.

use crate::bot::html::{edit_html, send_html};
use crate::errors::AppError;
use crate::localization::t_lang;
use anyhow::Result;
use std::sync::Arc;
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import dialogue types
use crate::dialogue::RecipeDialogueState;

// Import UI builder functions
use crate::bot::ui_builder::{create_ingredient_review_keyboard, render_review};

// Import HandlerContext
use crate::bot::cleanup::send_transient;

use super::*;
use crate::bot::callbacks::review_callbacks::{
    ReviewConfirmation, cancel_review, confirm_review, move_review_down,
};

/// Handle ingredient edit input during dialogue
pub async fn handle_ingredient_edit_input(
    ctx: DialogueContext<'_>,
    params: IngredientEditInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let IngredientEditInputParams {
        edit_input,
        recipe_name,
        ingredients,
        editing_index,
        field,
        ctx: handler_ctx,
        message_id,
        extracted_text,
        user_input_message_id,
        recipe_name_from_caption,
    } = params;

    let input = edit_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        return handle_edit_cancellation(EditCancellationParams {
            ctx: handler_ctx,
            msg,
            dialogue,
            ingredients: &ingredients,
            recipe_name,
            message_id,
            extracted_text,
            recipe_name_from_caption: recipe_name_from_caption.clone(),
        })
        .await;
    }

    // Parse and validate the user input
    let parse_result =
        crate::ingredient_editing::apply_edit(field, ingredients.get(editing_index), edit_input);
    match parse_result {
        Ok(new_ingredient) => {
            handle_edit_success(EditSuccessParams {
                ctx: handler_ctx,
                msg,
                dialogue,
                ingredients,
                editing_index,
                new_ingredient,
                recipe_name,
                message_id,
                extracted_text,
                user_input_message_id,
                recipe_name_from_caption: recipe_name_from_caption.clone(),
            })
            .await
        }
        Err(error) => {
            handle_edit_error(
                bot,
                msg,
                handler_ctx.localization,
                error,
                handler_ctx.language_code,
            )
            .await
        }
    }
}

/// Handle cancellation of ingredient editing
async fn handle_edit_cancellation(params: EditCancellationParams<'_>) -> Result<()> {
    let EditCancellationParams {
        ctx,
        msg,
        dialogue,
        ingredients,
        recipe_name,
        message_id,
        extracted_text,
        recipe_name_from_caption,
    } = params;

    // User cancelled editing, return to review state without changes
    let review_message = render_review(ingredients, ctx.language_code, ctx.localization);

    let keyboard =
        create_ingredient_review_keyboard(ingredients, ctx.language_code, ctx.localization);

    // If we have a message_id, edit the existing message; otherwise send a new one
    if let Some(msg_id) = message_id {
        match edit_html(
            ctx.bot,
            msg.chat.id,
            teloxide::types::MessageId(msg_id),
            review_message,
        )
        .reply_markup(keyboard)
        .await
        {
            Ok(_) => (),
            Err(e) if is_message_not_modified_error(&e) => {
                debug!("Message edit skipped - content unchanged (edit cancellation)");
            }
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_edit_cancellation",
                    "Failed to edit message after edit cancellation",
                    Some(msg.chat.id.0),
                );
            }
        }
    } else {
        send_html(ctx.bot, msg.chat.id, review_message)
            .reply_markup(keyboard)
            .await?;
    }

    // Update dialogue state to review ingredients
    dialogue
        .update(RecipeDialogueState::ReviewIngredients {
            recipe_name,
            ingredients: ingredients.to_vec(),
            language_code: ctx.language_code.map(|s| s.to_string()),
            message_id,
            extracted_text,
            recipe_name_from_caption, // Preserve caption info
            open_menu: None,
        })
        .await?;

    Ok(())
}

/// Handle successful ingredient editing
async fn handle_edit_success(params: EditSuccessParams<'_>) -> Result<()> {
    let EditSuccessParams {
        ctx,
        msg,
        dialogue,
        mut ingredients,
        editing_index,
        new_ingredient,
        recipe_name,
        message_id,
        extracted_text,
        user_input_message_id,
        recipe_name_from_caption,
    } = params;

    // Update the ingredient at the editing index; the index after the last one adds it
    if crate::ingredient_editing::put_ingredient(&mut ingredients, editing_index, new_ingredient) {
        // Return to review state with updated ingredients
        let review_message = render_review(&ingredients, ctx.language_code, ctx.localization);

        let keyboard =
            create_ingredient_review_keyboard(&ingredients, ctx.language_code, ctx.localization);

        // If we have a message_id, edit the existing message; otherwise send a new one
        if let Some(msg_id) = message_id {
            match edit_html(
                ctx.bot,
                msg.chat.id,
                teloxide::types::MessageId(msg_id),
                review_message,
            )
            .reply_markup(keyboard)
            .await
            {
                Ok(_) => (),
                Err(e) if is_message_not_modified_error(&e) => {
                    debug!("Message edit skipped - content unchanged (edit success)");
                }
                Err(e) => {
                    error_logging::log_internal_error(
                        &e,
                        "handle_edit_success",
                        "Failed to edit message after edit success",
                        Some(msg.chat.id.0),
                    );
                }
            }
        } else {
            // Send new message with reply to user's input if available
            let mut send_request =
                send_html(ctx.bot, msg.chat.id, review_message).reply_markup(keyboard);

            if let Some(input_msg_id) = user_input_message_id {
                send_request = send_request.reply_parameters(
                    teloxide::types::ReplyParameters::new(teloxide::types::MessageId(input_msg_id)),
                );
            }

            send_request.await?;
        }

        // Update dialogue state to review ingredients
        dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: ctx.language_code.map(|s| s.to_string()),
                message_id,
                extracted_text,
                recipe_name_from_caption: recipe_name_from_caption.clone(), // Preserve caption info
                open_menu: None,
            })
            .await?;
    } else {
        // Invalid index, return to review state
        send_transient(
            ctx.bot,
            msg.chat.id,
            t_lang(ctx.localization, "error-invalid-edit", ctx.language_code),
        )
        .await?;
        dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: ctx.language_code.map(|s| s.to_string()),
                message_id,
                extracted_text,
                recipe_name_from_caption: recipe_name_from_caption.clone(), // Preserve caption info
                open_menu: None,
            })
            .await?;
    }

    Ok(())
}

/// Handle ingredient editing error
pub(super) async fn handle_edit_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error: AppError,
    language_code: Option<&str>,
) -> Result<()> {
    // Invalid input, ask user to try again
    let key = match &error {
        AppError::Validation(reason) => reason.l10n_key(),
        AppError::Parser(_) => "error-processing-failed",
        _ => "edit-invalid-input",
    };
    // "Try again with a valid ingredient format" only fits a whole typed line
    let error_message = if matches!(
        error,
        AppError::Validation(crate::errors::ValidationError::InvalidQuantityAnswer)
    ) {
        t_lang(localization, key, language_code)
    } else {
        format!(
            "{}\n\n{}",
            t_lang(localization, key, language_code),
            t_lang(localization, "edit-try-again", language_code)
        )
    };
    send_transient(bot, msg.chat.id, error_message).await?;
    // Stay in editing state for user to try again
    Ok(())
}

/// Handle ingredient review input during dialogue
pub async fn handle_ingredient_review_input(
    ctx: DialogueContext<'_>,
    params: IngredientReviewInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let IngredientReviewInputParams {
        pool,
        review_input,
        recipe_name_from_caption,
        ingredients,
        ctx: handler_ctx,
        extracted_text,
        review_message_id,
        recipe_name,
        open_menu,
    } = params;
    let input = review_input.trim().to_lowercase();

    match input.as_str() {
        input if is_confirmation_word(input) => {
            // Exactly what ✅ does: quantity check, then save under the known name or ask it
            confirm_review(ReviewConfirmation {
                bot,
                localization: handler_ctx.localization,
                pool: &pool,
                dialogue: &dialogue,
                chat_id: msg.chat.id,
                owner_id: msg.chat.id.0,
                review_message_id: review_message_id.map(teloxide::types::MessageId),
                ingredients: &ingredients,
                extracted_text: &extracted_text,
                known_name: recipe_name_from_caption.as_deref(),
                language_code: handler_ctx.language_code,
            })
            .await?;
        }
        input if is_cancellation_command(input) => {
            // Exactly what ❌ does: the list loses its buttons, then the home message
            cancel_review(
                bot,
                handler_ctx.localization,
                &pool,
                &dialogue,
                msg.chat.id,
                review_message_id.map(teloxide::types::MessageId),
                handler_ctx.language_code,
            )
            .await?;
        }
        _ => {
            // With an ingredient's menu open, the answer applies to it; else it is not
            // understood. Either way the list then comes down below the answer with its buttons.
            let answered = open_menu
                .and_then(|index| Some((index, ingredients.get(index)?)))
                .and_then(|(index, current)| {
                    Some((
                        index,
                        crate::ingredient_editing::menu_answer(current, review_input)?,
                    ))
                });
            match answered {
                Some((index, new_ingredient)) => {
                    handle_edit_success(EditSuccessParams {
                        ctx: handler_ctx,
                        msg,
                        dialogue: dialogue.clone(),
                        ingredients,
                        editing_index: index,
                        new_ingredient,
                        recipe_name,
                        message_id: review_message_id,
                        extracted_text,
                        user_input_message_id: Some(msg.id.0),
                        recipe_name_from_caption,
                    })
                    .await?;
                }
                None => {
                    send_transient(
                        bot,
                        msg.chat.id,
                        t_lang(
                            handler_ctx.localization,
                            "review-not-understood",
                            handler_ctx.language_code,
                        ),
                    )
                    .await?;
                }
            }
            move_review_down(
                bot,
                msg.chat.id,
                &dialogue,
                handler_ctx.localization,
                handler_ctx.language_code,
            )
            .await?;
        }
    }

    Ok(())
}
