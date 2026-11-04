//! The name typed after ✅ when the photo gave none, and the save of a reviewed recipe.

use crate::errors::AppError;
use crate::localization::t_lang;
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info};

// Import text processing types
use crate::text_processing::MeasurementMatch;

// Import dialogue types
use crate::dialogue::RecipeDialogue;

// Import validation functions
use crate::validation::validate_recipe_name;

// Import HandlerContext
use crate::bot::cleanup::{self, send_transient};
use crate::bot::home::send_home;

use super::*;

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
        message_id,
    } = params;

    let input = recipe_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        return handle_recipe_name_cancellation(
            bot,
            msg,
            dialogue,
            message_id,
            &pool,
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
                message_id,
            })
            .await
        }
        Err(error) => {
            handle_recipe_name_validation_error(
                bot,
                msg,
                handler_ctx.localization,
                error,
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
    prompt_message_id: Option<i32>,
    pool: &PgPool,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    // A cancelled creation leaves nothing behind: the prompt and the review go
    dialogue.exit().await?;
    // The name prompt is the saved notice only when a name is given
    if let Some(id) = prompt_message_id {
        cleanup::track(msg.chat.id, teloxide::types::MessageId(id)).await;
    }
    send_home(bot, pool, msg.chat.id, localization, language_code).await;
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
        message_id,
    } = params;

    // Recipe name is valid, save ingredients to database; the list then shows its page
    let mut list_at = crate::bot::home::ListAt::Start;
    match save_ingredients_to_database(
        pool,
        msg.chat.id.0,
        extracted_text,
        ingredients,
        validated_name,
        ctx.language_code,
    )
    .await
    {
        Err(_) => {
            // Logged by the save; nothing was written: keep the dialogue to send the name again
            send_transient(
                ctx.bot,
                msg.chat.id,
                t_lang(ctx.localization, "save-failed-retry", ctx.language_code),
            )
            .await?;
            return Ok(());
        }
        Ok(saved) => {
            crate::dialogue::end_after_write(&dialogue).await;
            if let Some(main) = saved.first() {
                list_at = crate::bot::home::ListAt::Recipe(main.recipe_id);
            }
            // One history line is the result; the name prompt goes with the review messages
            if let Some(prompt_msg_id) = message_id {
                cleanup::track(msg.chat.id, teloxide::types::MessageId(prompt_msg_id)).await;
            }
            // The stored names: "Tarte (2)" when the user already has a "Tarte"
            crate::bot::home::send_created_lines(
                ctx.bot,
                msg.chat.id,
                &saved,
                ctx.localization,
                ctx.language_code,
            )
            .await;
        }
    }

    crate::bot::home::send_home_at(
        ctx.bot,
        pool,
        msg.chat.id,
        list_at,
        ctx.localization,
        ctx.language_code,
    )
    .await;
    Ok(())
}

/// Handle recipe name validation errors
async fn handle_recipe_name_validation_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error: AppError,
    language_code: Option<&str>,
) -> Result<()> {
    let error_message = match error {
        AppError::Validation(reason) => t_lang(localization, reason.l10n_key(), language_code),
        _ => t_lang(localization, "recipe-name-invalid", language_code),
    };

    send_transient(bot, msg.chat.id, error_message).await?;
    // Keep dialogue active, user can try again
    Ok(())
}

/// Save ingredients to database: one recipe, or one per part of the photo, linked together
/// (UX plan U9: "Malva pudding" and "Malva pudding (sauce)"). Returns the saved recipes, the
/// one named `recipe_name` first.
pub async fn save_ingredients_to_database(
    pool: &PgPool,
    telegram_id: i64,
    extracted_text: &str,
    ingredients: &[MeasurementMatch],
    recipe_name: &str,
    language_code: Option<&str>,
) -> Result<Vec<crate::db::SavedRecipe>> {
    let start_time = std::time::Instant::now();

    debug!(telegram_id = %telegram_id, ingredient_count = %ingredients.len(), "Starting ingredient save process");

    // User, recipes, names, ingredients and links in one transaction: nothing is left
    // half-saved
    let photo_parts = crate::recipe_parts::split_parts(ingredients);
    let names = crate::recipe_parts::part_names(recipe_name, &photo_parts);
    let parts: Vec<crate::db::RecipePart> = photo_parts
        .iter()
        .zip(names)
        .map(|(part, name)| crate::db::RecipePart {
            name,
            ingredients: part
                .lines
                .iter()
                .filter_map(|&line| ingredients.get(line))
                .map(crate::ingredient_editing::to_new_ingredient)
                .collect(),
        })
        .collect();
    let saved = match crate::db::save_recipe_parts(
        pool,
        telegram_id,
        language_code,
        extracted_text,
        &parts,
    )
    .await
    {
        Ok(saved) if !saved.is_empty() => saved,
        Ok(_) => return Err(anyhow::anyhow!("No recipe saved")),
        Err(e) => {
            error!(telegram_id = %telegram_id, ingredient_count = ingredients.len(), error = %e, "Recipe save failed, nothing was written");
            return Err(e);
        }
    };
    let (recipe_id, user_id) = (saved[0].recipe_id, saved[0].user_id);

    let processing_duration = start_time.elapsed();

    crate::observability::recipes_created(saved.len());

    info!(
        telegram_id = %telegram_id,
        user_id = %user_id,
        recipe_id = %recipe_id,
        recipes = saved.len(),
        ingredient_count = %ingredients.len(),
        duration_ms = %processing_duration.as_millis(),
        "Ingredient save process completed successfully"
    );

    Ok(saved)
}
