//! The end of the action: ✅ saves, ❌ cancels.

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tracing::debug;

// Import error logging utilities
use crate::errors::error_logging;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import UI components for the focused editing interface
use crate::bot::cleanup::{self, send_transient};
use crate::bot::create_ingredient_review_keyboard;
use crate::bot::home::send_home;
use crate::bot::ui_builder::render_review;

// Import callback types module
use crate::bot::callbacks::callback_types::ReviewIngredientsParams;

// Import dialogue manager functions
use crate::bot::dialogue_manager::save_ingredients_to_database;

use super::*;

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
pub(super) async fn handle_confirm_button(params: ReviewIngredientsParams<'_>) -> Result<()> {
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
        owner_id: crate::bot::telegram_id(q.from.id),
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
        // The list comes down below the message, with the buttons to fix them
        move_review_down(
            c.bot,
            c.chat_id,
            c.dialogue,
            c.localization,
            c.language_code,
        )
        .await?;
        return Ok(());
    }

    crate::observability::action(crate::observability::Action::ReviewConfirmed);

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
        // Deleted with the review's messages; its only exit "❌ Annuler" drops the creation
        let prompt_msg = c
            .bot
            .send_message(c.chat_id, recipe_name_prompt)
            .reply_markup(crate::bot::ui_builder::cancel_action_keyboard(
                c.language_code,
                c.localization,
            ))
            .await?;
        cleanup::track(c.chat_id, prompt_msg.id).await;
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
    let saved = match save_ingredients_to_database(
        c.pool,
        c.owner_id,
        c.extracted_text,
        c.ingredients,
        name,
        c.language_code,
    )
    .await
    {
        Ok(saved) => {
            crate::dialogue::end_after_write(c.dialogue).await;
            saved
        }
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

    // One history line replaces the review, then the recipe list
    remove_review_keyboard(&c).await;
    // The stored names: "Tarte (2)" when the user already has a "Tarte"
    crate::bot::home::send_created_lines(c.bot, c.chat_id, &saved, c.localization, c.language_code)
        .await;
    let list_at = saved
        .first()
        .map_or(crate::bot::home::ListAt::Start, |main| {
            crate::bot::home::ListAt::Recipe(main.recipe_id)
        });
    crate::bot::home::send_home_at(
        c.bot,
        c.pool,
        c.chat_id,
        list_at,
        c.localization,
        c.language_code,
    )
    .await;
    Ok(())
}

/// Sends the review list again at the bottom of the chat, with its buttons, after an answer
/// that pushed it out of view (a quantity check, a typed text not understood), and deletes the
/// one above (or only removes its buttons when Telegram refuses): its buttons stay at hand.
/// An open ingredient menu closes.
pub async fn move_review_down(
    bot: &Bot,
    chat_id: ChatId,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    let Some(RecipeDialogueState::ReviewIngredients {
        recipe_name,
        ingredients,
        language_code: state_language,
        message_id,
        extracted_text,
        recipe_name_from_caption,
        open_menu: _,
    }) = dialogue.get().await?
    else {
        return Ok(());
    };
    let review = crate::bot::html::send_html(
        bot,
        chat_id,
        render_review(&ingredients, language_code, localization),
    )
    .reply_markup(create_ingredient_review_keyboard(
        &ingredients,
        language_code,
        localization,
    ))
    .await?;
    cleanup::track(chat_id, review.id).await;
    if let Some(above) = message_id.map(teloxide::types::MessageId) {
        if bot.delete_message(chat_id, above).await.is_ok() {
            cleanup::untrack(chat_id, above).await;
        } else if let Err(error) = bot.edit_message_reply_markup(chat_id, above).await {
            debug!(error = %error, "Could not remove the buttons of the review list above");
        }
    }
    dialogue
        .update(RecipeDialogueState::ReviewIngredients {
            recipe_name,
            ingredients,
            language_code: state_language,
            message_id: Some(review.id.0),
            extracted_text,
            recipe_name_from_caption,
            open_menu: None,
        })
        .await?;
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
pub(super) async fn handle_cancel_review_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    dialogue_lang_code: Option<&str>,
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
        dialogue_lang_code,
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
            section: None,
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
