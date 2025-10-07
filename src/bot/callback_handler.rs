//! Callback Handler module for processing inline keyboard callback queries

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{debug, error};

// Import localization
use crate::localization::{init_localization, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use super::ui_builder::{
    create_ingredient_review_keyboard, create_recipes_pagination_keyboard, format_ingredients_list,
};

// Import database functions
use crate::db::get_user_recipes_paginated;

/// Handle callback queries from inline keyboards
pub async fn callback_handler(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    _pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
) -> Result<()> {
    // Initialize localization for this thread
    tracing::debug!("About to initialize localization in callback_handler");
    init_localization()?;
    tracing::debug!("Localization initialized successfully in callback_handler");

    debug!(user_id = %q.from.id, "Received callback query from user");

    // Check dialogue state
    let dialogue_state = dialogue.get().await?;
    debug!(user_id = %q.from.id, dialogue_state = ?dialogue_state, "Retrieved dialogue state");

    match dialogue_state {
        Some(RecipeDialogueState::ReviewIngredients {
            recipe_name,
            mut ingredients,
            language_code: dialogue_lang_code,
            message_id,
            extracted_text,
        }) => {
            let data = q.data.as_deref().unwrap_or("");
            if let Some(msg) = &q.message {
                if data.starts_with("edit_") {
                    // Handle edit button - transition to editing state
                    let index: usize = data.strip_prefix("edit_").unwrap().parse().unwrap_or(0);
                    if index < ingredients.len() {
                        let ingredient = &ingredients[index];
                        let edit_prompt = format!(
                            "✏️ {}\n\n{}: **{} {}**\n\n{}",
                            t_lang("edit-ingredient-prompt", dialogue_lang_code.as_deref()),
                            t_lang("current-ingredient", dialogue_lang_code.as_deref()),
                            ingredient.quantity,
                            ingredient.measurement.as_deref().unwrap_or(""),
                            ingredient.ingredient_name
                        );
                        bot.send_message(msg.chat().id, edit_prompt).await?;

                        // Transition to editing state
                        dialogue
                            .update(RecipeDialogueState::EditingIngredient {
                                recipe_name: recipe_name.clone(),
                                ingredients: ingredients.clone(),
                                editing_index: index,
                                language_code: dialogue_lang_code.clone(),
                                message_id,
                                extracted_text: extracted_text.clone(),
                            })
                            .await?;
                    }
                } else if data.starts_with("delete_") {
                    // Handle delete button
                    let index: usize = data.strip_prefix("delete_").unwrap().parse().unwrap_or(0);

                    if index < ingredients.len() {
                        ingredients.remove(index);

                        // Check if all ingredients were deleted
                        if ingredients.is_empty() {
                            // All ingredients deleted - inform user and provide options
                            let empty_message = format!(
                                "🗑️ **{}**\n\n{}\n\n{}",
                                t_lang("review-title", dialogue_lang_code.as_deref()),
                                t_lang("review-no-ingredients", dialogue_lang_code.as_deref()),
                                t_lang("review-no-ingredients-help", dialogue_lang_code.as_deref())
                            );

                            let keyboard = vec![vec![
                                teloxide::types::InlineKeyboardButton::callback(
                                    t_lang("review-add-more", dialogue_lang_code.as_deref()),
                                    "add_more",
                                ),
                                teloxide::types::InlineKeyboardButton::callback(
                                    t_lang("cancel", dialogue_lang_code.as_deref()),
                                    "cancel_empty",
                                ),
                            ]];

                            // Edit the original message
                            match bot
                                .edit_message_text(msg.chat().id, msg.id(), empty_message)
                                .reply_markup(teloxide::types::InlineKeyboardMarkup::new(keyboard))
                                .await
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    error!(user_id = %q.from.id, error = %e, "Failed to edit message for empty ingredients")
                                }
                            }
                        } else {
                            // Update the message with remaining ingredients
                            let review_message = format!(
                                "📝 **{}**\n\n{}\n\n{}",
                                t_lang("review-title", dialogue_lang_code.as_deref()),
                                t_lang("review-description", dialogue_lang_code.as_deref()),
                                format_ingredients_list(
                                    &ingredients,
                                    dialogue_lang_code.as_deref()
                                )
                            );

                            let keyboard = create_ingredient_review_keyboard(
                                &ingredients,
                                dialogue_lang_code.as_deref(),
                            );

                            // Edit the original message
                            match bot
                                .edit_message_text(msg.chat().id, msg.id(), review_message)
                                .reply_markup(keyboard)
                                .await
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    error!(user_id = %q.from.id, error = %e, "Failed to edit message after ingredient deletion")
                                }
                            }
                        }

                        // Update dialogue state with modified ingredients
                        match dialogue
                            .update(RecipeDialogueState::ReviewIngredients {
                                recipe_name: recipe_name.clone(),
                                ingredients: ingredients.clone(),
                                language_code: dialogue_lang_code.clone(),
                                message_id,
                                extracted_text: extracted_text.clone(),
                            })
                            .await
                        {
                            Ok(_) => (),
                            Err(e) => {
                                error!(user_id = %q.from.id, error = %e, "Failed to update dialogue state after deletion")
                            }
                        }
                    } else {
                        // Invalid index - ignore silently
                    }
                } else if data == "confirm" {
                    // Handle confirm button - proceed to recipe name input
                    let recipe_name_prompt = format!(
                        "🏷️ **{}**\n\n{}",
                        t_lang("recipe-name-prompt", dialogue_lang_code.as_deref()),
                        t_lang("recipe-name-prompt-hint", dialogue_lang_code.as_deref())
                    );

                    bot.send_message(msg.chat().id, recipe_name_prompt).await?;

                    // Transition to waiting for recipe name after confirmation
                    dialogue
                        .update(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                            ingredients,
                            language_code: dialogue_lang_code,
                            extracted_text,
                        })
                        .await?;
                } else if data == "add_more" {
                    // Handle add more ingredients - reset to start state to allow new image
                    bot.send_message(
                        msg.chat().id,
                        t_lang(
                            "review-add-more-instructions",
                            dialogue_lang_code.as_deref(),
                        ),
                    )
                    .await?;

                    // Reset dialogue to start state
                    dialogue.update(RecipeDialogueState::Start).await?;
                } else if data == "cancel_review" {
                    // Handle cancel button - end dialogue without saving
                    bot.send_message(
                        msg.chat().id,
                        t_lang("review-cancelled", dialogue_lang_code.as_deref()),
                    )
                    .await?;

                    // End the dialogue
                    dialogue.exit().await?;
                }
            }
        }
        _ => {
            // Handle recipes-related callbacks (not dependent on dialogue state)
            let data = q.data.as_deref().unwrap_or("");
            if let Some(msg) = &q.message {
                if data.starts_with("select_recipe:") {
                    // Handle recipe selection
                    let recipe_name = data.strip_prefix("select_recipe:").unwrap_or("");
                    handle_recipe_selection(&bot, msg, recipe_name, &q.from.language_code).await?;
                } else if data.starts_with("page:") {
                    // Handle pagination
                    let page_str = data.strip_prefix("page:").unwrap_or("0");
                    let page: usize = page_str.parse().unwrap_or(0);
                    handle_recipes_pagination(
                        &bot,
                        msg,
                        page,
                        _pool.clone(),
                        &q.from.language_code,
                    )
                    .await?;
                }
            }
        }
    }

    // Answer the callback query to remove the loading state
    bot.answer_callback_query(q.id).await?;

    Ok(())
}

