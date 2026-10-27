//! Message Handler module for processing incoming Telegram messages

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import localization
use crate::localization::{t_args_lang, t_lang};

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};
use crate::extraction::SharedExtractor;

// Import command handlers
use super::cleanup;
use super::command_handlers::{
    handle_help_command, handle_recipes_command, handle_search_command, handle_start_command,
    handle_unknown_command, handle_unsupported_message,
};
use super::commands::{route_text, Command, TextRoute};

// Import media handlers
use super::media_handlers::{handle_document_message, handle_photo_message};

// Import image processing
// use super::image_processing::process_ingredients_and_extract_matches;

// Import dialogue manager functions
use super::dialogue_manager::{
    handle_add_ingredient_input, handle_ingredient_edit_input, handle_ingredient_review_input,
    handle_quantity_correction_input, handle_recipe_name_after_confirm_input,
    handle_recipe_name_input, handle_recipe_rename_input, handle_saved_ingredient_edit_input,
    AddIngredientInputParams, DialogueContext, IngredientEditInputParams,
    IngredientReviewInputParams, QuantityCorrectionInputParams, RecipeNameAfterConfirmInputParams,
    RecipeNameInputParams, RecipeRenameInputParams, SavedIngredientEditInputParams,
};

// Import HandlerContext
use super::HandlerContext;

// Import observability
use crate::observability;

