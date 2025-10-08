//! Dialogue Manager module for handling dialogue state transitions

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::error;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import text processing types
use crate::text_processing::{MeasurementDetector, MeasurementMatch};

// Import dialogue types
use crate::dialogue::{validate_recipe_name, RecipeDialogue, RecipeDialogueState};

// Import database types
use crate::db::{create_ingredient, create_recipe, get_or_create_user, update_recipe_recipe_name};

// Import UI builder functions
use super::ui_builder::{create_ingredient_review_keyboard, format_ingredients_list};

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
    pub language_code: Option<&'a str>,
}

/// Parameters for recipe name input after confirmation
#[derive(Debug)]
pub struct RecipeNameAfterConfirmInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub recipe_name_input: &'a str,
    pub ingredients: Vec<MeasurementMatch>,
    pub language_code: Option<&'a str>,
    pub extracted_text: String,
}

/// Parameters for ingredient edit input handling
#[derive(Debug)]
pub struct IngredientEditInputParams<'a> {
    pub edit_input: &'a str,
    pub recipe_name: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub editing_index: usize,
    pub language_code: Option<&'a str>,
    pub message_id: Option<i32>,
    pub extracted_text: String,
}

/// Parameters for ingredient review input handling
#[derive(Debug)]
pub struct IngredientReviewInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub review_input: &'a str,
    pub recipe_name: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub language_code: Option<&'a str>,
    pub extracted_text: String,
}

/// Handle recipe name input during dialogue
pub async fn handle_recipe_name_input(
    ctx: DialogueContext<'_>,
    params: RecipeNameInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization,
    } = ctx;
    let RecipeNameInputParams {
        pool: _pool,
        recipe_name_input,
        extracted_text,
        ingredients,
        language_code,
    } = params;
    // Validate recipe name
    match validate_recipe_name(recipe_name_input) {
        Ok(validated_name) => {
            // Recipe name is valid, transition to ingredient review state
            let review_message = format!(
                "📝 **{}**\n\n{}\n\n{}",
                t_lang(localization, "review-title", language_code),
                t_lang(localization, "review-description", language_code),
                format_ingredients_list(&ingredients, language_code, localization)
            );

            let keyboard =
                create_ingredient_review_keyboard(&ingredients, language_code, localization);

            let sent_message = bot
                .send_message(msg.chat.id, review_message)
                .reply_markup(keyboard)
                .await?;

            // Update dialogue state to review ingredients
            dialogue
                .update(RecipeDialogueState::ReviewIngredients {
                    recipe_name: validated_name,
                    ingredients,
                    language_code: language_code.map(|s| s.to_string()),
                    message_id: Some(sent_message.id.0 as i32),
                    extracted_text,
                })
                .await?;
        }
        Err("empty") => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-invalid", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err("too_long") => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-too-long", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-invalid", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
    }

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
        localization,
    } = ctx;
    let RecipeNameAfterConfirmInputParams {
        pool,
        recipe_name_input,
        ingredients,
        language_code,
        extracted_text,
    } = params;
    let input = recipe_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if matches!(input.as_str(), "cancel" | "stop" | "back") {
        // User cancelled, end dialogue without saving
        bot.send_message(
            msg.chat.id,
            t_lang(localization, "review-cancelled", language_code),
        )
        .await?;
        dialogue.exit().await?;
        return Ok(());
    }

    // Validate recipe name
    match validate_recipe_name(recipe_name_input) {
        Ok(validated_name) => {
            // Recipe name is valid, save ingredients to database
            if let Err(e) = save_ingredients_to_database(
                &pool,
                msg.chat.id.0,
                &extracted_text,
                &ingredients,
                &validated_name,
                language_code,
            )
            .await
            {
                error!(error = %e, "Failed to save ingredients to database");
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "error-processing-failed", language_code),
                )
                .await?;
            } else {
                // Success! Send confirmation message
                let success_message = t_args_lang(
                    localization,
                    "recipe-complete",
                    &[
                        ("recipe_name", validated_name.as_str()),
                        ("ingredient_count", &ingredients.len().to_string()),
                    ],
                    language_code,
                );
                bot.send_message(msg.chat.id, success_message).await?;
            }

            // End the dialogue
            dialogue.exit().await?;
        }
        Err("empty") => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-invalid", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err("too_long") => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-too-long", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "recipe-name-invalid", language_code),
            )
            .await?;
            // Keep dialogue active, user can try again
        }
    }

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
        localization,
    } = ctx;
    let IngredientEditInputParams {
        edit_input,
        recipe_name,
        mut ingredients,
        editing_index,
        language_code,
        message_id,
        extracted_text,
    } = params;
    let input = edit_input.trim().to_lowercase();

    // Check for cancellation commands
    if matches!(input.as_str(), "cancel" | "stop" | "back") {
        // User cancelled editing, return to review state without changes
        let review_message = format!(
            "📝 **{}**\n\n{}\n\n{}",
            t_lang(localization, "review-title", language_code),
            t_lang(localization, "review-description", language_code),
            format_ingredients_list(&ingredients, language_code, localization)
        );

        let keyboard = create_ingredient_review_keyboard(&ingredients, language_code, localization);

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

        // Update dialogue state to review ingredients
        dialogue
            .update(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: language_code.map(|s| s.to_string()),
                message_id,
                extracted_text,
            })
            .await?;

        return Ok(());
    }

    // Parse the user input to create a new ingredient
    match parse_ingredient_from_text(edit_input) {
        Ok(new_ingredient) => {
            // Update the ingredient at the editing index
            if editing_index < ingredients.len() {
                ingredients[editing_index] = new_ingredient;

                // Return to review state with updated ingredients
                let review_message = format!(
                    "📝 **{}**\n\n{}\n\n{}",
                    t_lang(localization, "review-title", language_code),
                    t_lang(localization, "review-description", language_code),
                    format_ingredients_list(&ingredients, language_code, localization)
                );

                let keyboard =
                    create_ingredient_review_keyboard(&ingredients, language_code, localization);

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

                // Update dialogue state to review ingredients
                dialogue
                    .update(RecipeDialogueState::ReviewIngredients {
                        recipe_name,
                        ingredients,
                        language_code: language_code.map(|s| s.to_string()),
                        message_id,
                        extracted_text,
                    })
                    .await?;
            } else {
                // Invalid index, return to review state
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "error-invalid-edit", language_code),
                )
                .await?;
                dialogue
                    .update(RecipeDialogueState::ReviewIngredients {
                        recipe_name,
                        ingredients,
                        language_code: language_code.map(|s| s.to_string()),
                        message_id,
                        extracted_text,
                    })
                    .await?;
            }
        }
        Err(error_msg) => {
            // Invalid input, ask user to try again
            let error_message = format!(
                "{}\n\n{}",
                t_lang(localization, error_msg, language_code),
                t_lang(localization, "edit-try-again", language_code)
            );
            bot.send_message(msg.chat.id, error_message).await?;
            // Stay in editing state for user to try again
        }
    }

    Ok(())
}