/// Handle recipe selection callback
async fn handle_recipe_selection(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    recipe_name: &str,
    language_code: &Option<String>,
) -> Result<()> {
    debug!(recipe_name = %recipe_name, "Handling recipe selection");

    // For now, just send a placeholder message
    // TODO: Implement actual recipe details display
    let message = format!(
        "📖 **{}**\n\n{}",
        recipe_name,
        t_lang("recipe-details-coming-soon", language_code.as_deref())
    );

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    bot.send_message(chat_id, message).await?;

    Ok(())
}

/// Handle recipes pagination callback
async fn handle_recipes_pagination(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    page: usize,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
) -> Result<()> {
    debug!(page = %page, "Handling recipes pagination");

    // Extract chat id from the message
    let (chat_id, message_id) = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => (msg.chat.id, msg.id),
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Calculate offset
    let limit = 5i64;
    let offset = (page as i64) * limit;

    // Get paginated recipes
    let (recipes, total_count) =
        get_user_recipes_paginated(&pool, chat_id.0, limit, offset).await?;

    if recipes.is_empty() {
        // This shouldn't happen in normal pagination, but handle gracefully
        let message = t_lang("no-recipes-found", language_code.as_deref());
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Create updated message text
    let recipes_message = format!(
        "📚 **{}**\n\n{}",
        t_lang("your-recipes", language_code.as_deref()),
        t_lang("select-recipe", language_code.as_deref())
    );

    // Create updated keyboard
    let keyboard = create_recipes_pagination_keyboard(
        &recipes,
        page,
        total_count,
        limit,
        language_code.as_deref(),
    );

    // Edit the original message
    bot.edit_message_text(chat_id, message_id, recipes_message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}
