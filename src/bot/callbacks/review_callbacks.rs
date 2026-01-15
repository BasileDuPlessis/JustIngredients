//! Review Ingredients Callbacks Module
//!
//! This module handles all callback queries that occur when the user is in the
//! ReviewIngredients dialogue state. This includes editing, deleting, confirming,
//! and canceling ingredient reviews.

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI components for the focused editing interface
use crate::bot::ui_components::create_ingredient_editing_keyboard;
use crate::bot::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard, format_ingredients_list,
};

// Import HandlerContext
use crate::bot::HandlerContext;

// Import callback types module
use super::callback_types::ReviewIngredientsParams;

// Import dialogue manager functions
use crate::bot::dialogue_manager::save_ingredients_to_database;

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
    }) = dialogue_state
    {
        if q.message.is_some() {
            if data.starts_with("edit_") {
                handle_edit_button(ReviewIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: dialogue_lang_code.as_deref(),
                    },
                    q,
                    data: Some(data),
                    ingredients: None,
                    ingredients_slice: Some(&ingredients),
                    recipe_name: &recipe_name,
                    dialogue_lang_code: &dialogue_lang_code,
                    message_id,
                    extracted_text: &extracted_text,
                    recipe_name_from_caption: Some(&recipe_name_from_caption),
                    dialogue,
                    pool: None,
                })
                .await?;
            } else if data.starts_with("delete_") {
                handle_delete_button(ReviewIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: dialogue_lang_code.as_deref(),
                    },
                    q,
                    data: Some(data),
                    ingredients: Some(&mut ingredients),
                    ingredients_slice: None,
                    recipe_name: &recipe_name,
                    dialogue_lang_code: &dialogue_lang_code,
                    message_id,
                    extracted_text: &extracted_text,
                    recipe_name_from_caption: Some(&recipe_name_from_caption),
                    dialogue,
                    pool: None,
                })
                .await?;
            } else if data == "confirm" {
                handle_confirm_button(ReviewIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: dialogue_lang_code.as_deref(),
                    },
                    q,
                    data: None,
                    ingredients: None,
                    ingredients_slice: Some(&ingredients),
                    recipe_name: &recipe_name,
                    dialogue_lang_code: &dialogue_lang_code,
                    message_id,
                    extracted_text: &extracted_text,
                    recipe_name_from_caption: Some(&recipe_name_from_caption),
                    dialogue,
                    pool: Some(&pool),
                })
                .await?;
            } else if data == "add_more" {
                handle_add_more_button(bot, q, &dialogue_lang_code, dialogue, localization).await?;
            } else if data == "cancel_review" {
                handle_cancel_review_button(bot, q, &dialogue_lang_code, dialogue, localization)
                    .await?;
            } else if data.starts_with("workflow_") {
                super::workflow_callbacks::handle_workflow_button(
                    bot,
                    q,
                    data,
                    &pool,
                    dialogue,
                    localization,
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Handle edit button in review ingredients state
///
/// This function implements the "focused editing interface" approach to eliminate user confusion:
/// - Instead of leaving the full recipe display visible with inactive buttons during editing,
///   we replace the entire recipe display message with a clean, focused editing prompt
/// - Only a cancel button is shown, eliminating inactive button confusion
/// - After editing or canceling, the original recipe display is restored seamlessly
/// - This provides a clean, unambiguous editing experience without UI state confusion
async fn handle_edit_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ReviewIngredientsParams {
        ctx,
        q,
        data,
        ingredients_slice,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        ..
    } = params;

    let data = data.unwrap_or("");
    let ingredients = ingredients_slice.expect("Ingredients slice should be provided for edit callback");
    let index: usize = data.strip_prefix("edit_").expect("Edit callback data should start with 'edit_'").parse().unwrap_or(0);
    if index < ingredients.len() {
        // Record user engagement metric for ingredient editing
        crate::observability::record_user_engagement_metrics(
            q.from.id.0 as i64,
            crate::observability::UserAction::IngredientEdit,
            None, // No session duration for individual actions
            dialogue_lang_code.as_deref(),
        );

        let ingredient = &ingredients[index];

        // Create focused editing prompt message
        let edit_prompt = format!(
            "✏️ {}\n\n{}: **{} {} {}**\n\n{}",
            t_lang(
                ctx.localization,
                "edit-ingredient-title",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "edit-ingredient-current",
                dialogue_lang_code.as_deref()
            ),
            ingredient.quantity,
            ingredient.measurement.as_deref().unwrap_or(""),
            ingredient.ingredient_name,
            t_lang(
                ctx.localization,
                "edit-ingredient-instruction",
                dialogue_lang_code.as_deref()
            )
        );

        // Create focused editing keyboard with cancel button only
        let keyboard =
            create_ingredient_editing_keyboard(dialogue_lang_code.as_deref(), ctx.localization);

        // Replace the original recipe display message with focused editing prompt
        let edited_message = match ctx
            .bot
            .edit_message_text(
                q.message.as_ref().expect("Callback query should have a message").chat().id,
                teloxide::types::MessageId(
                    message_id.expect("Message ID should be present for editing"),
                ),
                edit_prompt.clone(),
            )
            .reply_markup(keyboard.clone())
            .await
        {
            Ok(msg) => msg,
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_edit_button",
                    "Failed to replace recipe display with editing prompt",
                    Some(q.from.id.0 as i64),
                );
                // Fallback: send new message if editing fails
                ctx.bot
                    .send_message(q.message.as_ref().expect("Callback query should have a message").chat().id, edit_prompt)
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
                language_code: dialogue_lang_code.clone(),
                message_id: Some(edited_message.id.0 as i32), // Track the editing prompt message
                original_message_id: message_id, // Original recipe display message to replace
                extracted_text: extracted_text.to_string(),
                recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info
            })
            .await?;
    }
    Ok(())
}

