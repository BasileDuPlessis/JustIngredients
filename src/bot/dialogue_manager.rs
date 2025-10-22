//! Dialogue Manager module for handling dialogue state transitions

use crate::localization::{t_args_lang, t_lang};
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import error logging utilities
use crate::errors::error_logging;

// Import text processing types
use crate::text_processing::MeasurementMatch;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import validation functions
use crate::validation::{parse_ingredient_from_text, parse_quantity, validate_recipe_name};

// Import database types
use crate::db::{
    create_ingredient, create_recipe, get_or_create_user, update_recipe_name, Ingredient,
};

// Import UI builder functions
use super::ui_builder::{create_ingredient_review_keyboard, format_ingredients_list};

// Import HandlerContext
use super::HandlerContext;

/// Parameters for ingredient review input handling
#[derive(Debug)]
pub struct IngredientReviewInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub review_input: &'a str,
    pub recipe_name: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub ctx: &'a HandlerContext<'a>,
    pub extracted_text: String,
}

/// Parameters for recipe name success handling
#[derive(Debug)]
struct RecipeNameSuccessParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    pool: &'a PgPool,
    ingredients: &'a [MeasurementMatch],
    extracted_text: &'a str,
    validated_name: &'a str,
}

/// Parameters for edit cancellation handling
#[derive(Debug)]
struct EditCancellationParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    ingredients: &'a [MeasurementMatch],
    recipe_name: String,
    message_id: Option<i32>,
    extracted_text: String,
}

/// Parameters for edit success handling
#[derive(Debug)]
struct EditSuccessParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    ingredients: Vec<MeasurementMatch>,
    editing_index: usize,
    new_ingredient: MeasurementMatch,
    recipe_name: String,
    message_id: Option<i32>,
    extracted_text: String,
}

/// Common context for dialogue handlers
#[derive(Debug)]
pub struct DialogueContext<'a> {
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub dialogue: RecipeDialogue,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// Parameters for recipe name input handling
#[derive(Debug)]
pub struct RecipeNameInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub recipe_name_input: &'a str,
    pub extracted_text: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub ctx: &'a HandlerContext<'a>,
}

/// Parameters for recipe name input after confirmation
#[derive(Debug)]
pub struct RecipeNameAfterConfirmInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub recipe_name_input: &'a str,
    pub ingredients: Vec<MeasurementMatch>,
    pub ctx: &'a HandlerContext<'a>,
    pub extracted_text: String,
}

/// Parameters for recipe rename input handling
#[derive(Debug)]
pub struct RecipeRenameInputParams<'a> {
    pub pool: &'a PgPool,
    pub new_name_input: &'a str,
    pub recipe_id: i64,
    pub current_name: String,
    pub ctx: &'a HandlerContext<'a>,
}

/// Parameters for ingredient edit input handling
#[derive(Debug)]
pub struct IngredientEditInputParams<'a> {
    pub edit_input: &'a str,
    pub recipe_name: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub editing_index: usize,
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
    pub extracted_text: String,
}

/// Parameters for adding ingredient input handling (saved recipes)
#[derive(Debug)]
pub struct AddIngredientInputParams<'a> {
    pub pool: &'a PgPool,
    pub add_input: &'a str,
    pub recipe_id: i64,
    pub original_ingredients: &'a [Ingredient],
    pub current_matches: &'a [MeasurementMatch],
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
}

/// Parameters for saved ingredient edit input handling
#[derive(Debug)]
pub struct SavedIngredientEditInputParams<'a> {
    pub pool: &'a PgPool,
    pub edit_input: &'a str,
    pub recipe_id: i64,
    pub original_ingredients: &'a [Ingredient],
    pub current_matches: &'a [MeasurementMatch],
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
    pub editing_index: usize,
}

