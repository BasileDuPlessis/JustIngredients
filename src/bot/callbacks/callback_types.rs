//! Shared types for callback handlers

use std::sync::Arc;
use teloxide::types::CallbackQuery;

// Import HandlerContext from the parent bot module
use crate::bot::HandlerContext;

/// Parameters for review ingredients state operations
#[derive(Debug)]
pub struct ReviewIngredientsParams<'a> {
    pub ctx: &'a HandlerContext<'a>,
    pub q: &'a CallbackQuery,
    pub data: Option<&'a str>,
    pub ingredients: Option<&'a mut Vec<crate::text_processing::MeasurementMatch>>,
    pub ingredients_slice: Option<&'a [crate::text_processing::MeasurementMatch]>,
    pub recipe_name: &'a str,
    pub dialogue_lang_code: &'a Option<String>,
    pub message_id: Option<i32>,
    pub extracted_text: &'a str,
    pub recipe_name_from_caption: Option<&'a Option<String>>,
    pub dialogue: &'a crate::dialogue::RecipeDialogue,
    pub pool: Option<&'a Arc<sqlx::postgres::PgPool>>,
}

/// Parameters for saved ingredients editing operations
#[derive(Debug)]
pub struct SavedIngredientsParams<'a> {
    pub ctx: &'a HandlerContext<'a>,
    pub q: &'a CallbackQuery,
    pub data: Option<&'a str>,
    pub current_matches: Option<&'a mut Vec<crate::text_processing::MeasurementMatch>>,
    pub current_matches_slice: Option<&'a [crate::text_processing::MeasurementMatch]>,
    pub recipe_id: i64,
    pub original_ingredients: &'a [crate::db::Ingredient],
    pub language_code: &'a Option<String>,
    pub message_id: Option<i32>,
    pub dialogue: &'a crate::dialogue::RecipeDialogue,
    pub pool: Option<&'a Arc<sqlx::postgres::PgPool>>,
}