/// Handle delete button in review ingredients state
async fn handle_delete_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
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
    let ingredients = ingredients.expect("Ingredients should be provided for delete callback");
    let index: usize = data.strip_prefix("delete_").expect("Delete callback data should start with 'delete_'").parse().unwrap_or(0);

    if index < ingredients.len() {
        // Record user engagement metric for ingredient deletion
        crate::observability::record_user_engagement_metrics(
            q.from.id.0 as i64,
            crate::observability::UserAction::IngredientDelete,
            None, // No session duration for individual actions
            dialogue_lang_code.as_deref(),
        );

        ingredients.remove(index);

        // Check if all ingredients were deleted
        if ingredients.is_empty() {
            // All ingredients deleted - inform user and provide options
            let empty_message = format!(
                "🗑️ **{}**\n\n{}\n\n{}",
                t_lang(
                    ctx.localization,
                    "review-title",
                    dialogue_lang_code.as_deref()
                ),
                t_lang(
                    ctx.localization,
                    "review-no-ingredients",
                    dialogue_lang_code.as_deref()
                ),
                t_lang(
                    ctx.localization,
                    "review-no-ingredients-help",
                    dialogue_lang_code.as_deref()
                )
            );

            let keyboard = vec![vec![
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(
                        ctx.localization,
                        "review-add-more",
                        dialogue_lang_code.as_deref(),
                    ),
                    "add_more",
                ),
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(ctx.localization, "cancel", dialogue_lang_code.as_deref()),
                    "cancel_empty",
                ),
            ]];

            // Edit the original message
            match ctx
                .bot
                .edit_message_text(
                    q.message.as_ref().expect("Callback query should have a message").chat().id,
                    q.message.as_ref().expect("Callback query should have a message").id(),
                    empty_message,
                )
                .reply_markup(teloxide::types::InlineKeyboardMarkup::new(keyboard))
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    error_logging::log_internal_error(
                        &e,
                        "callback_handler",
                        "Failed to edit message for empty ingredients",
                        Some(q.from.id.0 as i64),
                    );
                }
            }
        } else {
            // Update the message with remaining ingredients
            let review_message = format!(
                "📝 **{}**\n\n{}\n\n{}",
                t_lang(
                    ctx.localization,
                    "review-title",
                    dialogue_lang_code.as_deref()
                ),
                t_lang(
                    ctx.localization,
                    "review-description",
                    dialogue_lang_code.as_deref()
                ),
                format_ingredients_list(
                    ingredients,
                    dialogue_lang_code.as_deref(),
                    ctx.localization
                )
            );

            let keyboard = create_ingredient_review_keyboard(
                ingredients,
                dialogue_lang_code.as_deref(),
                ctx.localization,
            );

            // Edit the original message
            match ctx
                .bot
                .edit_message_text(
                    q.message.as_ref().expect("Callback query should have a message").chat().id,
                    q.message.as_ref().expect("Callback query should have a message").id(),
                    review_message,
                )
                .reply_markup(keyboard)
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    error_logging::log_internal_error(
                        &e,
                        "callback_handler",
                        "Failed to edit message after ingredient deletion",
                        Some(q.from.id.0 as i64),
                    );
                }
            }
        }

        // Update dialogue state with modified ingredients
        match dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name: recipe_name.to_string(),
                ingredients: ingredients.clone(),
                language_code: dialogue_lang_code.clone(),
                message_id,
                extracted_text: extracted_text.to_string(),
                recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info
            })
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "callback_handler",
                    "Failed to update dialogue state after deletion",
                    Some(q.from.id.0 as i64),
                );
            }
        }
    }
    Ok(())
}