/// Handle recipe name input during dialogue
pub async fn handle_recipe_name_input(
    ctx: DialogueContext<'_>,
    params: RecipeNameInputParams<'_>,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let RecipeNameInputParams {
        pool: _pool,
        recipe_name_input,
        extracted_text,
        ingredients,
        ctx: handler_ctx,
    } = params;

    let ingredients_count = ingredients.len();

    // Validate recipe name
    match validate_recipe_name(recipe_name_input) {
        Ok(validated_name) => {
            // Recipe name is valid, transition to ingredient review state
            let review_message = format!(
                "📝 **{}**\n\n{}\n\n{}",
                t_lang(
                    handler_ctx.localization,
                    "review-title",
                    handler_ctx.language_code
                ),
                t_lang(
                    handler_ctx.localization,
                    "review-description",
                    handler_ctx.language_code
                ),
                format_ingredients_list(
                    &ingredients,
                    handler_ctx.language_code,
                    handler_ctx.localization
                )
            );

            let keyboard = create_ingredient_review_keyboard(
                &ingredients,
                handler_ctx.language_code,
                handler_ctx.localization,
            );

            let sent_message = bot
                .send_message(msg.chat.id, review_message)
                .reply_markup(keyboard)
                .await?;

            // Update dialogue state to review ingredients
            dialogue
                .update(RecipeDialogueState::ReviewIngredients {
                    recipe_name: validated_name.to_string(),
                    ingredients,
                    language_code: handler_ctx.language_code.map(|s| s.to_string()),
                    message_id: Some(sent_message.id.0 as i32),
                    extracted_text,
                    recipe_name_from_caption: None, // Recipe name came from user input, not caption
                })
                .await?;
        }
        Err("empty") => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-invalid",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err("too_long") => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-too-long",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-invalid",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
    }

    let duration = start_time.elapsed();
    crate::observability::record_dialogue_metrics(
        msg.chat.id.0,
        crate::observability::DialogueType::RecipeNaming,
        true, // completed
        ingredients_count,
        duration,
    );

    Ok(())
}

/// Handle recipe name input after ingredient confirmation during dialogue
pub async fn handle_recipe_name_after_confirm_input(
    ctx: DialogueContext<'_>,
    params: RecipeNameAfterConfirmInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let RecipeNameAfterConfirmInputParams {
        pool,
        recipe_name_input,
        ingredients,
        ctx: handler_ctx,
        extracted_text,
    } = params;

    let input = recipe_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        return handle_recipe_name_cancellation(
            bot,
            msg,
            dialogue,
            handler_ctx.localization,
            handler_ctx.language_code,
        )
        .await;
    }

    // Validate and save recipe name
    match validate_recipe_name(recipe_name_input) {
        Ok(validated_name) => {
            handle_recipe_name_success(RecipeNameSuccessParams {
                ctx: handler_ctx,
                msg,
                dialogue,
                pool: &pool,
                ingredients: &ingredients,
                extracted_text: &extracted_text,
                validated_name,
            })
            .await
        }
        Err(error_type) => {
            handle_recipe_name_validation_error(
                bot,
                msg,
                handler_ctx.localization,
                error_type,
                handler_ctx.language_code,
            )
            .await
        }
    }
}

/// Handle cancellation of recipe name input
async fn handle_recipe_name_cancellation(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    // User cancelled, end dialogue without saving
    bot.send_message(
        msg.chat.id,
        t_lang(localization, "review-cancelled", language_code),
    )
    .await?;
    dialogue.exit().await?;
    Ok(())
}

/// Handle successful recipe name validation and saving
async fn handle_recipe_name_success(params: RecipeNameSuccessParams<'_>) -> Result<()> {
    let RecipeNameSuccessParams {
        ctx,
        msg,
        dialogue,
        pool,
        ingredients,
        extracted_text,
        validated_name,
    } = params;

    // Recipe name is valid, save ingredients to database
    if let Err(e) = save_ingredients_to_database(
        pool,
        msg.chat.id.0,
        extracted_text,
        ingredients,
        validated_name,
        ctx.language_code,
    )
    .await
    {
        error_logging::log_recipe_error(
            &e,
            "save_ingredients_to_database",
            msg.chat.id.0,
            Some(validated_name),
            Some(ingredients.len()),
        );
        ctx.bot
            .send_message(
                msg.chat.id,
                t_lang(
                    ctx.localization,
                    "error-processing-failed",
                    ctx.language_code,
                ),
            )
            .await?;
    } else {
        // Success! Send confirmation message
        let success_message = t_args_lang(
            ctx.localization,
            "recipe-complete",
            &[
                ("recipe_name", validated_name),
                ("ingredient_count", &ingredients.len().to_string()),
            ],
            ctx.language_code,
        );
        ctx.bot.send_message(msg.chat.id, success_message).await?;
    }

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}

