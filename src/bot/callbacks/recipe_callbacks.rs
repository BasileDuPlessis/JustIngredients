//! Recipe callback handlers module
//!
//! This module contains all callback handlers related to recipe management operations,
//! including selection, deletion, statistics, and other recipe-related actions.

use crate::bot::html::send_html;
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage, User};
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use crate::bot::ui_builder::{
    create_ingredient_review_keyboard, create_recipe_details_keyboard,
    create_recipe_instances_keyboard, format_database_ingredients_list,
};

// Import database functions
use crate::bot::cleanup::send_transient;
use crate::bot::home::send_home;
use crate::db::{get_recipes_by_name, read_recipe_with_name};

/// Handle recipe selection callback
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
pub async fn handle_recipe_selection(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    // Recipe name from the button: plain, or a fingerprint for names too long for Telegram
    let recipe_name =
        crate::bot::recipe_selection::resolve_recipe_selection(&pool, telegram_id, data)
            .await?
            .unwrap_or_default();
    let recipe_name = recipe_name.as_str();
    debug!(recipe_name = %recipe_name, "Handling recipe selection");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Query for all recipes with this name for the user
    let recipes = get_recipes_by_name(&pool, telegram_id, recipe_name).await?;

    match recipes.len() {
        0 => {
            // This shouldn't happen if the recipe exists in the list, but handle gracefully
            let message = format!(
                "❌ {}\n\n{}",
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
            let ingredients =
                crate::db::get_recipe_ingredients(&pool, telegram_id, recipe.id).await?;

            let message = format!(
                "📖 {}\n\n📅 {}\n\n{}",
                recipe
                    .recipe_name
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("Unnamed Recipe"),
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
                "📚 {}\n\n{}",
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
                let ingredients =
                    crate::db::get_recipe_ingredients(&pool, telegram_id, recipe.id).await?;
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
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
pub async fn handle_recipe_instance_selection(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    // Extract recipe ID from callback data (format: "recipe_instance:123")
    let recipe_id_str = data.strip_prefix("recipe_instance:").unwrap_or("");
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);
    debug!(recipe_id = %recipe_id, "Handling recipe instance selection");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = read_recipe_with_name(&pool, telegram_id, recipe_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Recipe not found"))?;
    let ingredients = crate::db::get_recipe_ingredients(&pool, telegram_id, recipe_id).await?;

    let message = format!(
        "📖 {}\n\n📅 {}\n\n{}",
        recipe
            .recipe_name
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("Unnamed Recipe"),
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
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe accessed here.
pub async fn handle_recipe_action(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

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
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    match action {
        "rename" => {
            // Get current recipe details
            if let Ok(Some(recipe)) =
                crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await
            {
                let current_name = recipe
                    .recipe_name
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("Unnamed Recipe");

                let message = format!(
                    "🏷️ {}\n\n{}: {}\n\n{}",
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
                send_transient(bot, chat_id, message).await?;

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
            // Get the original message ID to include in callback data
            let original_message_id = match msg {
                MaybeInaccessibleMessage::Regular(msg) => Some(msg.id),
                MaybeInaccessibleMessage::Inaccessible(_) => None,
            };

            let message = format!(
                "🗑️ {}\n\n{}",
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
                    format!(
                        "confirm_delete_recipe:{}:{}",
                        recipe_id,
                        original_message_id.map(|id| id.0).unwrap_or(0)
                    ),
                ),
                teloxide::types::InlineKeyboardButton::callback(
                    format!(
                        "❌ {}",
                        t_lang(localization, "cancel", language_code.as_deref())
                    ),
                    format!(
                        "cancel_delete_recipe:{}:{}",
                        recipe_id,
                        original_message_id.map(|id| id.0).unwrap_or(0)
                    ),
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
                from,
                localization,
            )
            .await?;
        }
        "statistics" => {
            handle_recipe_statistics(bot, msg, recipe_id, pool, from, localization).await?;
        }
        _ => {
            debug!(action = %action, "Unknown recipe action");
        }
    }

    Ok(())
}

/// Handle recipe statistics display
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
pub async fn handle_recipe_statistics(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    recipe_id: i64,
    pool: Arc<PgPool>,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    debug!(recipe_id = %recipe_id, "Handling recipe statistics");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = match crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    // Get recipe ingredients
    let ingredients = crate::db::get_recipe_ingredients(&pool, telegram_id, recipe_id).await?;
    let ingredient_count = ingredients.len() as i64;

    // Get user statistics
    let user_stats = crate::db::get_user_recipe_statistics(&pool, telegram_id).await?;

    // Format statistics message
    let recipe_name = recipe
        .recipe_name
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("Unnamed Recipe");

    let mut stats_message = format!(
        "📊 {}: {}\n\n",
        t_lang(
            localization,
            "recipe-statistics-title",
            language_code.as_deref()
        ),
        recipe_name
    );

    // Recipe-specific stats
    stats_message.push_str(&format!(
        "📝 {}\n",
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
        "\n📈 {}\n",
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
            "\n🕐 {}\n",
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
            "\n🏷️ {}\n",
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
        crate::bot::recipe_selection::recipe_selection_callback(recipe_name),
    )]];

    bot.send_message(chat_id, stats_message)
        .reply_markup(InlineKeyboardMarkup::new(keyboard))
        .await?;

    Ok(())
}

/// Handle delete recipe confirmation callbacks
///
/// `from` is the sender of the callback query (`q.from`): only a recipe they own can be deleted.
pub async fn handle_delete_recipe_confirmation(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    data: &str,
    pool: Arc<PgPool>,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
    dialogue: &RecipeDialogue,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    debug!(data = %data, "Handling delete recipe confirmation");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Parse callback data (format: "confirm_delete_recipe:{recipe_id}:{original_message_id}" or "cancel_delete_recipe:{recipe_id}:{original_message_id}")
    let parts: Vec<&str> = data.split(':').collect();
    let action = parts[0];
    let recipe_id_str = parts.get(1).unwrap_or(&"");
    let recipe_id: i64 = recipe_id_str.parse().unwrap_or(0);
    let original_message_id: Option<i32> = parts
        .get(2)
        .and_then(|s| s.parse().ok())
        .filter(|&id| id != 0);

    match action {
        "confirm_delete_recipe" => {
            // Attempt to delete the recipe
            match crate::db::delete_recipe(&pool, telegram_id, recipe_id).await {
                Ok(deleted) => {
                    if deleted {
                        // Delete the confirmation message entirely
                        if let MaybeInaccessibleMessage::Regular(msg) = msg {
                            match bot.delete_message(chat_id, msg.id).await {
                                Ok(_) => (),
                                Err(e) => {
                                    crate::errors::error_logging::log_internal_error(
                                        &e,
                                        "handle_delete_recipe_confirmation",
                                        "Failed to delete confirmation message",
                                        Some(chat_id.0),
                                    );
                                }
                            }
                        }

                        // Also delete the original recipe message
                        if let Some(original_msg_id) = original_message_id {
                            match bot
                                .delete_message(
                                    chat_id,
                                    teloxide::types::MessageId(original_msg_id),
                                )
                                .await
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    crate::errors::error_logging::log_internal_error(
                                        &e,
                                        "handle_delete_recipe_confirmation",
                                        "Failed to delete original recipe message",
                                        Some(chat_id.0),
                                    );
                                }
                            }
                        }
                    } else {
                        // Recipe not found - show error and delete both messages
                        if let MaybeInaccessibleMessage::Regular(msg) = msg {
                            match bot.delete_message(chat_id, msg.id).await {
                                Ok(_) => (),
                                Err(e) => {
                                    crate::errors::error_logging::log_internal_error(
                                        &e,
                                        "handle_delete_recipe_confirmation",
                                        "Failed to delete confirmation message",
                                        Some(chat_id.0),
                                    );
                                }
                            }
                        }

                        // Also delete the original recipe message
                        if let Some(original_msg_id) = original_message_id {
                            match bot
                                .delete_message(
                                    chat_id,
                                    teloxide::types::MessageId(original_msg_id),
                                )
                                .await
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    crate::errors::error_logging::log_internal_error(
                                        &e,
                                        "handle_delete_recipe_confirmation",
                                        "Failed to delete original recipe message",
                                        Some(chat_id.0),
                                    );
                                }
                            }
                        }

                        let message =
                            t_lang(localization, "recipe-not-found", language_code.as_deref());
                        bot.send_message(chat_id, message).await?;
                    }
                }
                Err(e) => {
                    // Database error - show error and delete both messages
                    if let MaybeInaccessibleMessage::Regular(msg) = msg {
                        match bot.delete_message(chat_id, msg.id).await {
                            Ok(_) => (),
                            Err(e) => {
                                crate::errors::error_logging::log_internal_error(
                                    &e,
                                    "handle_delete_recipe_confirmation",
                                    "Failed to delete confirmation message",
                                    Some(chat_id.0),
                                );
                            }
                        }
                    }

                    // Also delete the original recipe message
                    if let Some(original_msg_id) = original_message_id {
                        match bot
                            .delete_message(chat_id, teloxide::types::MessageId(original_msg_id))
                            .await
                        {
                            Ok(_) => (),
                            Err(e) => {
                                crate::errors::error_logging::log_internal_error(
                                    &e,
                                    "handle_delete_recipe_confirmation",
                                    "Failed to delete original recipe message",
                                    Some(chat_id.0),
                                );
                            }
                        }
                    }

                    error_logging::log_database_error(
                        &e,
                        "delete_recipe",
                        Some(chat_id.0),
                        Some(&[("recipe_id", &recipe_id.to_string())]),
                    );
                    let message = format!(
                        "❌ {}\n\n{}",
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
            // Delete the confirmation message entirely
            if let MaybeInaccessibleMessage::Regular(msg) = msg {
                match bot.delete_message(chat_id, msg.id).await {
                    Ok(_) => (),
                    Err(e) => {
                        crate::errors::error_logging::log_internal_error(
                            &e,
                            "handle_delete_recipe_confirmation",
                            "Failed to delete confirmation message",
                            Some(chat_id.0),
                        );
                    }
                }
            }
            // The recipe card stays: nothing was deleted
        }
        _ => return Ok(()),
    }

    // A delete done in the middle of another action (a rename, a review) leaves that action
    // running: its home message would hide the prompts the action still waits on
    if !dialogue.get().await?.is_some_and(|state| state.is_action()) {
        send_home(bot, &pool, chat_id, localization, language_code.as_deref()).await;
    }
    Ok(())
}

/// Handle edit ingredients callback for saved recipes
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
async fn handle_edit_ingredients_callback(
    bot: &Bot,
    msg: &MaybeInaccessibleMessage,
    recipe_id: i64,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    from: &User,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let language_code = &from.language_code;
    let telegram_id = from.id.0 as i64;

    debug!(recipe_id = %recipe_id, "Handling edit ingredients callback");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details
    let recipe = match crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    // Get current ingredients
    let original_ingredients =
        crate::db::get_recipe_ingredients(&pool, telegram_id, recipe_id).await?;
    if original_ingredients.is_empty() {
        let message = format!(
            "❌ {}\n\n{}",
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
    let edit_message = crate::bot::ui_builder::render_saved_editing(
        Some(
            recipe
                .recipe_name
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("Unnamed Recipe"),
        ),
        &current_matches,
        language_code.as_deref(),
        localization,
    );

    let keyboard =
        create_ingredient_review_keyboard(&current_matches, language_code.as_deref(), localization);

    let sent_message = send_html(bot, chat_id, edit_message)
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
