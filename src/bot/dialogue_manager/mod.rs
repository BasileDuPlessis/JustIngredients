//! Typed answers during an action, one module per flow: the review of a photo (`review`), the
//! name asked after ✅ and the save (`recipe_name`), a rename (`rename`), the edit of a saved
//! recipe (`saved_editing`), and the confirm and cancel words (`words`). The parameter structs
//! shared by the flows are here; everything is re-exported, so callers keep writing
//! `dialogue_manager::…`.

use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;

// Import text processing types
use crate::text_processing::MeasurementMatch;

// Import dialogue types
use crate::dialogue::{IngredientField, RecipeDialogue};

// Import database types
use crate::db::Ingredient;

// Import HandlerContext
use super::HandlerContext;

mod recipe_name;
mod rename;
mod review;
mod saved_editing;
mod words;

pub use recipe_name::*;
pub use rename::*;
pub use review::*;
pub use saved_editing::*;
pub(crate) use words::*;

/// Check if the error is a "message not modified" Telegram API error
fn is_message_not_modified_error(error: &teloxide::RequestError) -> bool {
    error.to_string().contains("message is not modified")
}

/// Parameters for ingredient review input handling
#[derive(Debug)]
pub struct IngredientReviewInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub review_input: &'a str,
    /// Name from the caption or the page title, if any.
    pub recipe_name_from_caption: Option<String>,
    pub ingredients: Vec<MeasurementMatch>,
    pub ctx: &'a HandlerContext<'a>,
    pub extracted_text: String,
    /// The review message (ingredient list), deleted once the review is confirmed.
    pub review_message_id: Option<i32>,
    pub recipe_name: String,
    /// The ingredient whose menu is open on the review list: a typed answer applies to it.
    pub open_menu: Option<usize>,
}

/// Parameters for recipe name success handling
#[derive(Debug)]
struct RecipeNameSuccessParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    pool: &'a PgPool,
    ingredients: &'a [MeasurementMatch],
    extracted_text: &'a str,
    validated_name: &'a str,
    message_id: Option<i32>, // ID of the prompt message to edit with confirmation
}

/// Parameters for edit cancellation handling
#[derive(Debug)]
struct EditCancellationParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    ingredients: &'a [MeasurementMatch],
    recipe_name: String,
    message_id: Option<i32>,
    extracted_text: String,
    recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
}

/// Parameters for edit success handling
#[derive(Debug)]
struct EditSuccessParams<'a> {
    ctx: &'a HandlerContext<'a>,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    ingredients: Vec<MeasurementMatch>,
    editing_index: usize,
    new_ingredient: MeasurementMatch,
    recipe_name: String,
    message_id: Option<i32>,
    extracted_text: String,
    user_input_message_id: Option<i32>, // ID of the user's input message for reply functionality
    recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
}

/// Common context for dialogue handlers
#[derive(Debug)]
pub struct DialogueContext<'a> {
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub dialogue: RecipeDialogue,
    pub localization: &'a Arc<crate::localization::LocalizationManager>,
}

/// Parameters for recipe name input after confirmation
#[derive(Debug)]
pub struct RecipeNameAfterConfirmInputParams<'a> {
    pub pool: Arc<PgPool>,
    pub recipe_name_input: &'a str,
    pub ingredients: Vec<MeasurementMatch>,
    pub ctx: &'a HandlerContext<'a>,
    pub extracted_text: String,
    pub message_id: Option<i32>, // ID of the prompt message to edit with confirmation
}

/// Parameters for recipe rename input handling
#[derive(Debug)]
pub struct RecipeRenameInputParams<'a> {
    pub pool: &'a PgPool,
    pub new_name_input: &'a str,
    pub recipe_id: i64,
    pub current_name: String,
    pub ctx: &'a HandlerContext<'a>,
}

/// Parameters for ingredient edit input handling
#[derive(Debug)]
pub struct IngredientEditInputParams<'a> {
    pub edit_input: &'a str,
    pub recipe_name: String,
    pub ingredients: Vec<MeasurementMatch>,
    pub editing_index: usize,
    /// What the typed answer replaces
    pub field: IngredientField,
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
    pub extracted_text: String,
    pub user_input_message_id: Option<i32>, // ID of the user's input message for reply functionality
    pub recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
}

/// Parameters for adding ingredient input handling (saved recipes)
#[derive(Debug)]
pub struct AddIngredientInputParams<'a> {
    pub pool: &'a PgPool,
    pub add_input: &'a str,
    pub recipe_id: i64,
    pub original_ingredients: &'a [Ingredient],
    pub current_matches: &'a [MeasurementMatch],
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
}

/// Parameters for saved ingredient edit input handling
#[derive(Debug)]
pub struct SavedIngredientEditInputParams<'a> {
    pub pool: &'a PgPool,
    pub edit_input: &'a str,
    pub recipe_id: i64,
    pub original_ingredients: &'a [Ingredient],
    pub current_matches: &'a [MeasurementMatch],
    pub ctx: &'a HandlerContext<'a>,
    pub message_id: Option<i32>,
    pub editing_index: usize,
    /// What the typed answer replaces
    pub field: IngredientField,
    pub original_message_id: Option<i32>, // ID of the original recipe display message to replace
    pub user_input_message_id: Option<i32>, // ID of the user's input message for reply functionality
}

/// Parameters for returning to saved ingredients review
#[derive(Debug)]
struct ReturnToSavedIngredientsReviewParams<'a> {
    bot: &'a Bot,
    msg: &'a Message,
    dialogue: RecipeDialogue,
    localization: &'a Arc<crate::localization::LocalizationManager>,
    recipe_id: i64,
    original_ingredients: &'a [Ingredient],
    current_matches: &'a [MeasurementMatch],
    language_code: Option<&'a str>,
    message_id: Option<i32>,
    user_input_message_id: Option<i32>, // ID of the user's input message for reply functionality
}