async fn handle_text_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    if let Some(text) = msg.text() {
        debug!(user_id = %msg.chat.id, message_length = text.len(), "Received text message from user");

        // Extract user's language code from Telegram
        let language_code = msg
            .from
            .as_ref()
            .and_then(|user| user.language_code.as_ref())
            .map(|s| s.as_str());

        let dialogue_state = dialogue.get().await?;
        let in_action = dialogue_state
            .as_ref()
            .is_some_and(RecipeDialogueState::is_action);

        match route_text(text, in_action) {
            TextRoute::Command(command) => {
                if in_action {
                    // The command ends the running action, which leaves no message behind;
                    // the home message from before the action stays
                    dialogue.exit().await?;
                    cleanup::delete_action_messages(bot, msg.chat.id).await;
                }
                return handle_command(bot, msg, pool, localization, language_code, command).await;
            }
            // Replies typed during an action are deleted when it ends
            TextRoute::ActionInput => cleanup::track(msg.chat.id, msg.id),
            TextRoute::FreeText => {}
        }

        match dialogue_state {
            Some(RecipeDialogueState::WaitingForRecipeName {
                extracted_text,
                ingredients,
                language_code: dialogue_lang_code,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe name input
                return handle_recipe_name_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeNameInputParams {
                        pool,
                        recipe_name_input: text,
                        extracted_text,
                        ingredients,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::WaitingForRecipeNameAfterConfirm {
                ingredients,
                language_code: dialogue_lang_code,
                extracted_text,
                recipe_name_from_caption: _,
                message_id,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe name input after ingredient confirmation
                return handle_recipe_name_after_confirm_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeNameAfterConfirmInputParams {
                        pool,
                        recipe_name_input: text,
                        ingredients,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        extracted_text,
                        message_id,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: dialogue_lang_code,
                message_id: review_message_id,
                extracted_text,
                recipe_name_from_caption: _,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle ingredient review commands
                return handle_ingredient_review_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    IngredientReviewInputParams {
                        pool,
                        review_input: text,
                        recipe_name,
                        ingredients,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        extracted_text,
                        review_message_id,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingIngredient {
                recipe_name,
                ingredients,
                editing_index,
                language_code: dialogue_lang_code,
                message_id,
                original_message_id: _original_message_id,
                extracted_text,
                recipe_name_from_caption,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle ingredient edit input
                return handle_ingredient_edit_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    IngredientEditInputParams {
                        edit_input: text,
                        recipe_name,
                        ingredients,
                        editing_index,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        message_id,
                        extracted_text,
                        user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
                        recipe_name_from_caption,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::RenamingRecipe {
                recipe_id,
                current_name,
                language_code: dialogue_lang_code,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle recipe rename input
                return handle_recipe_rename_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    RecipeRenameInputParams {
                        pool: &pool,
                        new_name_input: text,
                        recipe_id,
                        current_name,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::AddingIngredientToSavedRecipe {
                recipe_id,
                original_ingredients,
                current_matches,
                language_code: dialogue_lang_code,
                message_id,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle adding new ingredient input for saved recipes
                return handle_add_ingredient_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    AddIngredientInputParams {
                        pool: &pool,
                        add_input: text,
                        recipe_id,
                        original_ingredients: &original_ingredients,
                        current_matches: &current_matches,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        message_id,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingSavedIngredient {
                recipe_id,
                original_ingredients,
                current_matches,
                editing_index,
                language_code: dialogue_lang_code,
                message_id,
                original_message_id,
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle editing individual ingredient input for saved recipes
                return handle_saved_ingredient_edit_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    SavedIngredientEditInputParams {
                        pool: &pool,
                        edit_input: text,
                        recipe_id,
                        original_ingredients: &original_ingredients,
                        current_matches: &current_matches,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        message_id,
                        editing_index,
                        original_message_id,
                        user_input_message_id: Some(msg.id.0), // Add user's input message ID for reply functionality
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
                // Users should use buttons in this state, not type text
                let effective_language_code = language_code; // No dialogue language code available
                cleanup::send_transient(
                    bot,
                    msg.chat.id,
                    t_lang(
                        localization,
                        "use-buttons-instruction",
                        effective_language_code,
                    ),
                )
                .await?;
                return Ok(());
            }
            Some(RecipeDialogueState::AwaitingQuantityCorrection {
                recipe_name,
                ingredients,
                ingredient_index,
                language_code: dialogue_lang_code,
                extracted_text,
                recipe_name_from_caption,
                ..
            }) => {
                // Use dialogue language code if available, otherwise fall back to message language
                let effective_language_code = dialogue_lang_code.as_deref().or(language_code);

                // Handle quantity correction input
                return handle_quantity_correction_input(
                    DialogueContext {
                        bot,
                        msg,
                        dialogue,
                        localization,
                    },
                    QuantityCorrectionInputParams {
                        pool,
                        quantity_input: text,
                        recipe_name,
                        ingredients,
                        ingredient_index,
                        ctx: &HandlerContext {
                            bot,
                            localization,
                            language_code: effective_language_code,
                        },
                        extracted_text,
                        recipe_name_from_caption,
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::Start) | None => {
                // Continue with normal command handling
            }
        }

        bot.send_message(
            msg.chat.id,
            format!(
                "{} {}",
                t_args_lang(
                    localization,
                    "text-response",
                    &[("text", text)],
                    language_code
                ),
                t_lang(localization, "text-tip", language_code)
            ),
        )
        .await?;
    }
    Ok(())
}

/// Runs a typed command; `None` is an unknown command.
async fn handle_command(
    bot: &Bot,
    msg: &Message,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
    command: Option<Command>,
) -> Result<()> {
    match command {
        Some(Command::Start) => handle_start_command(bot, msg, localization, language_code).await,
        Some(Command::Help) => handle_help_command(bot, msg, localization, language_code).await,
        Some(Command::List) => {
            handle_recipes_command(bot, msg, pool, language_code, localization, None).await
        }
        Some(Command::Search(_)) => {
            handle_search_command(bot, msg, localization, language_code).await
        }
        None => handle_unknown_command(bot, msg, localization, language_code).await,
    }
}

/// Main message handler for Telegram bot interactions
///
/// Implements comprehensive message routing and dialogue state management.
/// This function orchestrates the entire bot interaction flow, handling different
/// message types and managing conversation state across multiple dialogue phases.
///
/// ## Message Routing Algorithm
///
/// ```text
/// 1. Extract message metadata (type, language, user info)
/// 2. Record telemetry metrics for monitoring
/// 3. Route by message type:
///    ├── Text → handle_text_message()
///    ├── Photo → handle_photo_message()
///    ├── Document → handle_document_message()
///    └── Other → handle_unsupported_message()
/// 4. Handle dialogue state transitions
/// 5. Record performance metrics
/// 6. Return result with error handling
/// ```
///
/// ## Dialogue State Machine
///
/// The bot maintains complex conversation state using `RecipeDialogueState`:
///
/// ```text
/// Start ────photo received────► WaitingForRecipeName
///    │                              │
///    │                              │ user provides name
///    │                              ▼
///    └───────────────► ReviewIngredients ────user confirms───► WaitingForRecipeNameAfterConfirm
///                              │                                      │
///                              │ user edits                           │ user provides name
///                              ▼                                      ▼
///                       EditingIngredient ──► ReviewIngredients ──► [Recipe Saved]
/// ```
///
/// ## State-Specific Message Handling
///
/// ### Text Messages
/// - **Any State**: Commands win (see `commands::route_text`) and end the running action
/// - **WaitingForRecipeName**: Process recipe name input with validation
/// - **ReviewIngredients**: Handle ingredient review commands (edit/delete/confirm)
/// - **EditingIngredient**: Process ingredient edit input
/// - **WaitingForRecipeNameAfterConfirm**: Handle post-confirmation recipe naming
///
/// ### Photo Messages
/// - Extract caption for automatic recipe naming
/// - Download and process image via OCR pipeline
/// - Transition to ingredient review interface
/// - Handle caption validation and fallback logic
///
/// ### Document Messages
/// - Validate image MIME types
/// - Process supported image formats
/// - Same OCR pipeline as photos (no caption support)
///
/// ## Language Detection & Localization
///
/// ```text
/// 1. Extract language_code from Telegram user.language_code
/// 2. Fallback to 'en' if not available
/// 3. Load appropriate Fluent bundle for localization
/// 4. Use localized messages throughout interaction
/// ```
///
/// ## Error Handling Strategy
///
/// - **Graceful Degradation**: Unsupported messages get helpful guidance
/// - **User-Friendly Messages**: Localized error responses
/// - **State Preservation**: Dialogue state maintained across errors
/// - **Logging**: Comprehensive error logging for debugging
///
/// ## Performance Monitoring
///
/// Tracks multiple metrics:
/// - Message type distribution
/// - Processing duration
/// - User language preferences
/// - Media attachment statistics
/// - Error rates by message type
///
/// ## Thread Safety
///
/// - Uses `Arc<PgPool>` for database connection sharing
/// - Dialogue state managed per chat_id
/// - Localization manager shared across requests
///
/// # Arguments
///
/// * `bot` - Telegram bot instance for sending responses
/// * `msg` - Incoming Telegram message to process
/// * `pool` - PostgreSQL connection pool for data persistence
/// * `dialogue` - Dialogue state manager for conversation flow
/// * `localization` - Localization manager for multi-language support
///
/// # Returns
///
/// Returns `Result<(), anyhow::Error>` indicating success or failure
///
/// # Message Type Support
///
/// - **Text**: Commands, dialogue input, recipe management
/// - **Photo**: Image processing with optional captions
/// - **Document**: Image files uploaded as documents
/// - **Unsupported**: Guidance for unsupported message types
pub async fn message_handler(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    deduplicator: Option<&crate::deduplication::SharedDeduplicator>,
    extractor: SharedExtractor,
) -> Result<()> {
    let span = crate::observability::telegram_span(
        "message_handler",
        msg.from.as_ref().map(|u| u.id.0 as i64),
    );
    let _enter = span.enter();

    if is_duplicate_message(deduplicator, msg.chat.id, msg.id)? {
        return Ok(());
    }

    dispatch_message(bot, msg, pool, dialogue, localization, extractor).await
}

/// Records the request in the deduplicator and returns `true` when it was already seen.
///
/// Each incoming message must go through this check exactly once: the deduplicator records
/// the request on the first call, so a second check would drop the message as a duplicate.
fn is_duplicate_message(
    deduplicator: Option<&crate::deduplication::SharedDeduplicator>,
    chat_id: teloxide::types::ChatId,
    message_id: teloxide::types::MessageId,
) -> Result<bool> {
    let Some(dedup) = deduplicator else {
        return Ok(false);
    };
    let request_id = crate::deduplication::RequestId::new(chat_id, message_id);
    if dedup.is_duplicate(&request_id)? {
        debug!(
            "Ignoring duplicate request: chat_id={}, message_id={}",
            chat_id, message_id
        );
        observability::record_telegram_duplicate_message();
        return Ok(true);
    }
    Ok(false)
}

/// Handles a message that already passed the duplicate check.
async fn dispatch_message(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    extractor: SharedExtractor,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let message_type = if msg.text().is_some() {
        "text"
    } else if msg.photo().is_some() {
        "photo"
    } else if msg.document().is_some() {
        "document"
    } else {
        "unsupported"
    };

    observability::record_telegram_message(message_type);

    let result = if msg.text().is_some() {
        handle_text_message(&bot, &msg, dialogue, pool, &localization).await
    } else if msg.photo().is_some() {
        handle_photo_message(&bot, &msg, dialogue, pool, &localization, &extractor).await
    } else if msg.document().is_some() {
        handle_document_message(&bot, &msg, dialogue, pool, &localization, &extractor).await
    } else {
        handle_unsupported_message(&bot, &msg, &localization).await
    };

    let duration = start_time.elapsed();
    observability::record_request_metrics("telegram_message", 200, duration);

    // Record enhanced Telegram performance metrics
    let message_size =
        msg.text().map(|t| t.len()).unwrap_or(0) + msg.caption().map(|c| c.len()).unwrap_or(0);
    let has_media = msg.photo().is_some() || msg.document().is_some();
    observability::record_telegram_performance_metrics(
        message_type,
        duration,
        msg.from.as_ref().map(|u| u.id.0 as i64),
        message_size,
        has_media,
    );

    result
}

/// Services shared by every message, built once at startup and cloned per message (all
/// fields are reference-counted).
#[derive(Clone)]
pub struct MessageServices {
    pub pool: Arc<PgPool>,
    pub localization: Arc<crate::localization::LocalizationManager>,
    pub cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
    pub deduplicator: Option<crate::deduplication::SharedDeduplicator>,
    pub extractor: SharedExtractor,
}

/// Cache-enabled message handler for improved performance
///
/// This version includes caching for database queries and OCR results
/// to reduce processing time and database load.
pub async fn message_handler_with_cache(
    bot: Bot,
    msg: Message,
    dialogue: RecipeDialogue,
    services: MessageServices,
) -> Result<()> {
    let MessageServices {
        pool,
        localization,
        cache: _,
        deduplicator,
        extractor,
    } = services;
    let span = crate::observability::telegram_span(
        "message_handler_with_cache",
        msg.from.as_ref().map(|u| u.id.0 as i64),
    );
    let _enter = span.enter();

    if is_duplicate_message(deduplicator.as_ref(), msg.chat.id, msg.id)? {
        return Ok(());
    }

    let start_time = std::time::Instant::now();

    // The regular dispatcher records its own message metrics. The duplicate check above
    // already ran and must not run again.
    dispatch_message(bot, msg, pool, dialogue, localization, extractor).await?;

    let duration = start_time.elapsed();
    observability::record_request_metrics("telegram_message_cached", 200, duration);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deduplication::{RequestDeduplicator, SharedDeduplicator};
    use teloxide::types::{ChatId, MessageId};

    #[test]
    fn is_duplicate_message_without_deduplicator_processes_every_message() -> Result<()> {
        assert!(!is_duplicate_message(None, ChatId(1), MessageId(1))?);
        assert!(!is_duplicate_message(None, ChatId(1), MessageId(1))?);
        Ok(())
    }

    #[test]
    fn is_duplicate_message_accepts_first_delivery_and_rejects_redelivery() -> Result<()> {
        let dedup: SharedDeduplicator = Arc::new(RequestDeduplicator::new(60, 100));
        assert!(!is_duplicate_message(
            Some(&dedup),
            ChatId(1),
            MessageId(7)
        )?);
        assert!(is_duplicate_message(Some(&dedup), ChatId(1), MessageId(7))?);
        assert!(!is_duplicate_message(
            Some(&dedup),
            ChatId(1),
            MessageId(8)
        )?);
        Ok(())
    }
}
