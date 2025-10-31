//! Callback Handler module for processing inline keyboard callback queries

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage};
use tracing::{debug, error};

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import text processing types
use crate::text_processing::MeasurementMatch;

// Import UI builder functions
use super::ui_builder::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard,
    create_recipe_details_keyboard, create_recipe_instances_keyboard,
    create_recipes_pagination_keyboard, format_database_ingredients_list, format_ingredients_list,
};

// Import HandlerContext
use super::HandlerContext;

// Import database functions
use crate::db::{get_recipes_by_name, get_user_recipes_paginated, read_recipe_with_name};

// Import dialogue manager functions
use super::dialogue_manager::save_ingredients_to_database;

// Import observability
use crate::observability;

/// Handle callback queries from inline keyboards
pub async fn callback_handler(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let span = crate::observability::telegram_span("callback_handler", Some(q.from.id.0 as i64));
    let _enter = span.enter();

    let start_time = std::time::Instant::now();

    // Check dialogue state
    let dialogue_state = dialogue.get().await?;
    debug!(user_id = %q.from.id, dialogue_state = ?dialogue_state, "Retrieved dialogue state");

    let data = q.data.as_deref().unwrap_or("");

    let result = match dialogue_state {
        Some(RecipeDialogueState::ReviewIngredients { .. }) => {
            handle_review_ingredients_callbacks(
                &bot,
                &q,
                data,
                pool.clone(),
                &dialogue,
                &localization,
            )
            .await
        }
        Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
            handle_editing_saved_ingredients_callbacks(
                &bot,
                &q,
                data,
                pool.clone(),
                &dialogue,
                &localization,
            )
            .await
        }
        _ => Ok(()), // No state-specific handling needed
    };

    // Handle general callbacks that work in any state
    if let Some(msg) = &q.message {
        if data.starts_with("select_recipe:") {
            handle_recipe_selection(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("recipe_instance:") {
            handle_recipe_instance_selection(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("recipe_action:") {
            handle_recipe_action(
                &bot,
                msg,
                data,
                pool.clone(),
                &dialogue,
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data == "back_to_recipes" {
            handle_back_to_recipes(
                &bot,
                msg,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("confirm_delete_recipe")
            || data.starts_with("cancel_delete_recipe")
        {
            handle_delete_recipe_confirmation(
                &bot,
                msg,
                data,
                pool.clone(),
                &q.from.language_code,
                &localization,
            )
            .await?;
        } else if data.starts_with("page:") {
            handle_recipes_pagination(&bot, msg, data, pool, &q.from.language_code, &localization)
                .await?;
        } else if data.starts_with("workflow_") {
            handle_workflow_button(&bot, &q, data, &pool, &dialogue, &localization).await?;
        }
    }

    // Answer the callback query to remove the loading state
    bot.answer_callback_query(q.id).await?;

    let duration = start_time.elapsed();
    observability::record_request_metrics("telegram_callback", 200, duration);

    result
}

/// Handle callbacks when in ReviewIngredients dialogue state
async fn handle_review_ingredients_callbacks(
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
                handle_workflow_button(bot, q, data, &pool, dialogue, localization).await?;
            }
        }
    }

    Ok(())
}

/// Handle callbacks when in EditingSavedIngredients dialogue state
async fn handle_editing_saved_ingredients_callbacks(
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

/// Handle recipe selection callback
async fn handle_recipe_selection(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract recipe name from callback data (format: "select_recipe:Recipe Name")
    let recipe_name = data.strip_prefix("select_recipe:").unwrap_or("");
    debug!(recipe_name = %recipe_name, "Handling recipe selection");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Query for all recipes with this name for the user
    let recipes = get_recipes_by_name(&pool, chat_id.0, recipe_name).await?;

    match recipes.len() {
        0 => {
            // This shouldn't happen if the recipe exists in the list, but handle gracefully
            let message = format!(
                "❌ **{}**\n\n{}",
                t_lang(localization, "recipe-not-found", language_code.as_deref()),
                t_lang(
                    localization,
                    "recipe-not-found-help",
                    language_code.as_deref()
                )
            );
            bot.send_message(chat_id, message).await?;
        }
        1 => {
            // Single recipe - show details directly
            let recipe = &recipes[0];
            let ingredients = crate::db::get_recipe_ingredients(&pool, recipe.id).await?;

            let message = format!(
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

            let keyboard =
                create_recipe_details_keyboard(recipe.id, language_code.as_deref(), localization);

            bot.send_message(chat_id, message)
                .reply_markup(keyboard)
                .await?;
        }
        _ => {
            // Multiple recipes with same name - show disambiguation UI
            let message = format!(
                "📚 **{}**\n\n{}",
                recipe_name,
                t_lang(
                    localization,
                    "select-recipe-instance",
                    language_code.as_deref()
                )
            );

            // Fetch ingredients for each recipe to show previews
            let mut recipe_data = Vec::new();
            for recipe in &recipes {
                let ingredients = crate::db::get_recipe_ingredients(&pool, recipe.id).await?;
                recipe_data.push((recipe.clone(), ingredients));
            }

            let keyboard = create_recipe_instances_keyboard(
                &recipe_data,
                language_code.as_deref(),
                localization,
            );

            bot.send_message(chat_id, message)
                .reply_markup(keyboard)
                .await?;
        }
    }

    Ok(())
}

/// Handle recipe instance selection callback (when user selects a specific recipe from duplicates)
async fn handle_recipe_instance_selection(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract recipe ID from callback data (format: "recipe_instance:123")
    let recipe_id_str = data.strip_prefix("recipe_instance:").unwrap_or("");
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);
    debug!(recipe_id = %recipe_id, "Handling recipe instance selection");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = read_recipe_with_name(&pool, recipe_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Recipe not found"))?;
    let ingredients = crate::db::get_recipe_ingredients(&pool, recipe_id).await?;

    let message = format!(
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
            format_database_ingredients_list(&ingredients, language_code.as_deref(), localization)
        }
    );

    let keyboard =
        create_recipe_details_keyboard(recipe_id, language_code.as_deref(), localization);

    bot.send_message(chat_id, message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle recipe action callbacks (rename, delete)
async fn handle_recipe_action(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Parse callback data (format: "recipe_action:{action}:{recipe_id}")
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() < 3 || parts[0] != "recipe_action" {
        debug!(data = %data, "Invalid recipe action callback format");
        return Ok(());
    }

    let action = parts[1];
    let recipe_id_str = parts[2];
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);

    debug!(action = %action, recipe_id = %recipe_id, "Handling recipe action");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    match action {
        "rename" => {
            // Get current recipe details
            if let Ok(Some(recipe)) = crate::db::read_recipe_with_name(&pool, recipe_id).await {
                let current_name = recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe");

                let message = format!(
                    "🏷️ **{}**\n\n{}: **{}**\n\n{}",
                    t_lang(
                        localization,
                        "rename-recipe-title",
                        language_code.as_deref()
                    ),
                    t_lang(
                        localization,
                        "current-recipe-name",
                        language_code.as_deref()
                    ),
                    current_name,
                    t_lang(
                        localization,
                        "rename-recipe-instructions",
                        language_code.as_deref()
                    )
                );
                bot.send_message(chat_id, message).await?;

                // Transition to renaming state
                dialogue
                    .update(RecipeDialogueState::RenamingRecipe {
                        recipe_id,
                        current_name: current_name.to_string(),
                        language_code: language_code.clone(),
                    })
                    .await?;
            } else {
                let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
                bot.send_message(chat_id, message).await?;
            }
        }
        "delete" => {
            let message = format!(
                "🗑️ **{}**\n\n{}",
                t_lang(
                    localization,
                    "delete-recipe-title",
                    language_code.as_deref()
                ),
                t_lang(
                    localization,
                    "delete-recipe-confirmation",
                    language_code.as_deref()
                )
            );

            let keyboard = vec![vec![
                teloxide::types::InlineKeyboardButton::callback(
                    format!(
                        "✅ {}",
                        t_lang(localization, "confirm", language_code.as_deref())
                    ),
                    format!("confirm_delete_recipe:{}", recipe_id),
                ),
                teloxide::types::InlineKeyboardButton::callback(
                    format!(
                        "❌ {}",
                        t_lang(localization, "cancel", language_code.as_deref())
                    ),
                    format!("cancel_delete_recipe:{}", recipe_id),
                ),
            ]];

            bot.send_message(chat_id, message)
                .reply_markup(teloxide::types::InlineKeyboardMarkup::new(keyboard))
                .await?;
        }
        "edit_ingredients" => {
            handle_edit_ingredients_callback(
                bot,
                msg,
                recipe_id,
                pool,
                dialogue,
                language_code,
                localization,
            )
            .await?;
        }
        "statistics" => {
            handle_recipe_statistics(bot, msg, recipe_id, pool, language_code, localization)
                .await?;
        }
        _ => {
            debug!(action = %action, "Unknown recipe action");
        }
    }

    Ok(())
}

/// Handle back to recipes list callback
async fn handle_back_to_recipes(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    _pool: Arc<PgPool>,
    _language_code: &Option<String>,
    _localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!("Handling back to recipes - removing message");

    // Extract chat id and message id from the message
    let (chat_id, message_id) = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => (msg.chat.id, msg.id),
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't delete inaccessible messages
            return Ok(());
        }
    };

    // Simply delete the message - no database queries, no content regeneration
    if let Err(e) = bot.delete_message(chat_id, message_id).await {
        debug!("Failed to delete message: {:?}", e);
        // If deletion fails, just ignore - the message might already be deleted or inaccessible
    }

    Ok(())
}

/// Handle recipe statistics display
async fn handle_recipe_statistics(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    recipe_id: i64,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(recipe_id = %recipe_id, "Handling recipe statistics");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = match crate::db::read_recipe_with_name(&pool, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    // Get recipe ingredients
    let ingredients = crate::db::get_recipe_ingredients(&pool, recipe_id).await?;
    let ingredient_count = ingredients.len() as i64;

    // Get user statistics
    let user_stats = crate::db::get_user_recipe_statistics(&pool, chat_id.0).await?;

    // Format statistics message
    let recipe_name = recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe");

    let mut stats_message = format!(
        "📊 **{}: {}**\n\n",
        t_lang(
            localization,
            "recipe-statistics-title",
            language_code.as_deref()
        ),
        recipe_name
    );

    // Recipe-specific stats
    stats_message.push_str(&format!(
        "📝 **{}**\n",
        t_lang(localization, "recipe-details", language_code.as_deref())
    ));
    stats_message.push_str(&format!(
        "• {}: {}\n",
        t_lang(localization, "ingredients-count", language_code.as_deref()),
        ingredient_count
    ));
    stats_message.push_str(&format!(
        "• {}: {}\n",
        t_lang(localization, "created-date", language_code.as_deref()),
        recipe.created_at.format("%B %d, %Y at %H:%M")
    ));

    // User overview stats
    stats_message.push_str(&format!(
        "\n📈 **{}**\n",
        t_lang(localization, "your-statistics", language_code.as_deref())
    ));
    stats_message.push_str(&format!(
        "• {}: {}\n",
        t_lang(localization, "total-recipes", language_code.as_deref()),
        user_stats.total_recipes
    ));
    stats_message.push_str(&format!(
        "• {}: {}\n",
        t_lang(localization, "total-ingredients", language_code.as_deref()),
        user_stats.total_ingredients
    ));
    stats_message.push_str(&format!(
        "• {}: {:.1}\n",
        t_lang(
            localization,
            "avg-ingredients-per-recipe",
            language_code.as_deref()
        ),
        user_stats.average_ingredients_per_recipe
    ));

    // Recent activity
    if user_stats.recipes_created_today > 0 || user_stats.recipes_created_this_week > 0 {
        stats_message.push_str(&format!(
            "\n🕐 **{}**\n",
            t_lang(localization, "recent-activity", language_code.as_deref())
        ));

        if user_stats.recipes_created_today > 0 {
            stats_message.push_str(&format!(
                "• {}: {}\n",
                t_lang(localization, "recipes-today", language_code.as_deref()),
                user_stats.recipes_created_today
            ));
        }

        if user_stats.recipes_created_this_week > 0 {
            stats_message.push_str(&format!(
                "• {}: {}\n",
                t_lang(localization, "recipes-this-week", language_code.as_deref()),
                user_stats.recipes_created_this_week
            ));
        }
    }

    // Most common units (if any)
    if !user_stats.most_common_units.is_empty() {
        stats_message.push_str(&format!(
            "\n🏷️ **{}**\n",
            t_lang(localization, "favorite-units", language_code.as_deref())
        ));

        for (unit, count) in user_stats.most_common_units.iter().take(3) {
            stats_message.push_str(&format!("• {} ({})\n", unit, count));
        }
    }

    // Add back button
    let keyboard = vec![vec![InlineKeyboardButton::callback(
        format!(
            "⬅️ {}",
            t_lang(localization, "back-to-recipe", language_code.as_deref())
        ),
        format!("select_recipe:{}", recipe_name),
    )]];

    bot.send_message(chat_id, stats_message)
        .reply_markup(InlineKeyboardMarkup::new(keyboard))
        .await?;

    Ok(())
}
async fn handle_recipes_pagination(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let page_str = data.strip_prefix("page:").unwrap_or("0");
    let page: usize = page_str.parse().unwrap_or(0);
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
        let message = t_lang(localization, "no-recipes-found", language_code.as_deref());
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Create updated message text
    let recipes_message = format!(
        "📚 **{}**\n\n{}",
        t_lang(localization, "your-recipes", language_code.as_deref()),
        t_lang(localization, "select-recipe", language_code.as_deref())
    );

    // Create updated keyboard
    let keyboard = create_recipes_pagination_keyboard(
        &recipes,
        page,
        total_count,
        limit,
        language_code.as_deref(),
        localization,
    );

    // Edit the original message
    bot.edit_message_text(chat_id, message_id, recipes_message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Handle list recipes workflow callback
async fn handle_list_recipes(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!("Handling list recipes workflow");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get user's recipes (first page)
    let limit = 5i64;
    let offset = 0i64;
    let (recipes, total_count) =
        get_user_recipes_paginated(&pool, chat_id.0, limit, offset).await?;

    if recipes.is_empty() {
        // No recipes found
        let message = format!(
            "📚 **{}**\n\n{}",
            t_lang(localization, "your-recipes", language_code.as_deref()),
            t_lang(
                localization,
                "no-recipes-suggestion",
                language_code.as_deref()
            )
        );
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Create message text
    let recipes_message = format!(
        "📚 **{}**\n\n{}",
        t_lang(localization, "your-recipes", language_code.as_deref()),
        t_lang(localization, "select-recipe", language_code.as_deref())
    );

    // Create keyboard
    let keyboard = create_recipes_pagination_keyboard(
        &recipes,
        0, // current page
        total_count,
        limit,
        language_code.as_deref(),
        localization,
    );

    // Send the message with keyboard
    bot.send_message(chat_id, recipes_message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

/// Parameters for review ingredients state operations
#[derive(Debug)]
struct ReviewIngredientsParams<'a> {
    ctx: &'a super::HandlerContext<'a>,
    q: &'a teloxide::types::CallbackQuery,
    data: Option<&'a str>,
    ingredients: Option<&'a mut Vec<crate::text_processing::MeasurementMatch>>,
    ingredients_slice: Option<&'a [crate::text_processing::MeasurementMatch]>,
    recipe_name: &'a str,
    dialogue_lang_code: &'a Option<String>,
    message_id: Option<i32>,
    extracted_text: &'a str,
    recipe_name_from_caption: Option<&'a Option<String>>,
    dialogue: &'a RecipeDialogue,
    pool: Option<&'a Arc<PgPool>>,
}

/// Parameters for saved ingredients editing operations
#[derive(Debug)]
struct SavedIngredientsParams<'a> {
    ctx: &'a super::HandlerContext<'a>,
    q: &'a teloxide::types::CallbackQuery,
    data: Option<&'a str>,
    current_matches: Option<&'a mut Vec<MeasurementMatch>>,
    current_matches_slice: Option<&'a [MeasurementMatch]>,
    recipe_id: i64,
    original_ingredients: &'a [crate::db::Ingredient],
    language_code: &'a Option<String>,
    message_id: Option<i32>,
    dialogue: &'a RecipeDialogue,
    pool: Option<&'a Arc<PgPool>>,
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

/// Handle delete recipe confirmation callbacks
async fn handle_delete_recipe_confirmation(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(data = %data, "Handling delete recipe confirmation");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Parse callback data (format: "confirm_delete_recipe:{recipe_id}" or "cancel_delete_recipe:{recipe_id}")
    let parts: Vec<&str> = data.split(':').collect();
    let action = parts[0];
    let recipe_id_str = parts.get(1).unwrap_or(&"");
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);

    match action {
        "confirm_delete_recipe" => {
            // Attempt to delete the recipe
            match crate::db::delete_recipe(&pool, recipe_id).await {
                Ok(deleted) => {
                    if deleted {
                        let message = format!(
                            "🗑️ **{}**\n\n{}",
                            t_lang(localization, "recipe-deleted", language_code.as_deref()),
                            t_lang(
                                localization,
                                "recipe-deleted-help",
                                language_code.as_deref()
                            )
                        );
                        bot.send_message(chat_id, message).await?;
                    } else {
                        let message =
                            t_lang(localization, "recipe-not-found", language_code.as_deref());
                        bot.send_message(chat_id, message).await?;
                    }
                }
                Err(e) => {
                    error_logging::log_database_error(
                        &e,
                        "delete_recipe",
                        Some(chat_id.0),
                        Some(&[("recipe_id", &recipe_id.to_string())]),
                    );
                    let message = format!(
                        "❌ **{}**\n\n{}",
                        t_lang(
                            localization,
                            "error-deleting-recipe",
                            language_code.as_deref()
                        ),
                        t_lang(
                            localization,
                            "error-deleting-recipe-help",
                            language_code.as_deref()
                        )
                    );
                    bot.send_message(chat_id, message).await?;
                }
            }
        }
        "cancel_delete_recipe" => {
            let message = t_lang(localization, "delete-cancelled", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
        }
        _ => {}
    }

    Ok(())
}
async fn handle_workflow_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: &Arc<PgPool>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    match data {
        "workflow_add_another" => {
            // Record user engagement metric for workflow continuation
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::WorkflowContinue,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            bot.send_message(
                q.message.as_ref().unwrap().chat().id,
                t_lang(
                    localization,
                    "workflow-what-next",
                    q.from.language_code.as_deref(),
                ),
            )
            .await?;
            dialogue.update(RecipeDialogueState::Start).await?;
        }
        "workflow_list_recipes" => {
            // Record user engagement metric for recipe listing
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::RecipesCommand,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            handle_list_recipes(
                bot,
                q.message.as_ref().unwrap(),
                pool.clone(),
                &q.from.language_code,
                localization,
            )
            .await?;
        }
        "workflow_search_recipes" => {
            // Record user engagement metric for recipe search
            crate::observability::record_user_engagement_metrics(
                q.from.id.0 as i64,
                crate::observability::UserAction::RecipeSearch,
                None, // No session duration for individual actions
                q.from.language_code.as_deref(),
            );

            bot.send_message(
                q.message.as_ref().unwrap().chat().id,
                "🔍 Recipe search coming soon! For now, use the 'List My Recipes' button.",
            )
            .await?;
        }
        _ => {}
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

/// Handle edit ingredients callback for saved recipes
async fn handle_edit_ingredients_callback(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    recipe_id: i64,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(recipe_id = %recipe_id, "Handling edit ingredients callback");

    // Extract chat id from the message
    let chat_id = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = match crate::db::read_recipe_with_name(&pool, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    // Get current ingredients
    let original_ingredients = crate::db::get_recipe_ingredients(&pool, recipe_id).await?;
    if original_ingredients.is_empty() {
        let message = format!(
            "❌ **{}**\n\n{}",
            t_lang(
                localization,
                "no-ingredients-to-edit",
                language_code.as_deref()
            ),
            t_lang(
                localization,
                "no-ingredients-to-edit-help",
                language_code.as_deref()
            )
        );
        bot.send_message(chat_id, message).await?;
        return Ok(());
    }

    // Convert to measurement matches for editing
    let current_matches =
        crate::ingredient_editing::ingredients_to_measurement_matches(&original_ingredients);

    // Send editing interface
    let edit_message = format!(
        "✏️ **{}: {}**\n\n{}\n\n{}",
        t_lang(localization, "editing-recipe", language_code.as_deref()),
        recipe.recipe_name.as_deref().unwrap_or("Unnamed Recipe"),
        t_lang(
            localization,
            "editing-instructions",
            language_code.as_deref()
        ),
        format_ingredients_list(&current_matches, language_code.as_deref(), localization)
    );

    let keyboard =
        create_ingredient_review_keyboard(&current_matches, language_code.as_deref(), localization);

    let sent_message = bot
        .send_message(chat_id, edit_message)
        .reply_markup(keyboard)
        .await?;

    // Transition to editing state
    dialogue
        .update(RecipeDialogueState::EditingSavedIngredients {
            recipe_id,
            original_ingredients,
            current_matches,
            language_code: language_code.clone(),
            message_id: Some(sent_message.id.0 as i32),
        })
        .await?;

    Ok(())
}

/// Cache-enabled callback handler for improved performance
///
/// This version includes caching for database queries to reduce
/// database load and improve response times.
pub async fn callback_handler_with_cache(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    _cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    // For now, delegate to the original handler
    // TODO: Integrate caching into specific operations
    callback_handler(bot, q, pool, dialogue, localization).await
}
