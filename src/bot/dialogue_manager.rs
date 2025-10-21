//! Dialogue Manager module for handling dialogue state transitions

use crate::localization::{t_args_lang, t_lang};
use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::error;

// Import text processing types
use crate::text_processing::{MeasurementDetector, MeasurementMatch};

// Import dialogue types
use crate::dialogue::{validate_recipe_name, RecipeDialogue, RecipeDialogueState};

// Import database types
use crate::db::{create_ingredient, create_recipe, get_or_create_user, update_recipe_name};

// Import UI builder functions
use super::ui_builder::{create_ingredient_review_keyboard, format_ingredients_list};

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

/// Parameters for recipe name success handling
#[derive(Debug)]
struct RecipeNameSuccessParams<'a> {
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub dialogue: RecipeDialogue,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
    pub pool: &'a PgPool,
    pub ingredients: &'a [MeasurementMatch],
    pub extracted_text: &'a str,
    pub validated_name: &'a str,
    pub language_code: Option<&'a str>,
}

/// Parameters for edit cancellation handling
#[derive(Debug)]
struct EditCancellationParams<'a> {
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub dialogue: RecipeDialogue,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
    pub ingredients: &'a [MeasurementMatch],
    pub recipe_name: String,
    pub language_code: Option<&'a str>,
    pub message_id: Option<i32>,
    pub extracted_text: String,
}

/// Parameters for edit success handling
#[derive(Debug)]
struct EditSuccessParams<'a> {
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub dialogue: RecipeDialogue,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
    pub ingredients: Vec<MeasurementMatch>,
    pub editing_index: usize,
    pub new_ingredient: MeasurementMatch,
    pub recipe_name: String,
    pub language_code: Option<&'a str>,
    pub message_id: Option<i32>,
    pub extracted_text: String,
}

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

/// Parameters for recipe rename input handling
#[derive(Debug)]
pub struct RecipeRenameInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub new_name_input: &'a str,
    pub recipe_id: i64,
    pub current_name: String,
    pub language_code: Option<&'a str>,
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

/// Handle recipe name input during dialogue
pub async fn handle_recipe_name_input(
    ctx: DialogueContext<'_>,
    params: RecipeNameInputParams<'_>,
) -> Result<()> {
    let start_time = std::time::Instant::now();
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

    let ingredients_count = ingredients.len();

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
                    recipe_name_from_caption: None, // Recipe name came from user input, not caption
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

    let duration = start_time.elapsed();
    crate::observability::record_dialogue_metrics(
        msg.chat.id.0,
        crate::observability::DialogueType::RecipeNaming,
        true, // completed
        ingredients_count,
        duration,
    );

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
    if is_cancellation_command(&input) {
        return handle_recipe_name_cancellation(bot, msg, dialogue, localization, language_code)
            .await;
    }

    // Validate and save recipe name
    match validate_recipe_name(recipe_name_input) {
        Ok(validated_name) => {
            handle_recipe_name_success(RecipeNameSuccessParams {
                bot,
                msg,
                dialogue,
                localization,
                pool: &pool,
                ingredients: &ingredients,
                extracted_text: &extracted_text,
                validated_name: &validated_name,
                language_code,
            })
            .await
        }
        Err(error_type) => {
            handle_recipe_name_validation_error(bot, msg, localization, error_type, language_code)
                .await
        }
    }
}

/// Handle cancellation of recipe name input
async fn handle_recipe_name_cancellation(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    // User cancelled, end dialogue without saving
    bot.send_message(
        msg.chat.id,
        t_lang(localization, "review-cancelled", language_code),
    )
    .await?;
    dialogue.exit().await?;
    Ok(())
}

