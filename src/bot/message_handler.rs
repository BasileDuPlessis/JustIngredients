//! Message Handler module for processing incoming Telegram messages

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};

// Import command handlers
use super::command_handlers::{
    handle_help_command, handle_recipes_command, handle_start_command, handle_unsupported_message,
};

// Import media handlers
use super::media_handlers::{handle_document_message, handle_photo_message};

// Import image processing
// use super::image_processing::process_ingredients_and_extract_matches;

// Import dialogue manager functions
use super::dialogue_manager::{
    handle_add_ingredient_input, handle_ingredient_edit_input, handle_ingredient_review_input,
    handle_recipe_name_after_confirm_input, handle_recipe_name_input, handle_recipe_rename_input,
    handle_saved_ingredient_edit_input, AddIngredientInputParams, DialogueContext,
    IngredientEditInputParams, IngredientReviewInputParams, RecipeNameAfterConfirmInputParams,
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

        // Check dialogue state first
        let dialogue_state = dialogue.get().await?;
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
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::ReviewIngredients {
                recipe_name,
                ingredients,
                language_code: dialogue_lang_code,
                message_id: _,
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
                extracted_text,
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
                    },
                )
                .await;
            }
            Some(RecipeDialogueState::EditingSavedIngredients { .. }) => {
                // Users should use buttons in this state, not type text
                let effective_language_code = language_code; // No dialogue language code available
                bot.send_message(
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
            Some(RecipeDialogueState::Start) | None => {
                // Continue with normal command handling
            }
        }

        // Handle /start command
        if text == "/start" {
            return handle_start_command(bot, msg, localization, language_code).await;
        }
        // Handle /help command
        else if text == "/help" {
            return handle_help_command(bot, msg, localization, language_code).await;
        }
        // Handle /recipes command
        else if text == "/recipes" {
            return handle_recipes_command(bot, msg, pool, language_code, localization).await;
        }
        // Handle regular text messages
        else {
            bot.send_message(
                msg.chat.id,
                format!(
                    "{} {}",
                    t_lang(localization, "text-response", language_code),
                    t_lang(localization, "text-tip", language_code)
                ),
            )
            .await?;
        }
    }
    Ok(())
}

/// Main message handler for Telegram bot interactions

/// Main message handler for Telegram bot interactions

/// Main message handler for Telegram bot interactions

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
/// - **Start State**: Handle `/start`, `/help`, `/recipes` commands
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
) -> Result<()> {
    let span = crate::observability::telegram_span(
        "message_handler",
        msg.from.as_ref().map(|u| u.id.0 as i64),
    );
    let _enter = span.enter();

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
        handle_photo_message(&bot, &msg, dialogue, pool, &localization).await
    } else if msg.document().is_some() {
        handle_document_message(&bot, &msg, dialogue, pool, &localization).await
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

/// Cache-enabled message handler for improved performance
///
/// This version includes caching for database queries and OCR results
/// to reduce processing time and database load.
pub async fn message_handler_with_cache(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    _cache: Arc<std::sync::Mutex<crate::cache::CacheManager>>,
) -> Result<()> {
    // For now, delegate to the original handler
    // TODO: Integrate caching into specific operations
    message_handler(bot, msg, pool, dialogue, localization).await
}