/// Parse ingredient text input and create a MeasurementMatch
pub fn parse_ingredient_from_text(input: &str) -> Result<MeasurementMatch, &'static str> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err("edit-empty");
    }

    // Check for maximum length to prevent abuse
    if trimmed.len() > 200 {
        return Err("edit-too-long");
    }

    // Try to extract measurement using the detector
    let detector = match MeasurementDetector::new() {
        Ok(detector) => detector,
        Err(_) => return Err("error-processing-failed"),
    };

    // Create a temporary text with the input to extract measurements
    let temp_text = format!("temp: {}", trimmed);
    let matches = detector.extract_ingredient_measurements(&temp_text);

    if let Some(mut measurement_match) = matches.into_iter().next() {
        // Found a measurement, validate the ingredient name
        let ingredient_name = measurement_match.ingredient_name.trim();

        // Check ingredient name length (before post-processing truncation)
        // Re-extract the raw ingredient name to check its length
        let temp_text = format!("temp: {}", trimmed);
        let measurement_end = measurement_match.end_pos;
        let raw_ingredient_name = temp_text[measurement_end..].trim();

        if raw_ingredient_name.is_empty() {
            return Err("edit-no-ingredient-name");
        }

        if raw_ingredient_name.len() > 100 {
            return Err("edit-ingredient-name-too-long");
        }

        if ingredient_name.is_empty() {
            return Err("edit-no-ingredient-name");
        }

        if ingredient_name.len() > 100 {
            return Err("edit-ingredient-name-too-long");
        }

        // Check for negative quantity by looking at the original text
        let temp_text = format!("temp: {}", trimmed);
        let quantity_start = measurement_match.start_pos;
        let mut actual_quantity = measurement_match.quantity.clone();

        // Check if there's a minus sign before the quantity
        if quantity_start > 0 && temp_text.as_bytes()[quantity_start - 1] == b'-' {
            // Check if the minus sign is not part of another word (should be preceded by space or start)
            let before_minus = if quantity_start > 1 {
                temp_text.as_bytes()[quantity_start - 2]
            } else {
                b' '
            };
            if before_minus == b' ' || quantity_start == 1 {
                actual_quantity = format!("-{}", actual_quantity);
            }
        }

        measurement_match.quantity = actual_quantity;

        // Validate quantity is reasonable (not zero or negative)
        if let Some(qty) = parse_quantity(&measurement_match.quantity) {
            if qty <= 0.0 || qty > 10000.0 {
                return Err("edit-invalid-quantity");
            }
        }

        // Clean up the ingredient name
        measurement_match.ingredient_name = ingredient_name.to_string();
        Ok(measurement_match)
    } else {
        // No measurement found, try to extract a simple quantity pattern
        let quantity_pattern = regex::Regex::new(r"^(-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?)").unwrap();
        if let Some(captures) = quantity_pattern.captures(trimmed) {
            if let Some(quantity_match) = captures.get(1) {
                let quantity = quantity_match.as_str().trim().to_string();
                let remaining = trimmed[quantity_match.end()..].trim().to_string();

                // Validate quantity
                if let Some(qty) = parse_quantity(&quantity) {
                    if qty <= 0.0 || qty > 10000.0 {
                        return Err("edit-invalid-quantity");
                    }
                }

                let ingredient_name = if remaining.is_empty() {
                    return Err("edit-no-ingredient-name");
                } else if remaining.len() > 100 {
                    return Err("edit-ingredient-name-too-long");
                } else {
                    remaining
                };

                Ok(MeasurementMatch {
                    quantity,
                    measurement: None,
                    ingredient_name,
                    line_number: 0,
                    start_pos: 0,
                    end_pos: trimmed.len(),
                })
            } else {
                Err("edit-invalid-format")
            }
        } else {
            // No quantity found, treat the whole input as ingredient name
            if trimmed.len() > 100 {
                return Err("edit-ingredient-name-too-long");
            }

            Ok(MeasurementMatch {
                quantity: "1".to_string(), // Default quantity
                measurement: None,
                ingredient_name: trimmed.to_string(),
                line_number: 0,
                start_pos: 0,
                end_pos: trimmed.len(),
            })
        }
    }
}