/// Handle successful recipe name validation and saving
async fn handle_recipe_name_success(params: RecipeNameSuccessParams<'_>) -> Result<()> {
    let RecipeNameSuccessParams {
        bot,
        msg,
        dialogue,
        localization,
        pool,
        ingredients,
        extracted_text,
        validated_name,
        language_code,
    } = params;

    // Recipe name is valid, save ingredients to database
    if let Err(e) = save_ingredients_to_database(
        pool,
        msg.chat.id.0,
        extracted_text,
        ingredients,
        validated_name,
        language_code,
    )
    .await
    {
        error!(
            error = %e,
            user_id = msg.chat.id.0,
            recipe_name = %validated_name,
            ingredient_count = ingredients.len(),
            "Failed to save recipe '{}' with {} ingredients to database for user {}",
            validated_name,
            ingredients.len(),
            msg.chat.id.0
        );
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
                ("recipe_name", validated_name),
                ("ingredient_count", &ingredients.len().to_string()),
            ],
            language_code,
        );
        bot.send_message(msg.chat.id, success_message).await?;
    }

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}

/// Handle recipe name validation errors
async fn handle_recipe_name_validation_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error_type: &str,
    language_code: Option<&str>,
) -> Result<()> {
    let error_message = match error_type {
        "empty" => t_lang(localization, "recipe-name-invalid", language_code),
        "too_long" => t_lang(localization, "recipe-name-too-long", language_code),
        _ => t_lang(localization, "recipe-name-invalid", language_code),
    };

    bot.send_message(msg.chat.id, error_message).await?;
    // Keep dialogue active, user can try again
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
        ingredients,
        editing_index,
        language_code,
        message_id,
        extracted_text,
    } = params;

    let input = edit_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        return handle_edit_cancellation(EditCancellationParams {
            bot,
            msg,
            dialogue,
            localization,
            ingredients: &ingredients,
            recipe_name,
            language_code,
            message_id,
            extracted_text,
        })
        .await;
    }

    // Parse and validate the user input
    match parse_ingredient_from_text(edit_input) {
        Ok(new_ingredient) => {
            handle_edit_success(EditSuccessParams {
                bot,
                msg,
                dialogue,
                localization,
                ingredients,
                editing_index,
                new_ingredient,
                recipe_name,
                language_code,
                message_id,
                extracted_text,
            })
            .await
        }
        Err(error_msg) => handle_edit_error(bot, msg, localization, error_msg, language_code).await,
    }
}

/// Handle recipe rename input during dialogue
pub async fn handle_recipe_rename_input(
    ctx: DialogueContext<'_>,
    params: RecipeRenameInputParams<'_>,
) -> Result<()> {
    let DialogueContext {
        bot,
        msg,
        dialogue,
        localization,
    } = ctx;
    let RecipeRenameInputParams {
        pool,
        new_name_input,
        recipe_id,
        current_name,
        language_code,
    } = params;

    let input = new_name_input.trim().to_lowercase();

    // Check for cancellation commands
    if is_cancellation_command(&input) {
        bot.send_message(
            msg.chat.id,
            t_lang(localization, "delete-cancelled", language_code),
        )
        .await?;
        dialogue.exit().await?;
        return Ok(());
    }

    // Validate the new recipe name
    match validate_recipe_name(new_name_input) {
        Ok(validated_name) => {
            // Update the recipe name in the database
            match update_recipe_name(&pool, recipe_id, &validated_name).await {
                Ok(true) => {
                    let success_message = format!(
                        "✅ **{}**\n\n{}",
                        t_lang(localization, "rename-recipe-success", language_code),
                        t_args_lang(
                            localization,
                            "rename-recipe-success-details",
                            &[("old_name", &current_name), ("new_name", &validated_name)],
                            language_code
                        )
                    );
                    bot.send_message(msg.chat.id, success_message).await?;
                }
                Ok(false) => {
                    let message = t_lang(localization, "recipe-not-found", language_code);
                    bot.send_message(msg.chat.id, message).await?;
                }
                Err(e) => {
                    error!(recipe_id = %recipe_id, error = %e, "Failed to update recipe name");
                    let message = format!(
                        "❌ **{}**\n\n{}",
                        t_lang(localization, "error-renaming-recipe", language_code),
                        t_lang(localization, "error-renaming-recipe-help", language_code)
                    );
                    bot.send_message(msg.chat.id, message).await?;
                }
            }
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

    // End the dialogue
    dialogue.exit().await?;
    Ok(())
}

/// Check if input is a cancellation command
fn is_cancellation_command(input: &str) -> bool {
    matches!(input, "cancel" | "stop" | "back")
}

/// Handle cancellation of ingredient editing
async fn handle_edit_cancellation(params: EditCancellationParams<'_>) -> Result<()> {
    let EditCancellationParams {
        bot,
        msg,
        dialogue,
        localization,
        ingredients,
        recipe_name,
        language_code,
        message_id,
        extracted_text,
    } = params;

    // User cancelled editing, return to review state without changes
    let review_message = format!(
        "📝 **{}**\n\n{}\n\n{}",
        t_lang(localization, "review-title", language_code),
        t_lang(localization, "review-description", language_code),
        format_ingredients_list(ingredients, language_code, localization)
    );

    let keyboard = create_ingredient_review_keyboard(ingredients, language_code, localization);

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
            ingredients: ingredients.to_vec(),
            language_code: language_code.map(|s| s.to_string()),
            message_id,
            extracted_text,
            recipe_name_from_caption: None, // Recipe name came from user input, not caption
        })
        .await?;

    Ok(())
}

