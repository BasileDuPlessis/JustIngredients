//! Editing Callbacks module for handling EditingSavedIngredients dialogue state

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::error;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use crate::bot::ui_builder::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard,
    create_recipe_details_keyboard,
    format_database_ingredients_list, format_ingredients_list,
};

// Import HandlerContext
use crate::bot::HandlerContext;

// Import callback types module
use super::callback_types::SavedIngredientsParams;

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
    }) = dialogue_state
    {
        if q.message.is_some() {
            if data.starts_with("edit_") {
                handle_edit_saved_ingredient_button(SavedIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: language_code.as_deref(),
                    },
                    q,
                    data: Some(data),
                    current_matches: None,
                    current_matches_slice: Some(&current_matches),
                    recipe_id,
                    original_ingredients: &original_ingredients,
                    language_code: &language_code,
                    message_id,
                    dialogue,
                    pool: None,
                })
                .await?;
            } else if data.starts_with("delete_") {
                handle_delete_saved_ingredient_button(SavedIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: language_code.as_deref(),
                    },
                    q,
                    data: Some(data),
                    current_matches: Some(&mut current_matches),
                    current_matches_slice: None,
                    recipe_id,
                    original_ingredients: &original_ingredients,
                    language_code: &language_code,
                    message_id,
                    dialogue,
                    pool: None,
                })
                .await?;
            } else if data == "confirm" {
                handle_confirm_saved_ingredients_button(SavedIngredientsParams {
                    ctx: &HandlerContext {
                        bot,
                        localization,
                        language_code: language_code.as_deref(),
                    },
                    q,
                    data: None,
                    current_matches: None,
                    current_matches_slice: Some(&current_matches),
                    recipe_id,
                    original_ingredients: &original_ingredients,
                    language_code: &language_code,
                    message_id,
                    dialogue,
                    pool: Some(&pool),
                })
                .await?;
            } else if data == "add_ingredient" {
                handle_add_ingredient_button(bot, q, &language_code, dialogue, localization)
                    .await?;
            } else if data == "cancel_review" {
                handle_cancel_saved_ingredients_button(
                    bot,
                    q,
                    &language_code,
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

/// Handle edit button for saved ingredients
async fn handle_edit_saved_ingredient_button(params: SavedIngredientsParams<'_>) -> Result<()> {
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

    let data = data.unwrap_or("");
    let current_matches = current_matches_slice.unwrap();

    let index: usize = data.strip_prefix("edit_").unwrap().parse().unwrap_or(0);
    if index < current_matches.len() {
        // Record user engagement metric for ingredient editing
        crate::observability::record_user_engagement_metrics(
            q.from.id.0 as i64,
            crate::observability::UserAction::IngredientEdit,
            None,
            language_code.as_deref(),
        );

        let ingredient = &current_matches[index];
        let edit_prompt = format!(
            "✏️ {}\n\n{}: **{} {}**\n\n{}",
            t_lang(
                ctx.localization,
                "edit-ingredient-prompt",
                language_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "current-ingredient",
                language_code.as_deref()
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
            .update(RecipeDialogueState::EditingSavedIngredient {
                recipe_id,
                original_ingredients: original_ingredients.to_vec(),
                current_matches: current_matches.to_vec(),
                editing_index: index,
                language_code: language_code.clone(),
                message_id,
            })
            .await?;
    }
    Ok(())
}

/// Handle delete button for saved ingredients
async fn handle_delete_saved_ingredient_button(params: SavedIngredientsParams<'_>) -> Result<()> {
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
    let current_matches = current_matches.unwrap();

    let index: usize = data.strip_prefix("delete_").unwrap().parse().unwrap_or(0);

    if index < current_matches.len() {
        // Record user engagement metric for ingredient deletion
        crate::observability::record_user_engagement_metrics(
            q.from.id.0 as i64,
            crate::observability::UserAction::IngredientDelete,
            None,
            language_code.as_deref(),
        );

        current_matches.remove(index);

        // Check if all ingredients were deleted
        if current_matches.is_empty() {
            // All ingredients deleted - inform user and provide options
            let empty_message = format!(
                "🗑️ **{}**\n\n{}\n\n{}",
                t_lang(ctx.localization, "review-title", language_code.as_deref()),
                t_lang(
                    ctx.localization,
                    "review-no-ingredients",
                    language_code.as_deref()
                ),
                t_lang(
                    ctx.localization,
                    "review-no-ingredients-help",
                    language_code.as_deref()
                )
            );

            let keyboard = vec![vec![
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(
                        ctx.localization,
                        "review-add-more",
                        language_code.as_deref(),
                    ),
                    "add_more",
                ),
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(ctx.localization, "cancel", language_code.as_deref()),
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
                "✏️ **{}**\n\n{}\n\n{}",
                t_lang(ctx.localization, "editing-recipe", language_code.as_deref()),
                t_lang(
                    ctx.localization,
                    "editing-instructions",
                    language_code.as_deref()
                ),
                format_ingredients_list(
                    current_matches,
                    language_code.as_deref(),
                    ctx.localization
                )
            );

            let keyboard = create_ingredient_review_keyboard(
                current_matches,
                language_code.as_deref(),
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
            .update(RecipeDialogueState::EditingSavedIngredients {
                recipe_id,
                original_ingredients: original_ingredients.to_vec(),
                current_matches: current_matches.clone(),
                language_code: language_code.clone(),
                message_id,
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

/// Handle confirm button for saved ingredients
async fn handle_confirm_saved_ingredients_button(params: SavedIngredientsParams<'_>) -> Result<()> {
    let SavedIngredientsParams {
        ctx,
        q,
        current_matches_slice,
        original_ingredients,
        recipe_id,
        language_code,
        dialogue,
        pool,
        ..
    } = params;

    let current_matches = current_matches_slice.unwrap();
    let pool = pool.unwrap();

    // Record user engagement metric for recipe confirmation
    crate::observability::record_user_engagement_metrics(
        q.from.id.0 as i64,
        crate::observability::UserAction::RecipeConfirm,
        None,
        language_code.as_deref(),
    );

    // Detect changes between original and current ingredients
    let changes =
        crate::ingredient_editing::detect_ingredient_changes(original_ingredients, current_matches);

    // Apply changes to database
    if !changes.to_update.is_empty() || !changes.to_add.is_empty() || !changes.to_delete.is_empty()
    {
        // Update existing ingredients
        for (ingredient_id, new_data) in &changes.to_update {
            if let Err(e) = crate::db::update_ingredient(
                pool,
                *ingredient_id,
                Some(&new_data.ingredient_name),
                new_data.quantity.parse().ok(),
                new_data.measurement.as_deref(),
            )
            .await
            {
                error_logging::log_database_error(
                    &e,
                    "update_ingredient",
                    Some(q.from.id.0 as i64),
                    Some(&[("ingredient_id", &ingredient_id.to_string())]),
                );
                ctx.bot
                    .send_message(
                        q.message.as_ref().unwrap().chat().id,
                        t_lang(
                            ctx.localization,
                            "error-updating-ingredients",
                            language_code.as_deref(),
                        ),
                    )
                    .await?;
                return Ok(());
            }
        }

        // Add new ingredients
        for new_ingredient in &changes.to_add {
            // Get the internal user ID from the database
            let user = match crate::db::get_or_create_user(
                pool,
                q.from.id.0 as i64,
                language_code.as_deref(),
            )
            .await
            {
                Ok(user) => user,
                Err(e) => {
                    error_logging::log_database_error(
                        &e,
                        "get_or_create_user",
                        Some(q.from.id.0 as i64),
                        None,
                    );
                    ctx.bot
                        .send_message(
                            q.message.as_ref().unwrap().chat().id,
                            t_lang(
                                ctx.localization,
                                "error-processing-failed",
                                language_code.as_deref(),
                            ),
                        )
                        .await?;
                    return Ok(());
                }
            };

            let quantity = new_ingredient.quantity.parse().ok();
            let unit = new_ingredient.measurement.as_deref();
            error!(
                user_id = %user.id,
                telegram_id = %q.from.id.0,
                recipe_id = %recipe_id,
                ingredient_name = %new_ingredient.ingredient_name,
                quantity = ?quantity,
                unit = ?unit,
                "Attempting to add new ingredient"
            );
            if let Err(e) = crate::db::create_ingredient(
                pool,
                user.id, // Use internal database user ID
                Some(recipe_id),
                &new_ingredient.ingredient_name,
                quantity,
                unit,
                "", // raw_text not meaningful for edited ingredients
            )
            .await
            {
                error_logging::log_database_error(
                    &e,
                    "create_ingredient",
                    Some(q.from.id.0 as i64),
                    Some(&[("recipe_id", &recipe_id.to_string())]),
                );
                ctx.bot
                    .send_message(
                        q.message.as_ref().unwrap().chat().id,
                        t_lang(
                            ctx.localization,
                            "error-adding-ingredients",
                            language_code.as_deref(),
                        ),
                    )
                    .await?;
                return Ok(());
            }
        }

        // Delete ingredients
        for ingredient_id in &changes.to_delete {
            if let Err(e) = crate::db::delete_ingredient(pool, *ingredient_id).await {
                error_logging::log_database_error(
                    &e,
                    "delete_ingredient",
                    Some(q.from.id.0 as i64),
                    Some(&[("ingredient_id", &ingredient_id.to_string())]),
                );
                ctx.bot
                    .send_message(
                        q.message.as_ref().unwrap().chat().id,
                        t_lang(
                            ctx.localization,
                            "error-deleting-ingredients",
                            language_code.as_deref(),
                        ),
                    )
                    .await?;
                return Ok(());
            }
        }

        // Show success message
        let success_message = format!(
            "✅ **{}**\n\n{}",
            t_lang(
                ctx.localization,
                "ingredients-updated",
                language_code.as_deref()
            ),
            t_lang(
                ctx.localization,
                "ingredients-updated-help",
                language_code.as_deref()
            )
        );

        let keyboard =
            create_post_confirmation_keyboard(language_code.as_deref(), ctx.localization);

        // Update the original message
        match ctx
            .bot
            .edit_message_text(
                q.message.as_ref().unwrap().chat().id,
                q.message.as_ref().unwrap().id(),
                success_message,
            )
            .reply_markup(keyboard)
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
    } else {
        // No changes made
        let no_changes_message = t_lang(
            ctx.localization,
            "no-changes-made",
            language_code.as_deref(),
        );
        ctx.bot
            .send_message(q.message.as_ref().unwrap().chat().id, no_changes_message)
            .await?;
    }

    // End the dialogue
    dialogue.exit().await?;

    Ok(())
}

/// Handle cancel button for saved ingredients editing
async fn handle_cancel_saved_ingredients_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: &Option<String>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: Arc<PgPool>,
) -> Result<()> {
    // Get current dialogue state to access recipe information
    let dialogue_state = dialogue.get().await?;
    if let Some(RecipeDialogueState::EditingSavedIngredients {
        recipe_id,
        message_id,
        ..
    }) = dialogue_state
    {
        // Fetch recipe details and ingredients from database
        let recipe = match crate::db::read_recipe_with_name(&pool, recipe_id).await? {
            Some(recipe) => recipe,
            None => {
                // Recipe not found, just exit dialogue
                dialogue.exit().await?;
                return Ok(());
            }
        };

        let ingredients = crate::db::get_recipe_ingredients(&pool, recipe_id).await?;

        // Create normal recipe details message
        let recipe_details_message = format!(
            "📖 **{}**\n\n📅 {}\n\n{}",
            recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe"),
            recipe.created_at.format("%B %d, %Y at %H:%M"),
            if ingredients.is_empty() {
                t_lang(
                    localization,
                    "no-ingredients-found",
                    language_code.as_deref(),
                )
            } else {
                format_database_ingredients_list(
                    &ingredients,
                    language_code.as_deref(),
                    localization,
                )
            }
        );

        // Create normal recipe details keyboard
        let keyboard =
            create_recipe_details_keyboard(recipe_id, language_code.as_deref(), localization);

        // Edit the original message to show normal recipe details
        if let Some(message_id) = message_id {
            match bot
                .edit_message_text(
                    q.message.as_ref().unwrap().chat().id,
                    teloxide::types::MessageId(message_id),
                    recipe_details_message,
                )
                .reply_markup(keyboard)
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    error_logging::log_internal_error(
                        &e,
                        "callback_handler",
                        "Failed to edit message when canceling ingredient editing",
                        Some(q.from.id.0 as i64),
                    );
                }
            }
        }
    }

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}

/// Handle add ingredient button in editing saved ingredients state
async fn handle_add_ingredient_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: &Option<String>,
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
        bot.send_message(
            q.message.as_ref().unwrap().chat().id,
            t_lang(
                localization,
                "add-ingredient-prompt",
                language_code.as_deref(),
            ),
        )
        .await?;

        // Transition to adding ingredient state
        dialogue
            .update(RecipeDialogueState::AddingIngredientToSavedRecipe {
                recipe_id,
                original_ingredients,
                current_matches,
                language_code: language_code.clone(),
                message_id,
            })
            .await?;
    }

    Ok(())
}