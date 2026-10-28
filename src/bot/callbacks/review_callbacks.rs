//! Review Ingredients Callbacks Module
//!
//! This module handles all callback queries that occur when the user is in the
//! ReviewIngredients dialogue state. This includes editing, deleting, confirming,
//! and canceling ingredient reviews.

use crate::bot::html::edit_html;
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
use crate::dialogue::{IngredientField, RecipeDialogue, RecipeDialogueState};

// Import UI components for the focused editing interface
use crate::bot::callbacks::ingredient_menu::show_ingredient_keyboard;
use crate::bot::cleanup::{self, send_transient};
use crate::bot::create_ingredient_review_keyboard;
use crate::bot::home::{send_home, send_saved_recipe};
use crate::bot::ui_builder::{
    edit_instruction_key, parse_ingredient_action, render_review, IngredientAction,
};
use crate::bot::ui_components::create_ingredient_editing_keyboard;

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
            let action = parse_ingredient_action(data);
            if let Some(action @ (IngredientAction::Menu(_) | IngredientAction::Back)) = action {
                show_ingredient_keyboard(
                    bot,
                    q,
                    &ingredients,
                    action,
                    dialogue_lang_code.as_deref(),
                    localization,
                )
                .await?;
            } else if let Some(IngredientAction::Edit(..)) = action {
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
                    recipe_name_from_caption: Some(&recipe_name_from_caption),
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
            } else if data == "add_ingredient" || data == "add_more" {
                handle_add_button(ReviewIngredientsParams {
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
                    pool: None,
                })
                .await?;
            } else if data == "cancel_review" {
                handle_cancel_review_button(
                    bot,
                    q,
                    &dialogue_lang_code,
                    dialogue,
                    localization,
                    &pool,
                )
                .await?;
            }
            // workflow_* buttons are handled once, by the prefix handler of callback_handler
        }
    }

    Ok(())
}

/// Handle edit button in review ingredients state: the review list is replaced by a prompt for
/// the chosen field of the ingredient (see [`open_line_prompt`]).
async fn handle_edit_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ingredients = params
        .ingredients_slice
        .expect("Ingredients slice should be provided for edit callback");
    let Some(IngredientAction::Edit(field, index)) = params.data.and_then(parse_ingredient_action)
    else {
        return Ok(());
    };
    let Some(ingredient) = ingredients.get(index) else {
        return Ok(());
    };
    // Record user engagement metric for ingredient editing
    crate::observability::record_user_engagement_metrics(
        params.q.from.id.0 as i64,
        crate::observability::UserAction::IngredientEdit,
        None, // No session duration for individual actions
        params.dialogue_lang_code.as_deref(),
    );

    let lang = params.dialogue_lang_code.as_deref();
    let prompt = format!(
        "✏️ {}\n\n{}: {}\n\n{}",
        t_lang(params.ctx.localization, "edit-ingredient-title", lang),
        t_lang(params.ctx.localization, "edit-ingredient-current", lang),
        // Same rendering as the review list ("sel" without quantity, not "**  sel**")
        crate::bot::ui_builder::ingredient_line(ingredient, &ingredient.ingredient_name),
        t_lang(params.ctx.localization, edit_instruction_key(field), lang)
    );
    open_line_prompt(params, index, field, prompt).await
}

/// ➕ Add ingredient (and the "add more" button of an emptied list): the typed line becomes a
/// new last ingredient, through the same prompt as an edit.
async fn handle_add_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
    let ingredients = params
        .ingredients_slice
        .expect("Ingredients slice should be provided for add callback");
    let lang = params.dialogue_lang_code.as_deref();
    let prompt = format!(
        "➕ {}\n\n{}",
        t_lang(params.ctx.localization, "add-ingredient", lang),
        t_lang(params.ctx.localization, "add-ingredient-prompt", lang)
    );
    let new_index = ingredients.len();
    open_line_prompt(params, new_index, IngredientField::Line, prompt).await
}