/// Parse quantity string to f64 (handles fractions and decimals)
fn parse_quantity(quantity_str: &str) -> Option<f64> {
    if quantity_str.contains('/') {
        // Handle fractions like "1/2"
        let parts: Vec<&str> = quantity_str.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(numerator), Ok(denominator)) =
                (parts[0].parse::<f64>(), parts[1].parse::<f64>())
            {
                if denominator != 0.0 {
                    Some(numerator / denominator)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // Handle regular numbers, replace comma with dot for European format
        quantity_str.replace(',', ".").parse::<f64>().ok()
    }
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
        localization,
    } = ctx;
    let IngredientReviewInputParams {
        pool: _pool,
        review_input,
        recipe_name,
        ingredients,
        language_code,
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
                language_code,
            )
            .await
            {
                error!(error = %e, "Failed to save ingredients to database");
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "error-processing-failed", language_code),
                )
                .await?;
            } else {
                // Success! Send confirmation message
                let success_message = t_args_lang(
                    localization,
                    "recipe-complete",
                    &[
                        ("recipe_name", recipe_name.as_str()),
                        ("ingredient_count", &ingredients.len().to_string()),
                    ],
                    language_code,
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
                t_lang(localization, "review-cancelled", language_code),
            )
            .await?;
            dialogue.exit().await?;
        }
        _ => {
            // Unknown command, show help
            let help_message = format!(
                "{}\n\n{}",
                t_lang(localization, "review-help", language_code),
                format_ingredients_list(&ingredients, language_code, localization)
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
    // Get or create user
    let user = get_or_create_user(pool, telegram_id, language_code).await?;

    // Create recipe
    let recipe_id = create_recipe(pool, telegram_id, extracted_text).await?;

    // Update recipe with recipe name
    update_recipe_recipe_name(pool, recipe_id, recipe_name).await?;

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

    Ok(())
}
