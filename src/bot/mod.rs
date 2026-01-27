//! Bot module for handling Telegram interactions
//!
//! This module is split into several submodules for better organization:
//! - `callbacks`: All callback query handling (organized into submodules)
//! - `message_handler`: Handles incoming text, photo, and document messages
//! - `ui_builder`: Creates keyboards and formats messages
//! - `dialogue_manager`: Manages dialogue state transitions and validation

pub mod callbacks;
pub mod command_handlers;
pub mod dialogue_manager;
pub mod image_processing;
pub mod media_handlers;
pub mod message_handler;
pub mod ui_builder;
pub mod ui_components;

// Common context structures for handler functions
use crate::localization::LocalizationManager;
use teloxide::Bot;

/// Common context for bot handlers containing shared dependencies
#[derive(Debug)]
pub struct HandlerContext<'a> {
    pub bot: &'a Bot,
    pub localization: &'a std::sync::Arc<LocalizationManager>,
    pub language_code: Option<&'a str>,
}

// Re-export main handler functions for use in main.rs
pub use callbacks::callback_handler::{callback_handler, callback_handler_with_cache};
pub use message_handler::{message_handler, message_handler_with_cache};

// Re-export utility functions that might be used elsewhere
pub use crate::validation::parse_ingredient_from_text;
pub use dialogue_manager::save_ingredients_to_database;
pub use image_processing::{
    download_and_process_image, download_file, process_ingredients_and_extract_matches,
};
pub use ui_builder::{
    create_ingredient_review_keyboard, create_post_confirmation_keyboard,
    create_processing_keyboard, create_recipes_pagination_keyboard, format_ingredients_list,
};
pub use ui_components::create_ingredient_editing_keyboard;