/// Handle recipe name validation errors
async fn handle_recipe_name_validation_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error_type: &str,
    language_code: Option<&str>,
) -> Result<()> {
    let error_message = match error_type {
        "empty" => t_lang(localization, "recipe-name-invalid", language_code),
        "too_long" => t_lang(localization, "recipe-name-too-long", language_code),
        _ => t_lang(localization, "recipe-name-invalid", language_code),
    };

    bot.send_message(msg.chat.id, error_message).await?;
    // Keep dialogue active, user can try again
    Ok(())
}

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
        ctx: handler_ctx,
        message_id,
        extracted_text,
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
        })
        .await;
    }

    // Parse and validate the user input
    match parse_ingredient_from_text(edit_input) {
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
            })
            .await
        }
        Err(error_msg) => {
            handle_edit_error(
                bot,
                msg,
                handler_ctx.localization,
                error_msg,
                handler_ctx.language_code,
            )
            .await
        }
    }
}

/// Handle recipe rename input during dialogue
pub async fn handle_recipe_rename_input(
    ctx: DialogueContext<'_>,
    params: RecipeRenameInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization: _,
    } = ctx;
    let RecipeRenameInputParams {
        pool: _pool,
        new_name_input,
        recipe_id,
        current_name,
        ctx: handler_ctx,
    } = params;

    let input = new_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        bot.send_message(
            msg.chat.id,
            t_lang(
                handler_ctx.localization,
                "delete-cancelled",
                handler_ctx.language_code,
            ),
        )
        .await?;
        dialogue.exit().await?;
        return Ok(());
    }

    // Validate the new recipe name
    match validate_recipe_name(new_name_input) {
        Ok(validated_name) => {
            // Update the recipe name in the database
            match update_recipe_name(_pool, recipe_id, validated_name).await {
                Ok(true) => {
                    let success_message = format!(
                        "✅ **{}**\n\n{}",
                        t_lang(
                            handler_ctx.localization,
                            "rename-recipe-success",
                            handler_ctx.language_code
                        ),
                        t_args_lang(
                            handler_ctx.localization,
                            "rename-recipe-success-details",
                            &[("old_name", &current_name), ("new_name", validated_name)],
                            handler_ctx.language_code
                        )
                    );
                    bot.send_message(msg.chat.id, success_message).await?;
                }
                Ok(false) => {
                    let message = t_lang(
                        handler_ctx.localization,
                        "recipe-not-found",
                        handler_ctx.language_code,
                    );
                    bot.send_message(msg.chat.id, message).await?;
                }
                Err(e) => {
                    error_logging::log_database_error(
                        &e,
                        "update_recipe_name",
                        Some(msg.chat.id.0),
                        Some(&[
                            ("recipe_id", &recipe_id.to_string()),
                            ("current_name", &current_name),
                        ]),
                    );
                    let message = format!(
                        "❌ **{}**\n\n{}",
                        t_lang(
                            handler_ctx.localization,
                            "error-renaming-recipe",
                            handler_ctx.language_code
                        ),
                        t_lang(
                            handler_ctx.localization,
                            "error-renaming-recipe-help",
                            handler_ctx.language_code
                        )
                    );
                    bot.send_message(msg.chat.id, message).await?;
                }
            }
        }
        Err("empty") => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-invalid",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err("too_long") => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-too-long",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "recipe-name-invalid",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
    }

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}