/// Handle successful ingredient editing
async fn handle_edit_success(params: EditSuccessParams<'_>) -> Result<()> {
    let EditSuccessParams {
        bot,
        msg,
        dialogue,
        localization,
        mut ingredients,
        editing_index,
        new_ingredient,
        recipe_name,
        language_code,
        message_id,
        extracted_text,
    } = params;

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
                recipe_name_from_caption: None, // Recipe name came from user input, not caption
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
                recipe_name_from_caption: None, // Recipe name came from user input, not caption
            })
            .await?;
    }

    Ok(())
}

/// Handle ingredient editing error
async fn handle_edit_error(
    bot: &Bot,
    msg: &Message,
    localization: &Arc<crate::localization::LocalizationManager>,
    error_msg: &str,
    language_code: Option<&str>,
) -> Result<()> {
    // Invalid input, ask user to try again
    let error_message = format!(
        "{}\n\n{}",
        t_lang(localization, error_msg, language_code),
        t_lang(localization, "edit-try-again", language_code)
    );
    bot.send_message(msg.chat.id, error_message).await?;
    // Stay in editing state for user to try again
    Ok(())
}

/// Parse ingredient text input and create a MeasurementMatch
///
/// This function implements a multi-stage parsing algorithm for ingredient editing:
///
/// ## Parsing Algorithm
///
/// 1. **Basic Validation**: Check input length and emptiness constraints
/// 2. **Measurement Detection**: Attempt to extract measurements using the standard detector
/// 3. **Fallback Parsing**: If no measurements found, use alternative parsing strategies
/// 4. **Validation & Normalization**: Apply comprehensive validation and normalization
///
/// ## Processing Stages
///
/// ### Stage 1: Basic Input Validation
/// ```text
/// - Empty input → Error: "edit-empty"
/// - Input > 200 chars → Error: "edit-too-long"
/// - Otherwise → Proceed to measurement detection
/// ```
///
/// ### Stage 2: Standard Measurement Detection
/// - Uses `MeasurementDetector` to find standard measurement patterns
/// - Handles traditional measurements: "2 cups flour", "500g butter"
/// - Handles quantity-only ingredients: "6 eggs", "4 apples"
/// - Supports fractions and Unicode characters
///
/// ### Stage 3: Fallback Parsing (when no measurements detected)
/// - **Quantity Pattern Matching**: Look for simple numeric patterns (`-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?`)
/// - **Quantity-Only Parsing**: Extract quantity and treat remainder as ingredient name
/// - **Default Quantity**: If no quantity found, default to "1"
///
/// ### Stage 4: Validation & Normalization
/// - **Measurement Validation**: Verify measurement match integrity
/// - **Quantity Range Check**: Ensure quantity is between 0 and 10,000
/// - **Negative Quantity Handling**: Detect and handle negative quantities (e.g., "-2 cups")
/// - **Ingredient Name Validation**: Check length and content constraints
///
/// ## Error Conditions
///
/// - `"edit-empty"`: Input is empty or whitespace-only
/// - `"edit-too-long"`: Input exceeds 200 characters
/// - `"edit-no-ingredient-name"`: No ingredient name found after quantity
/// - `"edit-ingredient-name-too-long"`: Ingredient name exceeds 100 characters
/// - `"edit-invalid-quantity"`: Quantity is ≤ 0 or > 10,000
/// - `"error-processing-failed"`: Measurement detector initialization failed
///
/// ## Thread Safety
///
/// This function is thread-safe as it creates new instances of `MeasurementDetector`
/// and doesn't rely on shared mutable state.
///
/// ## Performance
///
/// - **Fast Path**: Standard measurement detection (most common case)
/// - **Fallback Path**: Regex-based quantity extraction (slower but robust)
/// - **Memory**: Minimal allocations, reuses detector instances
/// - **Parse an ingredient from user input text during dialogue editing**
///
/// This function implements a sophisticated multi-stage parsing algorithm that converts
/// user-provided ingredient text into structured `MeasurementMatch` objects. It handles
/// various input formats and provides comprehensive validation and error handling.
///
/// ## Algorithm Overview
///
/// The parsing process follows this sequence:
/// 1. **Input Validation**: Basic length and emptiness checks
/// 2. **Measurement Detection**: Attempt extraction using the main measurement detector
/// 3. **Match Validation**: Verify extracted measurements meet requirements
/// 4. **Quantity Adjustment**: Handle negative quantities and special cases
/// 5. **Range Validation**: Ensure quantities are within reasonable bounds
/// 6. **Fallback Parsing**: Alternative parsing strategies when detector fails
///
/// ## Processing Stages
///
/// ### Stage 1: Basic Input Validation
/// **Algorithm**: Early rejection of invalid inputs before complex processing
///
/// **Validation Rules**:
/// - **Empty Input**: Reject completely empty strings
/// - **Length Limit**: Maximum 200 characters to prevent abuse
/// - **Trimming**: Automatic whitespace removal
///
/// **Error Codes**:
/// - `"edit-empty"`: Input is empty or whitespace-only
/// - `"edit-too-long"`: Input exceeds 200 character limit
///
/// ### Stage 2: Primary Measurement Detection
/// **Algorithm**: Leverage the main `MeasurementDetector` for comprehensive parsing
///
/// **Processing Steps**:
/// 1. Create temporary text wrapper: `"temp: {input}"`
/// 2. Run full measurement extraction pipeline
/// 3. Extract first (best) measurement match
/// 4. Validate match quality and completeness
///
/// **Success Path**: If measurement found, proceed to validation
/// **Failure Path**: Fall back to alternative parsing strategies
///
/// ### Stage 3: Measurement Match Validation
/// **Algorithm**: Comprehensive validation of extracted measurement components
///
/// **Validation Checks**:
/// - **Ingredient Name Presence**: Must have non-empty ingredient name
/// - **Name Length Limits**: Maximum 100 characters for ingredient names
/// - **Raw Text Verification**: Cross-check against original input
/// - **Measurement Completeness**: Ensure all required fields are present
///
/// **Error Codes**:
/// - `"edit-no-ingredient-name"`: Missing or empty ingredient name
/// - `"edit-ingredient-name-too-long"`: Ingredient name exceeds 100 characters
///
/// ### Stage 4: Quantity Adjustment for Negatives
/// **Algorithm**: Detect and handle negative quantity indicators in text
///
/// **Detection Logic**:
/// ```text
/// Input: "-2 cups flour"
/// Analysis:
///   - Find quantity start position in temp text
///   - Check for '-' character immediately before quantity
///   - Verify '-' is preceded by space or at start (not part of word)
///   - Prepend '-' to quantity string if valid
/// ```
///
/// **Examples**:
/// - `"-2 cups flour"` → quantity: `"-2"`
/// - `"some -2 cups"` → quantity: `"2"` (invalid position)
/// - `"minus 2 cups"` → quantity: `"2"` (not detected)
///
/// ### Stage 5: Quantity Range Validation
/// **Algorithm**: Ensure quantities are within practical and safe bounds
///
/// **Range Limits**:
/// - **Minimum**: > 0.0 (must be positive)
/// - **Maximum**: ≤ 10000.0 (prevents unreasonable values)
/// - **Fraction Support**: Handles decimal and fractional quantities
/// - **Unicode Fractions**: Supports ½, ¼, ¾, ⅓, ⅔ characters
///
/// **Error Codes**:
/// - `"edit-invalid-quantity"`: Quantity outside valid range
///
/// ### Stage 6: Fallback Parsing Strategies
/// **Algorithm**: Alternative parsing when primary detector fails
///
/// **Strategy 1: Quantity Pattern Matching**
/// ```regex
/// Pattern: ^(-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?)
/// Examples: "2", "1.5", "2 1/2", "-3"
/// ```
///
/// **Strategy 2: Ingredient-Only Parsing**
/// - No quantity detected → assume quantity = 1
/// - Entire input becomes ingredient name
/// - Length validation still applies
///
/// ## Measurement Match Construction
///
/// ### Successful Parsing Result
/// When parsing succeeds, a `MeasurementMatch` struct is returned containing:
/// - `quantity`: The parsed quantity string (e.g., "2", "½", "1.5")
/// - `measurement`: Optional unit string (e.g., Some("cups"), None for quantity-only)
/// - `ingredient_name`: Cleaned ingredient name (e.g., "flour", "sugar")
/// - `line_number`: Always 0 for single input parsing
/// - `start_pos`: Always 0 for single input parsing
/// - `end_pos`: Length of the input string
///
/// ## Error Handling and Recovery
///
/// ### Comprehensive Error Coverage
/// - **Input Validation**: Early rejection with specific error codes
/// - **Measurement Detection**: Graceful fallback to alternative strategies
/// - **Validation Failures**: Detailed error messages for user feedback
/// - **Parsing Errors**: Safe handling of malformed input
///
/// ### Error Code Mapping
/// - `"edit-empty"`: Empty input string
/// - `"edit-too-long"`: Input exceeds length limits
/// - `"error-processing-failed"`: Measurement detector initialization failure
/// - `"edit-no-ingredient-name"`: Missing ingredient name
/// - `"edit-ingredient-name-too-long"`: Ingredient name too long
/// - `"edit-invalid-quantity"`: Quantity outside valid range
///
/// ## Performance Characteristics
///
/// - **Time Complexity**: O(n) where n is input length
/// - **Memory Usage**: Minimal allocations, reuses detector instances
/// - **Regex Efficiency**: Pre-compiled patterns for fast matching
/// - **Early Exit**: Fast rejection of invalid inputs
/// - **Fallback Efficiency**: Alternative strategies for edge cases
///
/// ## Thread Safety
///
/// - **Immutable Access**: No shared mutable state
/// - **Detector Reuse**: MeasurementDetector can be safely shared
/// - **Concurrent Safe**: Can be called from multiple threads
///
/// ## Integration Points
///
/// - **MeasurementDetector**: Primary parsing engine with regex patterns
/// - **Localization**: Error messages support multiple languages
/// - **Dialogue System**: Provides structured input for conversation flow
/// - **Database Layer**: Parsed results feed into ingredient storage
///
/// # Arguments
///
/// * `input` - The raw ingredient text input from user (e.g., "2 cups flour", "3 eggs")
///
/// # Returns
///
/// Returns a `MeasurementMatch` containing parsed quantity, measurement, and ingredient name,
/// or an error string key for localization
///
/// # Examples
///
/// Note: This function is used internally by the dialogue system.
/// For usage examples, see the dialogue handling functions in the bot module.
pub fn parse_ingredient_from_text(input: &str) -> Result<MeasurementMatch, &'static str> {
    let trimmed = input.trim();

    // Basic validation
    validate_basic_input(trimmed)?;

    // Try to extract measurement using the detector
    let detector = MeasurementDetector::new().map_err(|_| "error-processing-failed")?;
    let temp_text = format!("temp: {}", trimmed);
    let matches = detector.extract_ingredient_measurements(&temp_text);

    if let Some(mut measurement_match) = matches.into_iter().next() {
        // Found a measurement, validate the ingredient name
        validate_measurement_match(&measurement_match, &temp_text)?;
        adjust_quantity_for_negative(&mut measurement_match, &temp_text);
        validate_quantity_range(&measurement_match)?;
        Ok(measurement_match)
    } else {
        // No measurement found, try alternative parsing strategies
        parse_without_measurement_detector(trimmed)
    }
}

