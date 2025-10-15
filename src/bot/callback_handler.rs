//! Callback Handler module for processing inline keyboard callback queries

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::MaybeInaccessibleMessage;
use tracing::{debug, error};

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use super::ui_builder::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard,
    create_recipes_pagination_keyboard, format_ingredients_list,
};

// Import database functions
use crate::db::get_user_recipes_paginated;

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
            handle_review_ingredients_callbacks(&bot, &q, data, pool, &dialogue, &localization)
                .await
        }
        _ => handle_general_callbacks(&bot, &q, data, pool, &dialogue, &localization).await,
    };

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
                handle_edit_button(EditButtonParams {
                    bot,
                    q,
                    data,
                    ingredients: &ingredients,
                    recipe_name: &recipe_name,
                    dialogue_lang_code: &dialogue_lang_code,
                    message_id,
                    extracted_text: &extracted_text,
                    dialogue,
                    localization,
                })
                .await?;
            } else if data.starts_with("delete_") {
                handle_delete_button(DeleteButtonParams {
                    bot,
                    q,
                    data,
                    ingredients: &mut ingredients,
                    recipe_name: &recipe_name,
                    dialogue_lang_code: &dialogue_lang_code,
                    message_id,
                    extracted_text: &extracted_text,
                    recipe_name_from_caption: &recipe_name_from_caption,
                    dialogue,
                    localization,
                })
                .await?;
            } else if data == "confirm" {
                handle_confirm_button(ConfirmButtonParams {
                    bot,
                    q,
                    ingredients: &ingredients,
                    dialogue_lang_code: &dialogue_lang_code,
                    extracted_text: &extracted_text,
                    recipe_name_from_caption: &recipe_name_from_caption,
                    dialogue,
                    pool: &pool,
                    localization,
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

/// Handle callbacks that work in any dialogue state
async fn handle_general_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: Arc<PgPool>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    if let Some(msg) = &q.message {
        if data.starts_with("select_recipe:") {
            handle_recipe_selection(bot, msg, data, &q.from.language_code, localization).await?;
        } else if data.starts_with("page:") {
            handle_recipes_pagination(bot, msg, data, pool, &q.from.language_code, localization)
                .await?;
        } else if data.starts_with("workflow_") {
            handle_workflow_button(bot, q, data, &pool, dialogue, localization).await?;
        }
    }

    Ok(())
}

/// Handle recipe selection callback
async fn handle_recipe_selection(
    bot: &Bot,
    msg: &teloxide::types::MaybeInaccessibleMessage,
    recipe_name: &str,
    language_code: &Option<String>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    debug!(recipe_name = %recipe_name, "Handling recipe selection");

    // For now, just send a placeholder message
    // TODO: Implement actual recipe details display
    let message = format!(
        "📖 **{}**\n\n{}",
        recipe_name,
        t_lang(
            localization,
            "recipe-details-coming-soon",
            language_code.as_deref()
        )
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

/// Parameters for edit button handling
#[derive(Debug)]
struct EditButtonParams<'a> {
    bot: &'a Bot,
    q: &'a teloxide::types::CallbackQuery,
    data: &'a str,
    ingredients: &'a Vec<crate::text_processing::MeasurementMatch>,
    recipe_name: &'a str,
    dialogue_lang_code: &'a Option<String>,
    message_id: Option<i32>,
    extracted_text: &'a str,
    dialogue: &'a RecipeDialogue,
    localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// Parameters for delete button handling
#[derive(Debug)]
struct DeleteButtonParams<'a> {
    bot: &'a Bot,
    q: &'a teloxide::types::CallbackQuery,
    data: &'a str,
    ingredients: &'a mut Vec<crate::text_processing::MeasurementMatch>,
    recipe_name: &'a str,
    dialogue_lang_code: &'a Option<String>,
    message_id: Option<i32>,
    extracted_text: &'a str,
    recipe_name_from_caption: &'a Option<String>,
    dialogue: &'a RecipeDialogue,
    localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// Parameters for confirm button handling
#[derive(Debug)]
struct ConfirmButtonParams<'a> {
    bot: &'a Bot,
    q: &'a teloxide::types::CallbackQuery,
    ingredients: &'a [crate::text_processing::MeasurementMatch],
    dialogue_lang_code: &'a Option<String>,
    extracted_text: &'a str,
    recipe_name_from_caption: &'a Option<String>,
    dialogue: &'a RecipeDialogue,
    pool: &'a Arc<PgPool>,
    localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// Handle edit button in review ingredients state
async fn handle_edit_button(params: EditButtonParams<'_>) -> Result<()> {
    let EditButtonParams {
        bot,
        q,
        data,
        ingredients,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        dialogue,
        localization,
    } = params;

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
                localization,
                "edit-ingredient-prompt",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                localization,
                "current-ingredient",
                dialogue_lang_code.as_deref()
            ),
            ingredient.quantity,
            ingredient.measurement.as_deref().unwrap_or(""),
            ingredient.ingredient_name
        );
        bot.send_message(q.message.as_ref().unwrap().chat().id, edit_prompt)
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
async fn handle_delete_button(params: DeleteButtonParams<'_>) -> Result<()> {
    let DeleteButtonParams {
        bot,
        q,
        data,
        ingredients,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        localization,
    } = params;

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
                t_lang(localization, "review-title", dialogue_lang_code.as_deref()),
                t_lang(
                    localization,
                    "review-no-ingredients",
                    dialogue_lang_code.as_deref()
                ),
                t_lang(
                    localization,
                    "review-no-ingredients-help",
                    dialogue_lang_code.as_deref()
                )
            );

            let keyboard = vec![vec![
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(
                        localization,
                        "review-add-more",
                        dialogue_lang_code.as_deref(),
                    ),
                    "add_more",
                ),
                teloxide::types::InlineKeyboardButton::callback(
                    t_lang(localization, "cancel", dialogue_lang_code.as_deref()),
                    "cancel_empty",
                ),
            ]];

            // Edit the original message
            match bot
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
                    error!(user_id = %q.from.id, error = %e, "Failed to edit message for empty ingredients")
                }
            }
        } else {
            // Update the message with remaining ingredients
            let review_message = format!(
                "📝 **{}**\n\n{}\n\n{}",
                t_lang(localization, "review-title", dialogue_lang_code.as_deref()),
                t_lang(
                    localization,
                    "review-description",
                    dialogue_lang_code.as_deref()
                ),
                format_ingredients_list(ingredients, dialogue_lang_code.as_deref(), localization)
            );

            let keyboard = create_ingredient_review_keyboard(
                ingredients,
                dialogue_lang_code.as_deref(),
                localization,
            );

            // Edit the original message
            match bot
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
                    error!(user_id = %q.from.id, error = %e, "Failed to edit message after ingredient deletion")
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
                recipe_name_from_caption: recipe_name_from_caption.clone(), // Preserve caption info
            })
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error!(user_id = %q.from.id, error = %e, "Failed to update dialogue state after deletion")
            }
        }
    }
    Ok(())
}

