//! Callback Handler module for processing inline keyboard callback queries

use crate::bot::html::{edit_html, send_html};
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{MaybeInaccessibleMessage, User};
use tracing::debug;
use tracing::Instrument;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import recipe callbacks module
use super::recipe_callbacks;

// Import UI builder functions
use crate::bot::ui_builder::{
    create_recipe_details_keyboard, create_recipe_instances_keyboard, render_recipe_card,
};

// Import database functions
// (using cached versions where available)

// Import workflow callbacks module
use super::workflow_callbacks;

// Import review callbacks module
use super::review_callbacks;

// Import editing callbacks module
use super::editing_callbacks;

// Import observability
use crate::observability;

// Import localization
use crate::localization::t_lang;

/// Handle callback queries from inline keyboards
pub async fn callback_handler(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let span = crate::observability::telegram_span("callback_handler", Some(q.from.id.0 as i64));
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {
        let start_time = std::time::Instant::now();

        // Check dialogue state
        let dialogue_state = dialogue.get().await?;
        debug!(user_id = %q.from.id, dialogue_state = ?dialogue_state, "Retrieved dialogue state");

        let data = q.data.as_deref().unwrap_or("");

        let result = match dialogue_state {
            Some(RecipeDialogueState::ReviewIngredients { .. }) => {
                review_callbacks::handle_review_ingredients_callbacks(
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
                editing_callbacks::handle_editing_saved_ingredients_callbacks(
                    &bot,
                    &q,
                    data,
                    pool.clone(),
                    &dialogue,
                    &localization,
                )
                .await
            }
            Some(RecipeDialogueState::EditingIngredient { .. }) => {
                handle_editing_ingredient_callbacks(&bot, &q, data, &dialogue, &localization).await
            }
            Some(RecipeDialogueState::EditingSavedIngredient { .. }) => {
                handle_editing_saved_ingredient_callbacks(&bot, &q, data, &dialogue, &localization)
                    .await
            }
            _ => Ok(()), // No state-specific handling needed
        };

        // Handle general callbacks that work in any state
        if let Some(msg) = &q.message {
            if crate::bot::recipe_selection::is_recipe_selection(data) {
                recipe_callbacks::handle_recipe_selection(
                    &bot,
                    msg,
                    data,
                    pool.clone(),
                    &q.from,
                    &localization,
                )
                .await?;
            } else if data.starts_with("recipe_instance:") {
                recipe_callbacks::handle_recipe_instance_selection(
                    &bot,
                    msg,
                    data,
                    pool.clone(),
                    &q.from,
                    &localization,
                )
                .await?;
            } else if data.starts_with("recipe_action:") {
                recipe_callbacks::handle_recipe_action(
                    &bot,
                    msg,
                    data,
                    pool.clone(),
                    &dialogue,
                    &q.from,
                    &localization,
                )
                .await?;
            } else if data == "back_to_recipes" {
                workflow_callbacks::handle_back_to_recipes(
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
                recipe_callbacks::handle_delete_recipe_confirmation(
                    &bot,
                    msg,
                    data,
                    pool.clone(),
                    &q.from,
                    &localization,
                    &dialogue,
                )
                .await?;
            } else if data.starts_with("page:") {
                workflow_callbacks::handle_recipes_pagination(
                    &bot,
                    msg,
                    data,
                    pool,
                    &q.from.language_code,
                    &localization,
                )
                .await?;
            } else if data.starts_with("workflow_") {
                workflow_callbacks::handle_workflow_button(
                    &bot,
                    &q,
                    data,
                    &pool,
                    &dialogue,
                    &localization,
                )
                .await?;
            }
        }

        // Answer the callback query to remove the loading state
        bot.answer_callback_query(q.id).await?;

        let duration = start_time.elapsed();
        observability::record_request_metrics("telegram_callback", 200, duration);

        result
    }
    .instrument(span)
    .await
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
) -> Result<()> {
    let span = crate::observability::telegram_span(
        "callback_handler_with_cache",
        Some(q.from.id.0 as i64),
    );
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {
        let start_time = std::time::Instant::now();

        // Check dialogue state
        let dialogue_state = dialogue.get().await?;
        debug!(user_id = %q.from.id, dialogue_state = ?dialogue_state, "Retrieved dialogue state");

        let data = q.data.as_deref().unwrap_or("");
        let stale = is_stale_button(data, dialogue_state.as_ref());

        let result = match dialogue_state {
            Some(RecipeDialogueState::ReviewIngredients { .. }) => {
                review_callbacks::handle_review_ingredients_callbacks(
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
                editing_callbacks::handle_editing_saved_ingredients_callbacks(
                    &bot,
                    &q,
                    data,
                    pool.clone(),
                    &dialogue,
                    &localization,
                )
                .await
            }
            Some(RecipeDialogueState::EditingIngredient { .. }) => {
                handle_editing_ingredient_callbacks(&bot, &q, data, &dialogue, &localization).await
            }
            Some(RecipeDialogueState::EditingSavedIngredient { .. }) => {
                handle_editing_saved_ingredient_callbacks(&bot, &q, data, &dialogue, &localization)
                    .await
            }
            _ => Ok(()), // No state-specific handling needed
        };

        // Handle general callbacks that work in any state. Recipe reads are not cached: they must
        // show a rename or an edit at once, and the database answers in a few milliseconds.
        let prefix_result: Result<()> = async {
            if let Some(msg) = &q.message {
                if crate::bot::recipe_selection::is_recipe_selection(data) {
                    handle_recipe_selection_by_name(
                        &bot,
                        msg,
                        data,
                        pool.clone(),
                        &q.from,
                        &localization,
                    )
                    .await?;
                } else if data.starts_with("recipe_instance:") {
                    handle_recipe_selection_by_id(
                        &bot,
                        msg,
                        data,
                        pool.clone(),
                        &q.from,
                        &localization,
                    )
                    .await?;
                } else if data.starts_with("recipe_action:") {
                    recipe_callbacks::handle_recipe_action(
                        &bot,
                        msg,
                        data,
                        pool.clone(),
                        &dialogue,
                        &q.from,
                        &localization,
                    )
                    .await?;
                } else if data == "back_to_recipes" {
                    workflow_callbacks::handle_back_to_recipes(
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
                    recipe_callbacks::handle_delete_recipe_confirmation(
                        &bot,
                        msg,
                        data,
                        pool.clone(),
                        &q.from,
                        &localization,
                        &dialogue,
                    )
                    .await?;
                } else if data.starts_with("page:") {
                    workflow_callbacks::handle_recipes_pagination(
                        &bot,
                        msg,
                        data,
                        pool,
                        &q.from.language_code,
                        &localization,
                    )
                    .await?;
                } else if data.starts_with("workflow_") {
                    workflow_callbacks::handle_workflow_button(
                        &bot,
                        &q,
                        data,
                        &pool,
                        &dialogue,
                        &localization,
                    )
                    .await?;
                }
            }
            Ok(())
        }
        .await;

        // Always answer, even when a handler failed (otherwise the button keeps spinning). A review
        // or edit button with no dialogue left (bot restarted, or a newer action) did nothing: say so
        // and remove the inactive keyboard.
        let mut answer = bot.answer_callback_query(q.id.clone());
        if stale {
            answer = answer.text(t_lang(
                &localization,
                "button-expired",
                q.from.language_code.as_deref(),
            ));
            if let Some(message) = &q.message {
                let _ = bot
                    .edit_message_reply_markup(message.chat().id, message.id())
                    .await;
            }
        }
        if let Err(error) = answer.await {
            debug!(error = %error, "Could not answer the callback query");
        }

        let duration = start_time.elapsed();
        observability::record_request_metrics("telegram_callback", 200, duration);

        result.and(prefix_result)
    }
    .instrument(span)
    .await
}

/// Whether a button only acts within a review or an ingredient edit.
fn needs_editing_dialogue(data: &str) -> bool {
    crate::bot::ui_builder::parse_ingredient_action(data).is_some()
        || matches!(
            data,
            "confirm"
                | "cancel_review"
                | "add_ingredient"
                | "add_more"
                | "cancel_ingredient_editing"
        )
}

/// Buttons older versions sent and nothing handles any more, e.g. the Cancel of a processing
/// message left when the bot restarted during an extraction.
const RETIRED_BUTTONS: [&str; 1] = ["cancel_processing"];

/// A review or edit button pressed while no review or edit is running: the bot restarted since
/// (dialogues are kept in memory) or a newer action replaced it, so the button did nothing.
/// Retired buttons are always stale.
fn is_stale_button(data: &str, state: Option<&RecipeDialogueState>) -> bool {
    RETIRED_BUTTONS.contains(&data)
        || needs_editing_dialogue(data)
            && !matches!(
                state,
                Some(
                    RecipeDialogueState::ReviewIngredients { .. }
                        | RecipeDialogueState::EditingSavedIngredients { .. }
                        | RecipeDialogueState::EditingIngredient { .. }
                        | RecipeDialogueState::EditingSavedIngredient { .. }
                )
            )
}

/// Handle callbacks when in EditingIngredient dialogue state
///
/// This function handles the cancel functionality for the focused editing interface:
/// - When user clicks "Cancel" during ingredient editing, restores the original recipe display
/// - Uses the original_message_id to replace the editing prompt back to the full recipe review
/// - Transitions dialogue state back to ReviewIngredients with proper message ID tracking
/// - Provides graceful fallback to sending new messages if editing fails
/// - Ensures seamless UX transition from focused editing back to full recipe review
async fn handle_editing_ingredient_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let dialogue_state = dialogue.get().await?;

    if let Some(RecipeDialogueState::EditingIngredient {
        recipe_name,
        ingredients,
        language_code,
        original_message_id,
        extracted_text,
        recipe_name_from_caption,
        ..
    }) = dialogue_state
    {
        if data == "cancel_ingredient_editing" {
            if let Some(msg) = &q.message {
                // Record user engagement metric for ingredient editing cancellation
                crate::observability::record_user_engagement_metrics(
                    q.from.id.0 as i64,
                    crate::observability::UserAction::IngredientEdit,
                    None, // No session duration for individual actions
                    language_code.as_deref(),
                );

                // Restore the original recipe display
                let review_message = crate::bot::ui_builder::render_review(
                    &ingredients,
                    language_code.as_deref(),
                    localization,
                );

                let keyboard = crate::bot::create_ingredient_review_keyboard(
                    &ingredients,
                    language_code.as_deref(),
                    localization,
                );

                // Use the original message ID to restore the recipe display
                if let Some(original_msg_id) = original_message_id {
                    match edit_html(
                        bot,
                        msg.chat().id,
                        teloxide::types::MessageId(original_msg_id),
                        review_message.clone(),
                    )
                    .reply_markup(keyboard.clone())
                    .await
                    {
                        Ok(_) => (),
                        Err(e) => {
                            crate::errors::error_logging::log_internal_error(
                                &e,
                                "handle_editing_ingredient_callbacks",
                                "Failed to restore original recipe display after cancel",
                                Some(msg.chat().id.0),
                            );
                            // Fallback: send new message if editing fails
                            send_html(bot, msg.chat().id, review_message)
                                .reply_markup(keyboard)
                                .await?;
                        }
                    }
                } else {
                    // No original message ID, send new message
                    send_html(bot, msg.chat().id, review_message)
                        .reply_markup(keyboard)
                        .await?;
                }

                // Reset dialogue state to review ingredients
                dialogue
                    .update(RecipeDialogueState::ReviewIngredients {
                        recipe_name,
                        ingredients,
                        language_code,
                        message_id: original_message_id, // Use original message ID for the restored display
                        extracted_text,
                        recipe_name_from_caption, // Preserve original caption info
                    })
                    .await?;
            }
        }
    }

    Ok(())
}

/// Handle callbacks when in EditingSavedIngredient dialogue state
///
/// This function handles the cancel functionality for editing a single ingredient in a saved recipe:
/// - When user clicks "Cancel" during ingredient editing, restores the editing list view
/// - Uses the original_message_id to replace the editing prompt back to the ingredient list
/// - Transitions dialogue state back to EditingSavedIngredients
async fn handle_editing_saved_ingredient_callbacks(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    dialogue: &RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    let dialogue_state = dialogue.get().await?;

    if let Some(RecipeDialogueState::EditingSavedIngredient {
        recipe_id,
        original_ingredients,
        current_matches,
        language_code,
        original_message_id,
        ..
    }) = dialogue_state
    {
        if data == "cancel_ingredient_editing" {
            if let Some(msg) = &q.message {
                // Record user engagement metric for ingredient editing cancellation
                crate::observability::record_user_engagement_metrics(
                    q.from.id.0 as i64,
                    crate::observability::UserAction::IngredientEdit,
                    None, // No session duration for individual actions
                    language_code.as_deref(),
                );

                // Restore the editing list view
                let edit_message = crate::bot::ui_builder::render_saved_editing(
                    None,
                    &current_matches,
                    language_code.as_deref(),
                    localization,
                );

                let keyboard = crate::bot::create_ingredient_review_keyboard(
                    &current_matches,
                    language_code.as_deref(),
                    localization,
                );

                // Use the original message ID to restore the editing list
                if let Some(original_msg_id) = original_message_id {
                    match edit_html(
                        bot,
                        msg.chat().id,
                        teloxide::types::MessageId(original_msg_id),
                        edit_message.clone(),
                    )
                    .reply_markup(keyboard.clone())
                    .await
                    {
                        Ok(_) => (),
                        Err(e) => {
                            crate::errors::error_logging::log_internal_error(
                                &e,
                                "handle_editing_saved_ingredient_callbacks",
                                "Failed to restore editing list after cancel",
                                Some(msg.chat().id.0),
                            );
                            // Fallback: send new message if editing fails
                            send_html(bot, msg.chat().id, edit_message)
                                .reply_markup(keyboard)
                                .await?;
                        }
                    }
                } else {
                    // No original message ID, send new message
                    send_html(bot, msg.chat().id, edit_message)
                        .reply_markup(keyboard)
                        .await?;
                }

                // Reset dialogue state to editing saved ingredients
                dialogue
                    .update(RecipeDialogueState::EditingSavedIngredients {
                        recipe_id,
                        original_ingredients,
                        current_matches,
                        language_code,
                        message_id: original_message_id, // Use original message ID for the restored display
                    })
                    .await?;
            }
        }
    }

    Ok(())
}

/// Cache-enabled recipe selection handler
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
async fn handle_recipe_selection_by_name(
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
    debug!(recipe_name = %recipe_name, "Handling recipe selection with caching");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Query for all recipes with this name for the user using cached function
    let recipes = crate::db::get_recipes_by_name(&pool, telegram_id, recipe_name).await?;

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

            let message = render_recipe_card(
                recipe
                    .recipe_name
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("Unnamed Recipe"),
                recipe.created_at,
                &ingredients,
                language_code.as_deref(),
                localization,
            );

            let keyboard =
                create_recipe_details_keyboard(recipe.id, language_code.as_deref(), localization);

            send_html(bot, chat_id, message)
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

            // Fetch ingredients for each recipe to show previews using cached function
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

/// Cache-enabled recipe instance selection handler
///
/// `from` is the sender of the callback query (`q.from`): the owner of every recipe read here.
async fn handle_recipe_selection_by_id(
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

    if recipe_id == 0 {
        debug!("Invalid recipe ID in callback data: {}", data);
        return Ok(());
    }

    debug!(recipe_id = %recipe_id, "Handling recipe instance selection with caching");

    // Extract chat id from the message
    let chat_id = match msg {
        MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
        MaybeInaccessibleMessage::Inaccessible(_) => {
            // Can't respond to inaccessible messages
            return Ok(());
        }
    };

    // Get recipe details using cached function
    let recipe = match crate::db::read_recipe_with_name(&pool, telegram_id, recipe_id).await? {
        Some(recipe) => recipe,
        None => {
            let message = t_lang(localization, "recipe-not-found", language_code.as_deref());
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    // Get ingredients using cached function
    let ingredients = crate::db::get_recipe_ingredients(&pool, telegram_id, recipe_id).await?;

    let message = render_recipe_card(
        recipe
            .recipe_name
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("Unnamed Recipe"),
        recipe.created_at,
        &ingredients,
        language_code.as_deref(),
        localization,
    );

    let keyboard =
        create_recipe_details_keyboard(recipe_id, language_code.as_deref(), localization);

    send_html(bot, chat_id, message)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review_state() -> RecipeDialogueState {
        RecipeDialogueState::ReviewIngredients {
            recipe_name: "Recipe".to_string(),
            ingredients: Vec::new(),
            language_code: None,
            message_id: None,
            extracted_text: String::new(),
            recipe_name_from_caption: None,
        }
    }

    #[test]
    fn review_buttons_without_a_dialogue_are_stale() {
        for data in [
            "confirm",
            "item_2",
            "qty_0",
            "delete_1",
            "back_review",
            "cancel_review",
            "add_more",
        ] {
            assert!(is_stale_button(data, None), "{data} after a restart");
            assert!(
                is_stale_button(data, Some(&RecipeDialogueState::Start)),
                "{data} with no action"
            );
            assert!(
                !is_stale_button(data, Some(&review_state())),
                "{data} in a review"
            );
        }
    }

    #[test]
    fn retired_buttons_are_always_stale() {
        assert!(is_stale_button("cancel_processing", None));
        assert!(is_stale_button("cancel_processing", Some(&review_state())));
    }

    #[test]
    fn buttons_that_work_in_any_state_are_never_stale() {
        for data in [
            "recipe_instance:3",
            "page:1",
            "workflow_list_recipes",
            "noop",
        ] {
            assert!(!is_stale_button(data, None), "{data}");
        }
    }
}