/// Check if input is a cancellation command
fn is_cancellation_command(input: &str) -> bool {
    matches!(input, "cancel" | "stop" | "back")
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
    } = params;

    // User cancelled editing, return to review state without changes
    let review_message = format!(
        "📝 **{}**\n\n{}\n\n{}",
        t_lang(ctx.localization, "review-title", ctx.language_code),
        t_lang(ctx.localization, "review-description", ctx.language_code),
        format_ingredients_list(ingredients, ctx.language_code, ctx.localization)
    );

    let keyboard =
        create_ingredient_review_keyboard(ingredients, ctx.language_code, ctx.localization);

    // If we have a message_id, edit the existing message; otherwise send a new one
    if let Some(msg_id) = message_id {
        ctx.bot
            .edit_message_text(
                msg.chat.id,
                teloxide::types::MessageId(msg_id),
                review_message,
            )
            .reply_markup(keyboard)
            .await?;
    } else {
        ctx.bot
            .send_message(msg.chat.id, review_message)
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
            recipe_name_from_caption: None, // Recipe name came from user input, not caption
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
    } = params;

    // Update the ingredient at the editing index
    if editing_index < ingredients.len() {
        ingredients[editing_index] = new_ingredient;

        // Return to review state with updated ingredients
        let review_message = format!(
            "📝 **{}**\n\n{}\n\n{}",
            t_lang(ctx.localization, "review-title", ctx.language_code),
            t_lang(ctx.localization, "review-description", ctx.language_code),
            format_ingredients_list(&ingredients, ctx.language_code, ctx.localization)
        );

        let keyboard =
            create_ingredient_review_keyboard(&ingredients, ctx.language_code, ctx.localization);

        // If we have a message_id, edit the existing message; otherwise send a new one
        if let Some(msg_id) = message_id {
            ctx.bot
                .edit_message_text(
                    msg.chat.id,
                    teloxide::types::MessageId(msg_id),
                    review_message,
                )
                .reply_markup(keyboard)
                .await?;
        } else {
            ctx.bot
                .send_message(msg.chat.id, review_message)
                .reply_markup(keyboard)
                .await?;
        }

        // Update dialogue state to review ingredients
        dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: ctx.language_code.map(|s| s.to_string()),
                message_id,
                extracted_text,
                recipe_name_from_caption: None, // Recipe name came from user input, not caption
            })
            .await?;
    } else {
        // Invalid index, return to review state
        ctx.bot
            .send_message(
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
                recipe_name_from_caption: None, // Recipe name came from user input, not caption
            })
            .await?;
    }

    Ok(())
}

/// Handle ingredient editing error
async fn handle_edit_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error_msg: &str,
    language_code: Option<&str>,
) -> Result<()> {
    // Invalid input, ask user to try again
    let error_message = format!(
        "{}\n\n{}",
        t_lang(localization, error_msg, language_code),
        t_lang(localization, "edit-try-again", language_code)
    );
    bot.send_message(msg.chat.id, error_message).await?;
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
        pool: _pool,
        review_input,
        recipe_name,
        ingredients,
        ctx: handler_ctx,
        extracted_text,
    } = params;
    let input = review_input.trim().to_lowercase();

    match input.as_str() {
        "confirm" | "ok" | "yes" | "save" => {
            // User confirmed, save ingredients to database
            if let Err(e) = save_ingredients_to_database(
                &_pool,
                msg.chat.id.0,
                &extracted_text,
                &ingredients,
                &recipe_name,
                handler_ctx.language_code,
            )
            .await
            {
                error_logging::log_recipe_error(
                    &e,
                    "save_ingredients_to_database",
                    msg.chat.id.0,
                    Some(&recipe_name),
                    Some(ingredients.len()),
                );
                bot.send_message(
                    msg.chat.id,
                    t_lang(
                        handler_ctx.localization,
                        "error-processing-failed",
                        handler_ctx.language_code,
                    ),
                )
                .await?;
            } else {
                // Success! Send confirmation message
                let success_message = t_args_lang(
                    handler_ctx.localization,
                    "recipe-complete",
                    &[
                        ("recipe_name", recipe_name.as_str()),
                        ("ingredient_count", &ingredients.len().to_string()),
                    ],
                    handler_ctx.language_code,
                );
                bot.send_message(msg.chat.id, success_message).await?;
            }

            // End the dialogue
            dialogue.exit().await?;
        }
        "cancel" | "stop" => {
            // User cancelled, end dialogue without saving
            bot.send_message(
                msg.chat.id,
                t_lang(
                    handler_ctx.localization,
                    "review-cancelled",
                    handler_ctx.language_code,
                ),
            )
            .await?;
            dialogue.exit().await?;
        }
        _ => {
            // Unknown command, show help
            let help_message = format!(
                "{}\n\n{}",
                t_lang(
                    handler_ctx.localization,
                    "review-help",
                    handler_ctx.language_code
                ),
                format_ingredients_list(
                    &ingredients,
                    handler_ctx.language_code,
                    handler_ctx.localization
                )
            );
            bot.send_message(msg.chat.id, help_message).await?;
            // Keep dialogue active
        }
    }

    Ok(())
}

