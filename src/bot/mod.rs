//! Bot module for handling Telegram interactions
//!
//! This module is split into several submodules for better organization:
//! - `message_handler`: Handles incoming text, photo, and document messages
//! - `callback_handler`: Handles inline keyboard callback queries
//! - `ui_builder`: Creates keyboards and formats messages
//! - `dialogue_manager`: Manages dialogue state transitions and validation

pub mod callback_handler;
pub mod dialogue_manager;
pub mod message_handler;
pub mod ui_builder;
pub mod ui_components;

// Common context structures for handler functions
use teloxide::Bot;
use crate::localization::LocalizationManager;

/// Common context for bot handlers containing shared dependencies
#[derive(Debug)]
pub struct HandlerContext<'a> {
    pub bot: &'a Bot,
    pub localization: &'a std::sync::Arc<LocalizationManager>,
    pub language_code: Option<&'a str>,
}

// Re-export main handler functions for use in main.rs
pub use callback_handler::{callback_handler, callback_handler_with_cache};
pub use message_handler::{message_handler, message_handler_with_cache};

// Re-export utility functions that might be used elsewhere
pub use dialogue_manager::save_ingredients_to_database;
pub use crate::validation::parse_ingredient_from_text;
pub use message_handler::{
    download_and_process_image, download_file, process_ingredients_and_extract_matches,
};
pub use ui_builder::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard,
    create_recipes_pagination_keyboard, format_ingredients_list,
};
