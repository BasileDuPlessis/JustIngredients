//! Recipe name dialogue module for handling conversation state with users.

use crate::text_processing::MeasurementMatch;
use serde::{Deserialize, Serialize};
use teloxide::dispatching::dialogue::{Dialogue, InMemStorage};

// Import database types for editing saved ingredients
use crate::db::Ingredient;

/// Represents the conversation state for recipe name dialogue
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum RecipeDialogueState {
    #[default]
    Start,
    WaitingForRecipeName {
        extracted_text: String,
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
    },
    ReviewIngredients {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        message_id: Option<i32>, // ID of the review message to edit
        extracted_text: String,  // Store the original OCR text
        recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
    },
    EditingIngredient {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        editing_index: usize,
        language_code: Option<String>,
        message_id: Option<i32>, // ID of the review message to edit after editing
        original_message_id: Option<i32>, // ID of the original recipe display message to replace during focused editing
        extracted_text: String,           // Store the original OCR text
    },
    WaitingForRecipeNameAfterConfirm {
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        extracted_text: String, // Store the original OCR text
        recipe_name_from_caption: Option<String>, // Track recipe name from photo caption
    },
    RenamingRecipe {
        recipe_id: i64,
        current_name: String,
        language_code: Option<String>,
    },
    EditingSavedIngredients {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>, // Keep original for comparison
        current_matches: Vec<MeasurementMatch>, // Working copy for editing
        language_code: Option<String>,
        message_id: Option<i32>,
    },
    EditingSavedIngredient {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>, // Keep original for comparison
        current_matches: Vec<MeasurementMatch>, // Working copy for editing
        editing_index: usize,                  // Which ingredient is being edited
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

/// Type alias for our recipe dialogue
pub type RecipeDialogue = Dialogue<RecipeDialogueState, InMemStorage<RecipeDialogueState>>;