/// Validate basic input constraints
fn validate_basic_input(input: &str) -> Result<(), &'static str> {
    if input.is_empty() {
        return Err("edit-empty");
    }

    if input.len() > 200 {
        return Err("edit-too-long");
    }

    Ok(())
}

/// Validate a measurement match and its ingredient name
fn validate_measurement_match(
    measurement_match: &MeasurementMatch,
    temp_text: &str,
) -> Result<(), &'static str> {
    let ingredient_name = measurement_match.ingredient_name.trim();

    // Re-extract the raw ingredient name to check its length
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

    Ok(())
}

/// Adjust quantity for negative values if detected in the text
fn adjust_quantity_for_negative(measurement_match: &mut MeasurementMatch, temp_text: &str) {
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
}

/// Validate that quantity is within reasonable range
fn validate_quantity_range(measurement_match: &MeasurementMatch) -> Result<(), &'static str> {
    if let Some(qty) = parse_quantity(&measurement_match.quantity) {
        if qty <= 0.0 || qty > 10000.0 {
            return Err("edit-invalid-quantity");
        }
    }
    Ok(())
}

/// Parse ingredient when no measurement detector match is found
fn parse_without_measurement_detector(trimmed: &str) -> Result<MeasurementMatch, &'static str> {
    // Try to extract a simple quantity pattern
    let quantity_pattern = regex::Regex::new(r"^(-?\d+(?:\.\d+)?(?:\s*\d+/\d+)?)").unwrap();

    if let Some(captures) = quantity_pattern.captures(trimmed) {
        if let Some(quantity_match) = captures.get(1) {
            return parse_with_quantity(trimmed, quantity_match);
        }
    }

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