/// Handle confirm button in review ingredients state
async fn handle_confirm_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ReviewIngredientsParams {
        ctx,
        q,
        ingredients_slice,
        dialogue_lang_code,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        pool,
        ..
    } = params;

    let ingredients = ingredients_slice.expect("Ingredients slice should be provided for confirm callback");
    let pool = pool.expect("Database pool should be provided for confirm callback");

    // Record user engagement metric for recipe confirmation
    crate::observability::record_user_engagement_metrics(
        q.from.id.0 as i64,
        crate::observability::UserAction::RecipeConfirm,
        None, // No session duration for individual actions
        dialogue_lang_code.as_deref(),
    );

    // Check if we have a recipe name from caption
    if let Some(caption_recipe_name) = recipe_name_from_caption.and_then(|opt| opt.as_ref()) {
        // STREAMLINED WORKFLOW: Skip recipe name input when caption is available
        debug!(user_id = %q.from.id, recipe_name = %caption_recipe_name, "Using recipe name from caption, skipping name input");

        // Save ingredients directly to database
        if let Err(e) = save_ingredients_to_database(
            pool,
            q.from.id.0 as i64,
            extracted_text,
            ingredients,
            caption_recipe_name,
            dialogue_lang_code.as_deref(),
        )
        .await
        {
            error_logging::log_database_error(
                &e,
                "save_ingredients_to_database",
                Some(q.from.id.0 as i64),
                None,
            );
            ctx.bot
                .send_message(
                    q.message.as_ref().expect("Callback query should have a message").chat().id,
                    t_lang(
                        ctx.localization,
                        "error-processing-failed",
                        dialogue_lang_code.as_deref(),
                    ),
                )
                .await?;
            return Ok(());
        }

        // Remove the keyboard from the ingredients message to keep it visible
        match ctx
            .bot
            .edit_message_reply_markup(
                q.message.as_ref().expect("Callback query should have a message").chat().id,
                q.message.as_ref().expect("Callback query should have a message").id(),
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_confirm_button",
                    "Failed to remove keyboard from ingredients message",
                    Some(q.from.id.0 as i64),
                );
            }
        }

        // Send confirmation as a new message
        let confirmation_message = format!(
            "✅ **{}**\n\n📝 {}\n\n{}",
            t_lang(
                ctx.localization,
                "workflow-recipe-saved",
                dialogue_lang_code.as_deref()
            ),
            t_args_lang(
                ctx.localization,
                "caption-recipe-saved",
                &[("recipe_name", caption_recipe_name.as_str())],
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "workflow-what-next",
                dialogue_lang_code.as_deref()
            )
        );

        let confirmation_keyboard =
            create_post_confirmation_keyboard(dialogue_lang_code.as_deref(), ctx.localization);

        ctx.bot
            .send_message(q.message.as_ref().expect("Callback query should have a message").chat().id, confirmation_message)
            .reply_markup(confirmation_keyboard)
            .await?;

        // End the dialogue - workflow complete
        dialogue.exit().await?;
    } else {
        // LEGACY WORKFLOW: No caption available, ask for recipe name
        debug!(user_id = %q.from.id, "No caption available, proceeding with recipe name input");

        // Remove the keyboard from the ingredients message to keep it visible
        match ctx
            .bot
            .edit_message_reply_markup(
                q.message.as_ref().expect("Callback query should have a message").chat().id,
                q.message.as_ref().expect("Callback query should have a message").id(),
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_confirm_button",
                    "Failed to remove keyboard from ingredients message",
                    Some(q.from.id.0 as i64),
                );
            }
        }

        // Send recipe name prompt as a new message
        let recipe_name_prompt = format!(
            "🏷️ **{}**\n\n{}",
            t_lang(
                ctx.localization,
                "recipe-name-prompt",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "recipe-name-prompt-hint",
                dialogue_lang_code.as_deref()
            )
        );

        let prompt_msg = ctx
            .bot
            .send_message(q.message.as_ref().expect("Callback query should have a message").chat().id, recipe_name_prompt)
            .await?;

        // Transition to waiting for recipe name after confirmation
        dialogue
            .update(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                ingredients: ingredients.to_vec(),
                language_code: dialogue_lang_code.clone(),
                extracted_text: extracted_text.to_string(),
                recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info from ReviewIngredients state
                message_id: Some(prompt_msg.id.0 as i32), // Store prompt message ID
            })
            .await?;
    }

    Ok(())
}

/// Handle add more button in review ingredients state
async fn handle_add_more_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    dialogue_lang_code: &Option<String>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    bot.send_message(
        q.message.as_ref().expect("Callback query should have a message").chat().id,
        t_lang(
            localization,
            "review-add-more-instructions",
            dialogue_lang_code.as_deref(),
        ),
    )
    .await?;

    // Reset dialogue to start state
    dialogue.update(RecipeDialogueState::Start).await?;
    Ok(())
}

/// Handle cancel review button in review ingredients state
async fn handle_cancel_review_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    dialogue_lang_code: &Option<String>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    bot.send_message(
        q.message.as_ref().expect("Callback query should have a message").chat().id,
        t_lang(
            localization,
            "review-cancelled",
            dialogue_lang_code.as_deref(),
        ),
    )
    .await?;

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}