/// Save ingredients to database
pub async fn save_ingredients_to_database(
    pool: &PgPool,
    telegram_id: i64,
    extracted_text: &str,
    ingredients: &[MeasurementMatch],
    recipe_name: &str,
    language_code: Option<&str>,
) -> Result<()> {
    let start_time = std::time::Instant::now();

    // Get or create user
    let user = get_or_create_user(pool, telegram_id, language_code).await?;

    // Create recipe
    let recipe_id = create_recipe(pool, telegram_id, extracted_text).await?;

    // Update recipe with recipe name
    update_recipe_name(pool, recipe_id, recipe_name).await?;

    // Save each ingredient
    for ingredient in ingredients {
        // Parse quantity from string (handle fractions)
        let quantity = parse_quantity(&ingredient.quantity);
        let unit = ingredient.measurement.as_deref();

        create_ingredient(
            pool,
            user.id,
            Some(recipe_id),
            &ingredient.ingredient_name,
            quantity,
            unit,
            extracted_text,
        )
        .await?;
    }

    let processing_duration = start_time.elapsed();

    // Record business metrics
    let naming_method = if recipe_name == "Recipe" {
        crate::observability::RecipeNamingMethod::Default
    } else {
        // For now, assume manual naming - could be enhanced to detect caption usage
        crate::observability::RecipeNamingMethod::Manual
    };

    crate::observability::record_recipe_metrics(
        recipe_name,
        ingredients.len(),
        naming_method,
        processing_duration,
        user.id,
    );

    Ok(())
}

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
            })
            .await?;
        }
        Err(error_msg) => {
            // Invalid input, ask user to try again
            let error_message = format!(
                "{}\n\n{}",
                t_lang(
                    handler_ctx.localization,
                    error_msg,
                    handler_ctx.language_code
                ),
                t_lang(
                    handler_ctx.localization,
                    "edit-try-again",
                    handler_ctx.language_code
                )
            );
            bot.send_message(msg.chat.id, error_message).await?;
            // Stay in adding state for user to try again
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
        message_id,
        editing_index,
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
            message_id,
        })
        .await?;
        return Ok(());
    }

    // Parse and validate the user input
    match parse_ingredient_from_text(edit_input) {
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
                    message_id,
                })
                .await?;
            } else {
                // Invalid index
                bot.send_message(
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
                    message_id,
                })
                .await?;
            }
        }
        Err(error_msg) => {
            // Invalid input, ask user to try again
            let error_message = format!(
                "{}\n\n{}",
                t_lang(
                    handler_ctx.localization,
                    error_msg,
                    handler_ctx.language_code
                ),
                t_lang(
                    handler_ctx.localization,
                    "edit-try-again",
                    handler_ctx.language_code
                )
            );
            bot.send_message(msg.chat.id, error_message).await?;
            // Stay in editing state for user to try again
        }
    }

    Ok(())
}

/// Parameters for returning to saved ingredients review
#[derive(Debug)]
struct ReturnToSavedIngredientsReviewParams<'a> {
    bot: &'a Bot,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    localization: &'a Arc<crate::localization::LocalizationManager>,
    recipe_id: i64,
    original_ingredients: &'a [Ingredient],
    current_matches: &'a [MeasurementMatch],
    language_code: Option<&'a str>,
    message_id: Option<i32>,
}

/// Helper function to return to saved ingredients review state
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
    } = params;
    // Send updated ingredient list message
    let review_message = format!(
        "✏️ **{}**\n\n{}\n\n{}",
        t_lang(localization, "editing-recipe", language_code),
        t_lang(localization, "editing-instructions", language_code),
        format_ingredients_list(current_matches, language_code, localization)
    );

    let keyboard = create_ingredient_review_keyboard(current_matches, language_code, localization);

    // If we have a message_id, edit the existing message; otherwise send a new one
    if let Some(msg_id) = message_id {
        bot.edit_message_text(
            msg.chat.id,
            teloxide::types::MessageId(msg_id),
            review_message,
        )
        .reply_markup(keyboard)
        .await?;
    } else {
        bot.send_message(msg.chat.id, review_message)
            .reply_markup(keyboard)
            .await?;
    }

    // Update dialogue state
    dialogue
        .update(RecipeDialogueState::EditingSavedIngredients {
            recipe_id,
            original_ingredients: original_ingredients.to_vec(),
            current_matches: current_matches.to_vec(),
            language_code: language_code.map(|s| s.to_string()),
            message_id,
        })
        .await?;

    Ok(())
}