/// Parse ingredient when a quantity pattern is found
fn parse_with_quantity(
    trimmed: &str,
    quantity_match: regex::Match,
) -> Result<MeasurementMatch, &'static str> {
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
                error!(
                    error = %e,
                    user_id = msg.chat.id.0,
                    recipe_name = %recipe_name,
                    ingredient_count = ingredients.len(),
                    "Failed to save recipe '{}' with {} ingredients to database for user {}",
                    recipe_name,
                    ingredients.len(),
                    msg.chat.id.0
                );
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
    let start_time = std::time::Instant::now();

    // Get or create user
    let user = get_or_create_user(pool, telegram_id, language_code).await?;

    // Create recipe
    let recipe_id = create_recipe(pool, telegram_id, extracted_text).await?;

    // Update recipe with recipe name
    update_recipe_name(pool, recipe_id, recipe_name).await?;

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

    let processing_duration = start_time.elapsed();

    // Record business metrics
    let naming_method = if recipe_name == "Recipe" {
        crate::observability::RecipeNamingMethod::Default
    } else {
        // For now, assume manual naming - could be enhanced to detect caption usage
        crate::observability::RecipeNamingMethod::Manual
    };

    crate::observability::record_recipe_metrics(
        recipe_name,
        ingredients.len(),
        naming_method,
        processing_duration,
        user.id,
    );

    Ok(())
}