/// Handle confirm button in review ingredients state
async fn handle_confirm_button(params: ConfirmButtonParams<'_>) -> Result<()> {
    let ConfirmButtonParams {
        bot,
        q,
        ingredients,
        dialogue_lang_code,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        pool,
        localization,
    } = params;

    // Record user engagement metric for recipe confirmation
    crate::observability::record_user_engagement_metrics(
        q.from.id.0 as i64,
        crate::observability::UserAction::RecipeConfirm,
        None, // No session duration for individual actions
        dialogue_lang_code.as_deref(),
    );

    // Check if we have a recipe name from caption
    if let Some(caption_recipe_name) = recipe_name_from_caption {
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
            error!(error = %e, "Failed to save ingredients to database");
            bot.send_message(
                q.message.as_ref().unwrap().chat().id,
                t_lang(
                    localization,
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
                localization,
                "workflow-recipe-saved",
                dialogue_lang_code.as_deref()
            ),
            t_args_lang(
                localization,
                "caption-recipe-saved",
                &[("recipe_name", caption_recipe_name.as_str())],
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                localization,
                "workflow-what-next",
                dialogue_lang_code.as_deref()
            )
        );

        let confirmation_keyboard =
            create_post_confirmation_keyboard(dialogue_lang_code.as_deref(), localization);

        // Update the original review message
        match bot
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
                error!(user_id = %q.from.id, error = %e, "Failed to update message after confirmation")
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
                localization,
                "workflow-recipe-saved",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                localization,
                "workflow-what-next",
                dialogue_lang_code.as_deref()
            )
        );

        let confirmation_keyboard =
            create_post_confirmation_keyboard(dialogue_lang_code.as_deref(), localization);

        // Update the original review message
        match bot
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
                error!(user_id = %q.from.id, error = %e, "Failed to update message after confirmation")
            }
        }

        // Send recipe name prompt
        let recipe_name_prompt = format!(
            "🏷️ **{}**\n\n{}",
            t_lang(
                localization,
                "recipe-name-prompt",
                dialogue_lang_code.as_deref()
            ),
            t_lang(
                localization,
                "recipe-name-prompt-hint",
                dialogue_lang_code.as_deref()
            )
        );

        bot.send_message(q.message.as_ref().unwrap().chat().id, recipe_name_prompt)
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

/// Handle workflow buttons (add another, list recipes, search recipes)
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
    cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    // For now, delegate to the original handler
    // TODO: Integrate caching into specific operations
    callback_handler(bot, q, pool, dialogue, localization).await
}
