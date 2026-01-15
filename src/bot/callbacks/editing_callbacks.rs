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
    create_ingredient_review_keyboard, create_recipe_details_keyboard, format_ingredients_list,
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
///
/// This function implements the same "focused editing interface" approach as the initial recipe editing:
/// - Replaces the full recipe display with a clean editing prompt to eliminate inactive button confusion
/// - Only shows a cancel button during editing for clarity
/// - Tracks the original message ID to restore the recipe display after editing/canceling
/// - Provides consistent UX across both initial recipe creation and saved recipe editing workflows
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
    let current_matches =
        current_matches_slice.expect("Current matches slice should be provided for edit callback");

    let index: usize = data
        .strip_prefix("edit_")
        .expect("Edit callback data should start with 'edit_'")
        .parse()
        .unwrap_or(0);
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
            "✏️ {}\n\n{}: **{} {} {}**\n\n{}",
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
            ingredient.quantity,
            ingredient.measurement.as_deref().unwrap_or(""),
            ingredient.ingredient_name,
            t_lang(
                ctx.localization,
                "edit-ingredient-instruction",
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
                    .expect("Callback query should have a message")
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
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
                    Some(q.from.id.0 as i64),
                );
                // Fallback: send new message if editing fails
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
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
                language_code: language_code.clone(),
                message_id,
                original_message_id: Some(
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .id()
                        .0,
                ),
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
    let current_matches =
        current_matches.expect("Current matches should be provided for delete callback");

    let index: usize = data
        .strip_prefix("delete_")
        .expect("Delete callback data should start with 'delete_'")
        .parse()
        .unwrap_or(0);

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
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .chat()
                        .id,
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .id(),
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
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .chat()
                        .id,
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .id(),
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

    let current_matches = current_matches_slice
        .expect("Current matches slice should be provided for confirm callback");
    let pool = pool.expect("Database pool should be provided for confirm callback");

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
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
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
                            q.message
                                .as_ref()
                                .expect("Callback query should have a message")
                                .chat()
                                .id,
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
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
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
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
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

        // Fetch updated recipe details and ingredients
        let recipe = match crate::db::read_recipe_with_name(pool, recipe_id).await {
            Ok(Some(recipe)) => recipe,
            Ok(None) => {
                error_logging::log_internal_error(
                    &anyhow::anyhow!("Recipe not found"),
                    "handle_confirm_saved_ingredients_button",
                    "Recipe not found after confirmation",
                    Some(q.from.id.0 as i64),
                );
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
                        t_lang(
                            ctx.localization,
                            "error-recipe-not-found",
                            language_code.as_deref(),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            Err(e) => {
                error_logging::log_database_error(
                    &e,
                    "read_recipe_with_name",
                    Some(q.from.id.0 as i64),
                    Some(&[("recipe_id", &recipe_id.to_string())]),
                );
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
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

        let updated_ingredients = crate::db::get_recipe_ingredients(pool, recipe_id).await?;
        let updated_matches =
            crate::ingredient_editing::ingredients_to_measurement_matches(&updated_ingredients);

        // Show the updated recipe details
        let recipe_name = recipe
            .recipe_name
            .unwrap_or_else(|| "Unnamed Recipe".to_string());
        let recipe_message = format!(
            "📝 **{}**\n\n{}",
            recipe_name,
            crate::bot::format_ingredients_list(
                &updated_matches,
                language_code.as_deref(),
                ctx.localization
            )
        );

        let keyboard =
            create_recipe_details_keyboard(recipe_id, language_code.as_deref(), ctx.localization);

        // Update the message to show the updated recipe
        match ctx
            .bot
            .edit_message_text(
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .id(),
                recipe_message,
            )
            .reply_markup(keyboard)
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_confirm_saved_ingredients_button",
                    "Failed to update message with recipe details after confirmation",
                    Some(q.from.id.0 as i64),
                );
            }
        }
    } else {
        // No changes made - still show the recipe details
        let recipe = match crate::db::read_recipe_with_name(pool, recipe_id).await {
            Ok(Some(recipe)) => recipe,
            Ok(None) => {
                error_logging::log_internal_error(
                    &anyhow::anyhow!("Recipe not found"),
                    "handle_confirm_saved_ingredients_button",
                    "Recipe not found after confirmation (no changes)",
                    Some(q.from.id.0 as i64),
                );
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
                        t_lang(
                            ctx.localization,
                            "error-recipe-not-found",
                            language_code.as_deref(),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            Err(e) => {
                error_logging::log_database_error(
                    &e,
                    "read_recipe_with_name",
                    Some(q.from.id.0 as i64),
                    Some(&[("recipe_id", &recipe_id.to_string())]),
                );
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .expect("Callback query should have a message")
                            .chat()
                            .id,
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

        let ingredients = crate::db::get_recipe_ingredients(pool, recipe_id).await?;
        let matches = crate::ingredient_editing::ingredients_to_measurement_matches(&ingredients);

        let recipe_name = recipe
            .recipe_name
            .unwrap_or_else(|| "Unnamed Recipe".to_string());
        let recipe_message = format!(
            "📝 **{}**\n\n{}",
            recipe_name,
            crate::bot::format_ingredients_list(
                &matches,
                language_code.as_deref(),
                ctx.localization
            )
        );

        let keyboard =
            create_recipe_details_keyboard(recipe_id, language_code.as_deref(), ctx.localization);

        // Update the message to show the recipe details
        match ctx
            .bot
            .edit_message_text(
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .expect("Callback query should have a message")
                    .id(),
                recipe_message,
            )
            .reply_markup(keyboard)
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "handle_confirm_saved_ingredients_button",
                    "Failed to update message with recipe details after confirmation (no changes)",
                    Some(q.from.id.0 as i64),
                );
            }
        }
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

        // Convert ingredients to measurement matches for display
        let measurement_matches =
            crate::ingredient_editing::ingredients_to_measurement_matches(&ingredients);

        // Create the recipe details message
        let recipe_name = recipe
            .recipe_name
            .unwrap_or_else(|| "Unnamed Recipe".to_string());
        let recipe_message = format!(
            "📝 **{}**\n\n{}",
            recipe_name,
            crate::bot::format_ingredients_list(
                &measurement_matches,
                language_code.as_deref(),
                localization
            )
        );

        let keyboard =
            create_recipe_details_keyboard(recipe_id, language_code.as_deref(), localization);

        // Edit the editing message back to the recipe details
        if let Some(message_id) = message_id {
            match bot
                .edit_message_text(
                    q.message
                        .as_ref()
                        .expect("Callback query should have a message")
                        .chat()
                        .id,
                    teloxide::types::MessageId(message_id),
                    recipe_message,
                )
                .reply_markup(keyboard)
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    error_logging::log_internal_error(
                        &e,
                        "handle_cancel_saved_ingredients_button",
                        "Failed to edit message back to recipe details when canceling ingredient editing",
                        Some(q.from.id.0 as i64),
                    );
                    // Fallback: delete the message
                    let _ = bot
                        .delete_message(
                            q.message
                                .as_ref()
                                .expect("Callback query should have a message")
                                .chat()
                                .id,
                            teloxide::types::MessageId(message_id),
                        )
                        .await;
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
            q.message
                .as_ref()
                .expect("Callback query should have a message")
                .chat()
                .id,
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
