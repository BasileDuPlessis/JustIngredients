//! Recipe name dialogue module for handling conversation state with users.

use crate::text_processing::MeasurementMatch;
use serde::{Deserialize, Serialize};
use teloxide::dispatching::dialogue::Dialogue;

use crate::dialogue_storage::PgDialogueStorage;

// Import database types for editing saved ingredients
use crate::db::Ingredient;

/// What a typed answer replaces in the ingredient being edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IngredientField {
    /// The whole line ("3 cups flour"), parsed again.
    #[default]
    Line,
    /// Only the quantity (a number, or `-` for none); the unit and name stay.
    Quantity,
    /// Only the name; the quantity and unit stay.
    Name,
}

/// Represents the conversation state for recipe name dialogue.
///
/// Kept as JSON across deploys (`dialogue_storage`): a state written by the previous release is
/// read by the next one. A new field needs `#[serde(default)]`; a field or variant whose meaning
/// changes takes a new name, so an old state is dropped (its buttons say they are no longer
/// active) instead of being read with the wrong meaning.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum RecipeDialogueState {
    #[default]
    Start,
    ReviewIngredients {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        message_id: Option<i32>, // ID of the review message to edit
        extracted_text: String,  // The ingredient text read from the photo
        recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
        /// The ingredient whose menu (quantity, name, line, delete) is open on the review list:
        /// a typed answer applies to it.
        #[serde(default)]
        open_menu: Option<usize>,
    },
    EditingIngredient {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        editing_index: usize,
        /// What the typed answer replaces
        #[serde(default)]
        field: IngredientField,
        language_code: Option<String>,
        message_id: Option<i32>, // ID of the review message to edit after editing
        original_message_id: Option<i32>, // ID of the original recipe display message to replace during focused editing
        extracted_text: String,           // The ingredient text read from the photo
        recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
    },
    WaitingForRecipeNameAfterConfirm {
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        extracted_text: String, // The ingredient text read from the photo
        recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
        message_id: Option<i32>, // ID of the prompt message to edit with confirmation
    },
    RenamingRecipe {
        recipe_id: i64,
        current_name: String,
        language_code: Option<String>,
    },
    /// The note of a saved recipe: asked right after its creation (`creation`, with "Passer"), or
    /// from its card (with "Annuler" and "Effacer"). The typed answer is the note.
    EditingRecipeNote {
        recipe_id: i64,
        /// The recipe's name, for the prompt and the line that follows the answer
        recipe_name: String,
        creation: bool,
        language_code: Option<String>,
    },
    EditingSavedIngredients {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>, // Keep original for comparison
        current_matches: Vec<MeasurementMatch>, // Working copy for editing
        language_code: Option<String>,
        message_id: Option<i32>,
        /// The ingredient whose menu is open on the editing view: a typed answer applies to it.
        #[serde(default)]
        open_menu: Option<usize>,
    },
    EditingSavedIngredient {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>, // Keep original for comparison
        current_matches: Vec<MeasurementMatch>, // Working copy for editing
        editing_index: usize,                  // Which ingredient is being edited
        /// What the typed answer replaces
        #[serde(default)]
        field: IngredientField,
        language_code: Option<String>,
        message_id: Option<i32>,
        original_message_id: Option<i32>, // ID of the original recipe display message to replace during focused editing
    },
    AddingIngredientToSavedRecipe {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>, // Keep original for comparison
        current_matches: Vec<MeasurementMatch>, // Working copy for editing
        language_code: Option<String>,
        message_id: Option<i32>,
    },
}

impl RecipeDialogueState {
    /// Whether this state belongs to a running action (anything but `Start`).
    pub fn is_action(&self) -> bool {
        !matches!(self, Self::Start)
    }
}

/// A chat's dialogue, kept in Postgres across restarts (`dialogue_storage`)
pub type RecipeDialogue = Dialogue<RecipeDialogueState, PgDialogueStorage>;

/// Ends the action right after its result was written (a recipe saved, renamed, edited), before
/// anything is sent: a state left behind would write it again at the next answer. Tried twice; a
/// failure is logged and counted, not returned, since the chat must still show the result.
pub async fn end_after_write(dialogue: &RecipeDialogue) {
    if dialogue.exit().await.is_ok() {
        return;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    if let Err(error) = dialogue.exit().await {
        tracing::error!(error = %error, "Could not end the action after its write");
        crate::observability::error("database", "dialogue_exit");
    }
}
