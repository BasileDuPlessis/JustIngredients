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
        let pressed = q.message.as_ref().map(|message| message.id().0);
        let stale = is_stale_button(data, dialogue_state.as_ref(), pressed);

        // A stale button must not act on the current dialogue (an older list's ✅ would save
        // the newer review)
        let result = match dialogue_state.filter(|_| !stale) {
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
                if data.starts_with("recipe_instance:") {
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
                } else if data == "show_list" || data == "back_to_recipes" {
                    workflow_callbacks::handle_show_list(
                        &bot,
                        msg,
                        pool.clone(),
                        &dialogue,
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
/// Same, for data with a parameter: lists grouped by name (U3.1) and the card's statistics (U3.2).
const RETIRED_PREFIXES: [&str; 3] = [
    "select_recipe:",
    "select_recipe_h:",
    "recipe_action:statistics:",
];

/// A review or edit button that cannot act: no review or edit is running (the bot restarted
/// since, dialogues being kept in memory, or a newer action replaced it), or it is on an older
/// list than the running one. Retired buttons are always stale.
fn is_stale_button(data: &str, state: Option<&RecipeDialogueState>, pressed: Option<i32>) -> bool {
    if RETIRED_BUTTONS.contains(&data)
        || RETIRED_PREFIXES
            .iter()
            .any(|prefix| data.starts_with(prefix))
    {
        return true;
    }
    if !needs_editing_dialogue(data) {
        return false;
    }
    match state.and_then(editing_messages) {
        None => true,
        // Message ids grow within a chat: a list sent before the running one is older. A newer
        // one (a message re-sent because an edit failed) is accepted.
        Some(oldest) => {
            matches!((pressed, oldest), (Some(pressed), Some(oldest)) if pressed < oldest)
        }
    }
}

/// For a running review or edit, the oldest of the messages its buttons are on (`None` inside
/// when not recorded); `None` when no review or edit is running.
fn editing_messages(state: &RecipeDialogueState) -> Option<Option<i32>> {
    let oldest = |ids: &[Option<i32>]| ids.iter().flatten().min().copied();
    match state {
        RecipeDialogueState::ReviewIngredients { message_id, .. }
        | RecipeDialogueState::EditingSavedIngredients { message_id, .. }
        | RecipeDialogueState::AddingIngredientToSavedRecipe { message_id, .. } => {
            Some(*message_id)
        }
        RecipeDialogueState::EditingIngredient {
            message_id,
            original_message_id,
            ..
        }
        | RecipeDialogueState::EditingSavedIngredient {
            message_id,
            original_message_id,
            ..
        } => Some(oldest(&[*message_id, *original_message_id])),
        _ => None,
    }
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

    // One card per recipe, the list below it (U5.4)
    crate::bot::cards::show_card(
        bot,
        &pool,
        chat_id,
        Some(msg.id()),
        recipe_id,
        language_code.as_deref(),
        localization,
    )
    .await
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
            assert!(is_stale_button(data, None, None), "{data} after a restart");
            assert!(
                is_stale_button(data, Some(&RecipeDialogueState::Start), None),
                "{data} with no action"
            );
            assert!(
                !is_stale_button(data, Some(&review_state()), None),
                "{data} in a review"
            );
        }
    }

    #[test]
    fn buttons_of_an_older_list_are_stale() {
        let review = RecipeDialogueState::ReviewIngredients {
            recipe_name: "Recipe".to_string(),
            ingredients: Vec::new(),
            language_code: None,
            message_id: Some(50),
            extracted_text: String::new(),
            recipe_name_from_caption: None,
        };
        assert!(
            is_stale_button("confirm", Some(&review), Some(40)),
            "older list"
        );
        assert!(!is_stale_button("confirm", Some(&review), Some(50)));
        assert!(
            !is_stale_button("confirm", Some(&review), Some(60)),
            "a list re-sent after a failed edit"
        );
        assert!(
            !is_stale_button("confirm", Some(&review_state()), Some(40)),
            "id not recorded"
        );
        assert!(
            !is_stale_button("page:2", Some(&review), Some(40)),
            "not a review button"
        );
    }

    #[test]
    fn list_buttons_stay_active_while_adding_to_a_saved_recipe() {
        let adding = RecipeDialogueState::AddingIngredientToSavedRecipe {
            recipe_id: 1,
            original_ingredients: Vec::new(),
            current_matches: Vec::new(),
            language_code: None,
            message_id: Some(70),
        };
        assert!(!is_stale_button("item_0", Some(&adding), Some(70)));
    }

    #[test]
    fn retired_buttons_are_always_stale() {
        for data in [
            "select_recipe:Tarte",
            "select_recipe_h:abcd",
            "recipe_action:statistics:3",
        ] {
            assert!(is_stale_button(data, None, None), "{data}");
        }
        assert!(is_stale_button("cancel_processing", None, None));
        assert!(is_stale_button(
            "cancel_processing",
            Some(&review_state()),
            None
        ));
    }

    #[test]
    fn buttons_that_work_in_any_state_are_never_stale() {
        for data in [
            "recipe_instance:3",
            "page:1",
            "workflow_list_recipes",
            "noop",
        ] {
            assert!(!is_stale_button(data, None, None), "{data}");
        }
    }
}
