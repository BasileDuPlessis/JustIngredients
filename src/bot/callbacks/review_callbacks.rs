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

// Import UI builder functions
use crate::bot::ui_builder::{create_ingredient_review_keyboard, create_post_confirmation_keyboard, format_ingredients_list};

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
                    recipe_name_from_caption: None,
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
                super::workflow_callbacks::handle_workflow_button(bot, q, data, &pool, dialogue, localization).await?;
            }
        }
    }

    Ok(())
}

/// Handle edit button in review ingredients state
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
        dialogue,
        ..
    } = params;

    let data = data.unwrap_or("");
    let ingredients = ingredients_slice.unwrap();
    let index: usize = data.strip_prefix("edit_").unwrap().parse().unwrap_or(0);
    if index < ingredients.len() {
        // Record user engagement metric for ingredient editing
        crate::observability::record_user_engagement_metrics(
            q.from.id.0 as i64,
            crate::observability::UserAction::IngredientEdit,
            None, // No session duration for individual actions
            dialogue_lang_code.as_deref(),
        );

        let ingredient = &ingredients[index];
        let edit_prompt = format!(
            "✏️ {}\n\n{}: **{} {}**\n\n{}",
            t_lang(
                ctx.localization,
                "edit-ingredient-prompt",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "current-ingredient",
                dialogue_lang_code.as_deref()
            ),
            ingredient.quantity,
            ingredient.measurement.as_deref().unwrap_or(""),
            ingredient.ingredient_name
        );
        ctx.bot
            .send_message(q.message.as_ref().unwrap().chat().id, edit_prompt)
            .await?;

        // Transition to editing state
        dialogue
            .update(RecipeDialogueState::EditingIngredient {
                recipe_name: recipe_name.to_string(),
                ingredients: ingredients.to_vec(),
                editing_index: index,
                language_code: dialogue_lang_code.clone(),
                message_id,
                extracted_text: extracted_text.to_string(),
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
    let ingredients = ingredients.unwrap();
    let index: usize = data.strip_prefix("delete_").unwrap().parse().unwrap_or(0);

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
                    q.message.as_ref().unwrap().chat().id,
                    q.message.as_ref().unwrap().id(),
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
                    q.message.as_ref().unwrap().chat().id,
                    q.message.as_ref().unwrap().id(),
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

    let ingredients = ingredients_slice.unwrap();
    let pool = pool.unwrap();

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
                    q.message.as_ref().unwrap().chat().id,
                    t_lang(
                        ctx.localization,
                        "error-processing-failed",
                        dialogue_lang_code.as_deref(),
                    ),
                )
                .await?;
            return Ok(());
        }

        // Show confirmation with caption recipe name
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

        // Update the original review message
        match ctx
            .bot
            .edit_message_text(
                q.message.as_ref().unwrap().chat().id,
                q.message.as_ref().unwrap().id(),
                confirmation_message,
            )
            .reply_markup(confirmation_keyboard)
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "callback_handler",
                    "Failed to update message after confirmation",
                    Some(q.from.id.0 as i64),
                );
            }
        }

        // End the dialogue - workflow complete
        dialogue.exit().await?;
    } else {
        // LEGACY WORKFLOW: No caption available, ask for recipe name
        debug!(user_id = %q.from.id, "No caption available, proceeding with recipe name input");

        let confirmation_message = format!(
            "✅ **{}**\n\n{}",
            t_lang(
                ctx.localization,
                "workflow-recipe-saved",
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

        // Update the original review message
        match ctx
            .bot
            .edit_message_text(
                q.message.as_ref().unwrap().chat().id,
                q.message.as_ref().unwrap().id(),
                confirmation_message,
            )
            .reply_markup(confirmation_keyboard)
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "callback_handler",
                    "Failed to update message after confirmation",
                    Some(q.from.id.0 as i64),
                );
            }
        }

        // Send recipe name prompt
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

        ctx.bot
            .send_message(q.message.as_ref().unwrap().chat().id, recipe_name_prompt)
            .await?;

        // Transition to waiting for recipe name after confirmation
        dialogue
            .update(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                ingredients: ingredients.to_vec(),
                language_code: dialogue_lang_code.clone(),
                extracted_text: extracted_text.to_string(),
                recipe_name_from_caption: None, // No caption available
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
        q.message.as_ref().unwrap().chat().id,
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
        q.message.as_ref().unwrap().chat().id,
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