/// Focused editing: the review list is replaced by `prompt` with a single Cancel button, and the
/// typed answer replaces `field` of ingredient `index` (a new ingredient when `index` is the
/// list's length). After the answer or Cancel the list comes back in the same message.
async fn open_line_prompt(
    params: ReviewIngredientsParams<'_>,
    index: usize,
    field: IngredientField,
    prompt: String,
) -> Result<()> {
    let ReviewIngredientsParams {
        ctx,
        q,
        ingredients_slice,
        recipe_name,
        dialogue_lang_code,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        dialogue,
        ..
    } = params;
    let ingredients =
        ingredients_slice.expect("Ingredients slice should be provided for edit callback");
    let Some(chat_id) = q.message.as_ref().map(|message| message.chat().id) else {
        return Ok(());
    };

    let keyboard =
        create_ingredient_editing_keyboard(dialogue_lang_code.as_deref(), ctx.localization);

    // Replace the original recipe display message with the focused prompt
    let edited = match message_id {
        Some(id) => ctx
            .bot
            .edit_message_text(chat_id, teloxide::types::MessageId(id), prompt.clone())
            .reply_markup(keyboard.clone())
            .await
            .map_err(|e| {
                error_logging::log_internal_error(
                    &e,
                    "open_line_prompt",
                    "Failed to replace recipe display with editing prompt",
                    Some(q.from.id.0 as i64),
                );
            })
            .ok(),
        None => None,
    };
    // Fallback: send a new message if the list could not be edited
    let prompt_message = match edited {
        Some(message) => message,
        None => {
            ctx.bot
                .send_message(chat_id, prompt)
                .reply_markup(keyboard)
                .await?
        }
    };

    // Transition to editing state with updated message tracking
    dialogue
        .update(RecipeDialogueState::EditingIngredient {
            recipe_name: recipe_name.to_string(),
            ingredients: ingredients.to_vec(),
            editing_index: index,
            field,
            language_code: dialogue_lang_code.clone(),
            message_id: Some(prompt_message.id.0 as i32), // Track the editing prompt message
            original_message_id: message_id, // Original recipe display message to replace
            extracted_text: extracted_text.to_string(),
            recipe_name_from_caption: recipe_name_from_caption.cloned().flatten(), // Preserve caption info
        })
        .await?;
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
    let ingredients = ingredients.expect("Ingredients should be provided for delete callback");
    let index: usize = data
        .strip_prefix("delete_")
        .expect("Delete callback data should start with 'delete_'")
        .parse()
        .unwrap_or(0);

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
                "🗑️ {}\n\n{}\n\n{}",
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
                    "cancel_review",
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
            let review_message =
                render_review(ingredients, dialogue_lang_code.as_deref(), ctx.localization);

            let keyboard = create_ingredient_review_keyboard(
                ingredients,
                dialogue_lang_code.as_deref(),
                ctx.localization,
            );

            // Edit the original message
            match edit_html(
                ctx.bot,
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

/// Names of the ingredients whose quantity must be checked before saving: flagged by the
/// extractor, or out of the accepted range.
fn quantities_to_check(ingredients: &[crate::text_processing::MeasurementMatch]) -> Vec<&str> {
    ingredients
        .iter()
        .filter(|ingredient| ingredient.requires_quantity_confirmation)
        .map(|ingredient| ingredient.ingredient_name.as_str())
        .collect()
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
    let (Some(ingredients), Some(pool), Some(message)) =
        (ingredients_slice, pool, q.message.as_ref())
    else {
        return Ok(());
    };
    confirm_review(ReviewConfirmation {
        bot: ctx.bot,
        localization: ctx.localization,
        pool,
        dialogue,
        chat_id: message.chat().id,
        owner_id: q.from.id.0 as i64,
        // The message the button is on is the review list
        review_message_id: Some(message.id()),
        ingredients,
        extracted_text,
        known_name: recipe_name_from_caption.and_then(|name| name.as_deref()),
        language_code: dialogue_lang_code.as_deref(),
    })
    .await
}

/// A review being confirmed, by ✅ or by a typed confirmation word.
#[derive(Debug)]
pub struct ReviewConfirmation<'a> {
    pub bot: &'a Bot,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
    pub pool: &'a PgPool,
    pub dialogue: &'a RecipeDialogue,
    pub chat_id: ChatId,
    /// Telegram id of the user, owner of the saved recipe.
    pub owner_id: i64,
    /// The review list, deleted at the end of the action.
    pub review_message_id: Option<teloxide::types::MessageId>,
    pub ingredients: &'a [crate::text_processing::MeasurementMatch],
    pub extracted_text: &'a str,
    /// Name from the caption or the page title; asked for when missing.
    pub known_name: Option<&'a str>,
    pub language_code: Option<&'a str>,
}

/// What ✅ and a typed "valider" both do. Flagged quantities must be checked first (the review
/// stays open); then the recipe is saved under the known name, or the name is asked.
pub async fn confirm_review(c: ReviewConfirmation<'_>) -> Result<()> {
    // A recipe without ingredients cannot be edited later: refuse it, the review stays open
    if c.ingredients.is_empty() {
        send_transient(
            c.bot,
            c.chat_id,
            t_lang(c.localization, "review-empty-confirm", c.language_code),
        )
        .await?;
        return Ok(());
    }

    // Quantities the extractor was unsure about, or out of range, must be checked before
    // saving; saving them as they are would store a wrong or zero quantity (IMP-004).
    let to_check = quantities_to_check(c.ingredients);
    if !to_check.is_empty() {
        send_transient(
            c.bot,
            c.chat_id,
            t_args_lang(
                c.localization,
                "review-quantities-to-check",
                &[("ingredients", &to_check.join(", "))],
                c.language_code,
            ),
        )
        .await?;
        return Ok(());
    }

    // Record user engagement metric for recipe confirmation
    crate::observability::record_user_engagement_metrics(
        c.owner_id,
        crate::observability::UserAction::RecipeConfirm,
        None, // No session duration for individual actions
        c.language_code,
    );

    // The review ends here: its list goes with the action's messages
    if let Some(id) = c.review_message_id {
        cleanup::track(c.chat_id, id).await;
    }

    let Some(name) = c.known_name else {
        // No caption nor title: ask for the name
        debug!(user_id = %c.owner_id, "No caption or title, asking for the recipe name");
        remove_review_keyboard(&c).await;
        let recipe_name_prompt = format!(
            "🏷️ {}\n\n{}",
            t_lang(c.localization, "recipe-name-prompt", c.language_code),
            t_lang(c.localization, "recipe-name-prompt-hint", c.language_code)
        );
        // Deleted with the review's messages, also when a command ends the action
        let prompt_msg = send_transient(c.bot, c.chat_id, recipe_name_prompt).await?;
        c.dialogue
            .update(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                ingredients: c.ingredients.to_vec(),
                language_code: c.language_code.map(str::to_string),
                extracted_text: c.extracted_text.to_string(),
                recipe_name_from_caption: None,
                message_id: Some(prompt_msg.id.0 as i32),
            })
            .await?;
        return Ok(());
    };

    debug!(user_id = %c.owner_id, "Recipe name from the caption or the title, saving at once");
    let recipe_id = match save_ingredients_to_database(
        c.pool,
        c.owner_id,
        c.extracted_text,
        c.ingredients,
        name,
        c.language_code,
    )
    .await
    {
        Ok(recipe_id) => recipe_id,
        Err(_) => {
            // Logged by the save; nothing was written: the review stays open to try again
            send_transient(
                c.bot,
                c.chat_id,
                t_lang(c.localization, "save-failed-retry", c.language_code),
            )
            .await?;
            return Ok(());
        }
    };

    // The saved recipe card replaces the review list, then the home message
    remove_review_keyboard(&c).await;
    send_saved_recipe(
        c.bot,
        c.chat_id,
        recipe_id,
        name,
        c.ingredients,
        c.localization,
        c.language_code,
    )
    .await;
    send_home(c.bot, c.pool, c.chat_id, c.localization, c.language_code).await;
    c.dialogue.exit().await?;
    Ok(())
}

