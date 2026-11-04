//! Editing Callbacks module for handling EditingSavedIngredients dialogue state

use crate::bot::html::edit_html;
use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use crate::bot::ui_builder::{create_ingredient_review_keyboard, render_recipe_summary};

// Import HandlerContext
use crate::bot::callbacks::ingredient_menu::show_ingredient_keyboard;
use crate::bot::cleanup::send_transient;
use crate::bot::home::send_home;
use crate::bot::ui_builder::{edit_instruction_key, parse_ingredient_action, IngredientAction};
use crate::bot::HandlerContext;
use crate::ingredient_editing::to_new_ingredient;

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
        open_menu: _,
    }) = dialogue_state
    {
        if q.message.is_some() {
            let action = parse_ingredient_action(data);
            if let Some(action @ (IngredientAction::Menu(_) | IngredientAction::Back)) = action {
                show_ingredient_keyboard(
                    bot,
                    q,
                    &current_matches,
                    action,
                    language_code.as_deref(),
                    localization,
                )
                .await?;
                // A typed answer applies to the ingredient whose menu is open
                let open_menu = match action {
                    IngredientAction::Menu(index) if index < current_matches.len() => Some(index),
                    _ => None,
                };
                dialogue
                    .update(RecipeDialogueState::EditingSavedIngredients {
                        recipe_id,
                        original_ingredients,
                        current_matches,
                        language_code,
                        message_id,
                        open_menu,
                    })
                    .await?;
            } else if let Some(IngredientAction::Edit(..)) = action {
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
            } else if data == "add_ingredient" || data == "add_more" {
                handle_add_ingredient_button(
                    bot,
                    q,
                    language_code.as_deref(),
                    dialogue,
                    localization,
                )
                .await?;
            } else if data == "cancel_review" {
                handle_cancel_saved_ingredients_button(
                    bot,
                    q,
                    language_code.as_deref(),
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

    let current_matches = current_matches_slice
        .context("Current matches slice should be provided for edit callback")?;
    let Some(IngredientAction::Edit(field, index)) = data.and_then(parse_ingredient_action) else {
        return Ok(());
    };
    if index < current_matches.len() {
        crate::observability::action(crate::observability::Action::IngredientEdited);

        let ingredient = &current_matches[index];
        let edit_prompt = format!(
            "✏️ {}\n\n{}: {}\n\n{}",
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
            // Same rendering as the list ("sel" without quantity, not "  sel")
            crate::bot::ui_builder::ingredient_line(ingredient, &ingredient.ingredient_name),
            t_lang(
                ctx.localization,
                edit_instruction_key(field),
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
                    .context("Callback query should have a message")?
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .context("Callback query should have a message")?
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
                    Some(crate::bot::telegram_id(q.from.id)),
                );
                // Fallback: send new message if editing fails
                ctx.bot
                    .send_message(
                        q.message
                            .as_ref()
                            .context("Callback query should have a message")?
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
                field,
                language_code: language_code.clone(),
                message_id,
                original_message_id: Some(
                    q.message
                        .as_ref()
                        .context("Callback query should have a message")?
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
        current_matches.context("Current matches should be provided for delete callback")?;

    let index: usize = data
        .strip_prefix("delete_")
        .context("Delete callback data should start with 'delete_'")?
        .parse()
        .unwrap_or(0);

    if index < current_matches.len() {
        crate::observability::action(crate::observability::Action::IngredientDeleted);

        current_matches.remove(index);

        // Check if all ingredients were deleted
        if current_matches.is_empty() {
            // All ingredients deleted - inform user and provide options
            let empty_message = format!(
                "🗑️ {}\n\n{}\n\n{}",
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

            // ➕ above the single exit ❌ (U6.2)
            let keyboard = vec![
                vec![teloxide::types::InlineKeyboardButton::callback(
                    t_lang(
                        ctx.localization,
                        "review-add-more",
                        language_code.as_deref(),
                    ),
                    "add_ingredient",
                )],
                vec![teloxide::types::InlineKeyboardButton::callback(
                    format!(
                        "❌ {}",
                        t_lang(ctx.localization, "cancel", language_code.as_deref())
                    ),
                    "cancel_review",
                )],
            ];

            // Edit the original message
            match ctx
                .bot
                .edit_message_text(
                    q.message
                        .as_ref()
                        .context("Callback query should have a message")?
                        .chat()
                        .id,
                    q.message
                        .as_ref()
                        .context("Callback query should have a message")?
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
                        Some(crate::bot::telegram_id(q.from.id)),
                    );
                }
            }
        } else {
            // Update the message with remaining ingredients
            let review_message = crate::bot::ui_builder::render_saved_editing(
                None,
                current_matches,
                language_code.as_deref(),
                ctx.localization,
            );

            let keyboard = create_ingredient_review_keyboard(
                current_matches,
                language_code.as_deref(),
                ctx.localization,
            );

            // Edit the original message
            match edit_html(
                ctx.bot,
                q.message
                    .as_ref()
                    .context("Callback query should have a message")?
                    .chat()
                    .id,
                q.message
                    .as_ref()
                    .context("Callback query should have a message")?
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
                        Some(crate::bot::telegram_id(q.from.id)),
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
                open_menu: None,
            })
            .await
        {
            Ok(_) => (),
            Err(e) => {
                error_logging::log_internal_error(
                    &e,
                    "callback_handler",
                    "Failed to update dialogue state after deletion",
                    Some(crate::bot::telegram_id(q.from.id)),
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
        .context("Current matches slice should be provided for confirm callback")?;
    let pool = pool.context("Database pool should be provided for confirm callback")?;
    let message = q
        .message
        .as_ref()
        .context("Callback query should have a message")?;
    let chat_id = message.chat().id;
    let lang = language_code.as_deref();

    // Saving an empty list would delete every ingredient, and the recipe could not be edited
    // again: refuse it, the editing stays open
    if current_matches.is_empty() {
        send_transient(
            ctx.bot,
            chat_id,
            t_lang(ctx.localization, "review-empty-confirm", lang),
        )
        .await?;
        return Ok(());
    }

    // The recipe owner is the sender of the callback query, never a value from callback data
    let telegram_id = crate::bot::telegram_id(q.from.id);

    crate::observability::action(crate::observability::Action::Edit);

    // Check ownership before any write: a recipe owned by another user is handled
    // exactly like a missing one
    if crate::db::read_recipe_with_name(pool, telegram_id, recipe_id)
        .await?
        .is_none()
    {
        send_transient(
            ctx.bot,
            chat_id,
            t_lang(ctx.localization, "recipe-not-found", lang),
        )
        .await?;
        dialogue.exit().await?;
        send_home(ctx.bot, pool, chat_id, ctx.localization, lang).await;
        return Ok(());
    }

    // Updates, additions and deletions in one transaction: never a half-applied edit
    let changes =
        crate::ingredient_editing::detect_ingredient_changes(original_ingredients, current_matches);
    let changed = !changes.to_update.is_empty()
        || !changes.to_add.is_empty()
        || !changes.to_delete.is_empty();
    if changed {
        let updates: Vec<crate::db::IngredientUpdate> = changes
            .to_update
            .iter()
            .map(|(id, m)| crate::db::IngredientUpdate {
                id: *id,
                ingredient: to_new_ingredient(m),
            })
            .collect();
        let additions: Vec<crate::db::NewIngredient> =
            changes.to_add.iter().map(to_new_ingredient).collect();
        if let Err(e) = crate::db::apply_ingredient_changes(
            pool,
            telegram_id,
            recipe_id,
            lang,
            &updates,
            &additions,
            &changes.to_delete,
        )
        .await
        {
            error_logging::log_database_error(
                &e,
                "apply_ingredient_changes",
                Some(telegram_id),
                Some(&[("recipe_id", &recipe_id.to_string())]),
            );
            send_transient(
                ctx.bot,
                chat_id,
                t_lang(ctx.localization, "error-updating-ingredients", lang),
            )
            .await?;
            return Ok(());
        }
        crate::dialogue::end_after_write(dialogue).await;
    }

    // The recipe as it is saved now; unreadable, the user was told and nothing more is shown
    let Some(card) = saved_recipe_card(ctx, pool, telegram_id, recipe_id, chat_id, lang).await?
    else {
        return Ok(());
    };
    // The history line of the edit, when something changed
    let edited_line = changed.then(|| {
        t_args_lang(
            ctx.localization,
            "recipe-edited-line",
            &[("name", card.name.as_str())],
            lang,
        )
    });

    // The recorded cards of the recipe get the new ingredients (nothing to update when nothing
    // changed) and the editing view goes; without one (older cards), the editing view becomes
    // the card
    let has_card = if changed {
        crate::bot::cards::refresh_cards(
            ctx.bot,
            pool,
            chat_id,
            telegram_id,
            recipe_id,
            lang,
            ctx.localization,
        )
        .await
    } else {
        !crate::bot::cleanup::recipe_cards(chat_id, recipe_id)
            .await
            .is_empty()
    };
    if !has_card {
        if let Err(e) = edit_html(ctx.bot, chat_id, message.id(), card.message)
            .reply_markup(card.keyboard)
            .await
        {
            error_logging::log_internal_error(
                &e,
                "handle_confirm_saved_ingredients_button",
                "Failed to update message with recipe details after confirmation",
                Some(telegram_id),
            );
        }
        crate::bot::cleanup::record_active_card(chat_id, message.id(), recipe_id).await;
    }

    // End the dialogue
    dialogue.exit().await?;
    if let Some(line) = edited_line {
        ctx.bot
            .send_message(chat_id, line)
            .disable_notification(true)
            .await?;
    }
    crate::bot::home::send_home_at(
        ctx.bot,
        pool,
        chat_id,
        crate::bot::home::ListAt::Recipe(recipe_id),
        ctx.localization,
        lang,
    )
    .await;

    Ok(())
}

/// A saved recipe's card: its name, text and buttons.
struct SavedRecipeCard {
    name: String,
    message: crate::bot::html::Html,
    keyboard: teloxide::types::InlineKeyboardMarkup,
}

/// The card of the recipe as it is saved now, after an edit; `None` when it cannot be read
/// (gone, or a database error): the user is then told.
async fn saved_recipe_card(
    ctx: &HandlerContext<'_>,
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    chat_id: ChatId,
    lang: Option<&str>,
) -> Result<Option<SavedRecipeCard>> {
    let recipe = match crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await {
        Ok(Some(recipe)) => recipe,
        Ok(None) => {
            error_logging::log_internal_error(
                &anyhow::anyhow!("Recipe not found"),
                "handle_confirm_saved_ingredients_button",
                "Recipe not found after confirmation",
                Some(telegram_id),
            );
            ctx.bot
                .send_message(chat_id, t_lang(ctx.localization, "recipe-not-found", lang))
                .await?;
            return Ok(None);
        }
        Err(e) => {
            error_logging::log_database_error(
                &e,
                "read_recipe_with_name",
                Some(telegram_id),
                Some(&[("recipe_id", &recipe_id.to_string())]),
            );
            ctx.bot
                .send_message(
                    chat_id,
                    t_lang(ctx.localization, "error-processing-failed", lang),
                )
                .await?;
            return Ok(None);
        }
    };
    let ingredients = crate::db::get_recipe_ingredients(pool, telegram_id, recipe_id).await?;
    let matches = crate::ingredient_editing::ingredients_to_measurement_matches(&ingredients);
    let name = recipe
        .recipe_name
        .unwrap_or_else(|| "Unnamed Recipe".to_string());
    let message = render_recipe_summary(&name, &matches, lang, ctx.localization);
    let keyboard =
        crate::bot::cards::card_keyboard(pool, telegram_id, recipe_id, lang, ctx.localization)
            .await;
    Ok(Some(SavedRecipeCard {
        name,
        message,
        keyboard,
    }))
}

/// Handle cancel button for saved ingredients editing
async fn handle_cancel_saved_ingredients_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: Option<&str>,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: Arc<PgPool>,
) -> Result<()> {
    let Some(message) = &q.message else {
        return Ok(());
    };
    // The recipe owner is the sender of the callback query, never a value from callback data
    cancel_saved_editing(
        bot,
        localization,
        &pool,
        dialogue,
        message.chat().id,
        crate::bot::telegram_id(q.from.id),
        language_code,
    )
    .await
}

/// What ❌ and a typed cancel word both do while a saved recipe's ingredients are edited: nothing
/// is written, the editing view goes (or turns back into the card when the recipe has no
/// recorded card), the editing ends and the recipe list follows. A recipe deleted meanwhile just
/// ends the editing.
pub async fn cancel_saved_editing(
    bot: &Bot,
    localization: &Arc<crate::localization::LocalizationManager>,
    pool: &PgPool,
    dialogue: &RecipeDialogue,
    chat_id: ChatId,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<()> {
    let mut list_at = crate::bot::home::ListAt::Start;
    if let Some(RecipeDialogueState::EditingSavedIngredients {
        recipe_id,
        message_id: Some(message_id),
        ..
    }) = dialogue.get().await?
    {
        list_at = crate::bot::home::ListAt::Recipe(recipe_id);
        // With a recorded card of the recipe (U5.4), the editing view just goes with the action;
        // otherwise it turns back into the card
        let has_card = !crate::bot::cleanup::recipe_cards(chat_id, recipe_id)
            .await
            .is_empty();
        if has_card {
            // "❌ Annuler" goes back one step: the editing view goes, the card is active again
            dialogue.exit().await?;
            crate::bot::home::back_to_card(
                bot,
                pool,
                chat_id,
                recipe_id,
                None,
                localization,
                language_code,
            )
            .await;
            return Ok(());
        }
        // Without a recorded card (shown before U5.4), the editing view turns back into the card
        if let Some(recipe) = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await?
        {
            let ingredients =
                crate::db::get_recipe_ingredients(pool, telegram_id, recipe_id).await?;
            let measurement_matches =
                crate::ingredient_editing::ingredients_to_measurement_matches(&ingredients);
            let recipe_name = recipe
                .recipe_name
                .unwrap_or_else(|| "Unnamed Recipe".to_string());
            let recipe_message = render_recipe_summary(
                &recipe_name,
                &measurement_matches,
                language_code,
                localization,
            );
            let keyboard = crate::bot::cards::card_keyboard(
                pool,
                telegram_id,
                recipe_id,
                language_code,
                localization,
            )
            .await;

            // Edit the editing message back to the recipe details
            let message_id = teloxide::types::MessageId(message_id);
            if let Err(e) = edit_html(bot, chat_id, message_id, recipe_message)
                .reply_markup(keyboard)
                .await
            {
                error_logging::log_internal_error(
                    &e,
                    "cancel_saved_editing",
                    "Failed to edit message back to recipe details when canceling ingredient editing",
                    Some(telegram_id),
                );
                // Fallback: delete the message
                let _ = bot.delete_message(chat_id, message_id).await;
            } else {
                crate::bot::cleanup::record_active_card(chat_id, message_id, recipe_id).await;
            }
        }
    }

    dialogue.exit().await?;
    crate::bot::home::send_home_at(bot, pool, chat_id, list_at, localization, language_code).await;
    Ok(())
}

/// Handle add ingredient button in editing saved ingredients state
async fn handle_add_ingredient_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    language_code: Option<&str>,
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
        send_transient(
            bot,
            q.message
                .as_ref()
                .context("Callback query should have a message")?
                .chat()
                .id,
            t_lang(localization, "add-ingredient-prompt", language_code),
        )
        .await?;

        // Transition to adding ingredient state
        dialogue
            .update(RecipeDialogueState::AddingIngredientToSavedRecipe {
                recipe_id,
                original_ingredients,
                current_matches,
                language_code: language_code.map(str::to_string),
                message_id,
            })
            .await?;
    }

    Ok(())
}
