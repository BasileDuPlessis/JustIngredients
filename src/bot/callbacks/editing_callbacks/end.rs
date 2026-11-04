//! The end of the action: ✅ saves, ❌ cancels.

use crate::bot::html::edit_html;
use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use std::sync::Arc;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI builder functions
use crate::bot::ui_builder::render_recipe_summary;

// Import HandlerContext
use crate::bot::cleanup::send_transient;
use crate::bot::home::send_home;
use crate::bot::HandlerContext;
use crate::ingredient_editing::to_new_ingredient;

// Import callback types module
use crate::bot::callbacks::callback_types::SavedIngredientsParams;

use super::*;

/// Handle confirm button for saved ingredients
pub(super) async fn handle_confirm_saved_ingredients_button(
    params: SavedIngredientsParams<'_>,
) -> Result<()> {
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
        if !write_changes(ctx, pool, telegram_id, recipe_id, chat_id, lang, &changes).await? {
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

    show_card_after_edit(ctx, pool, message, telegram_id, recipe_id, changed, card).await;

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

/// Writes a saved recipe's edit in one transaction: updates, additions and deletions, never a
/// half-applied edit. `false` when it failed: the user was told, and the edit stays open.
async fn write_changes(
    ctx: &HandlerContext<'_>,
    pool: &PgPool,
    telegram_id: i64,
    recipe_id: i64,
    chat_id: ChatId,
    lang: Option<&str>,
    changes: &crate::ingredient_editing::IngredientChanges,
) -> Result<bool> {
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
        return Ok(false);
    }
    Ok(true)
}

/// The edited recipe's card: its recorded cards get the new ingredients (nothing to update
/// when nothing changed) and the editing view goes; without a recorded card (older cards), the
/// editing view becomes the card.
async fn show_card_after_edit(
    ctx: &HandlerContext<'_>,
    pool: &PgPool,
    message: &teloxide::types::MaybeInaccessibleMessage,
    telegram_id: i64,
    recipe_id: i64,
    changed: bool,
    card: SavedRecipeCard,
) {
    let chat_id = message.chat().id;
    let lang = ctx.language_code;
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