/// The list is deleted at the end of the action; until then (or if it cannot be) its buttons
/// must not act any more.
async fn remove_review_keyboard(c: &ReviewConfirmation<'_>) {
    let Some(id) = c.review_message_id else {
        return;
    };
    if let Err(e) = c.bot.edit_message_reply_markup(c.chat_id, id).await {
        error_logging::log_internal_error(
            &e,
            "confirm_review",
            "Failed to remove keyboard from ingredients message",
            Some(c.owner_id),
        );
    }
}

/// Handle cancel review button in review ingredients state
async fn handle_cancel_review_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    dialogue_lang_code: &Option<String>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: &PgPool,
) -> Result<()> {
    let Some(message) = q.message.as_ref() else {
        return Ok(());
    };
    cancel_review(
        bot,
        localization,
        pool,
        dialogue,
        message.chat().id,
        Some(message.id()),
        dialogue_lang_code.as_deref(),
    )
    .await
}

/// What ❌ and a typed cancel word both do: a cancelled creation leaves nothing behind (the
/// photo and the review list go with the action's messages), the dialogue ends and the recipe
/// list comes back at the bottom.
pub async fn cancel_review(
    bot: &Bot,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: &PgPool,
    dialogue: &RecipeDialogue,
    chat_id: ChatId,
    review_message_id: Option<teloxide::types::MessageId>,
    language_code: Option<&str>,
) -> Result<()> {
    if let Some(id) = review_message_id {
        // Recorded since U5: taking it again is harmless and covers older reviews
        cleanup::track(chat_id, id).await;
    }
    dialogue.exit().await?;
    send_home(bot, pool, chat_id, localization, language_code).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_processing::MeasurementMatch;

    fn ingredient(name: &str, flagged: bool) -> MeasurementMatch {
        MeasurementMatch {
            quantity: "1".to_string(),
            measurement: None,
            ingredient_name: name.to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 0,
            requires_quantity_confirmation: flagged,
        }
    }

    #[test]
    fn confirm_lists_only_flagged_ingredients() {
        let ingredients = [
            ingredient("farine", false),
            ingredient("beurre", true),
            ingredient("sucre", true),
        ];
        assert_eq!(quantities_to_check(&ingredients), vec!["beurre", "sucre"]);
        assert!(quantities_to_check(&ingredients[..1]).is_empty());
    }
}
