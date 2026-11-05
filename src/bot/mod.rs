//! Bot module for handling Telegram interactions
//!
//! This module is split into several submodules for better organization:
//! - `callbacks`: All callback query handling (organized into submodules)
//! - `cleanup`: Messages of an action, deleted when it ends
//! - `commands`: Command parsing and the Telegram command menu
//! - `home`: Home message sent at the end of every action
//! - `html`: Telegram HTML messages, escaped by construction
//! - `message_handler`: Handles incoming text, photo, and document messages
//! - `ui_builder`: Creates keyboards and formats messages
//! - `dialogue_manager`: Manages dialogue state transitions and validation

mod action_input;
pub mod api_keys;
pub mod callbacks;
pub mod cards;
pub mod cleanup;
pub mod command_handlers;
pub mod commands;
pub mod dialogue_manager;
pub mod dispatch;
pub mod home;
pub mod html;
pub mod image_processing;
pub mod media_handlers;
pub mod message_handler;
pub mod note;
pub mod recipe_list;
pub mod ui_builder;
pub mod ui_components;
pub mod unit_choice;

// Common context structures for handler functions
use crate::localization::LocalizationManager;
use teloxide::Bot;

/// A Telegram user id as the database stores it (`BIGINT`, `telegram_id` columns). Telegram's
/// ids have at most 52 significant bits, so they always fit; one that did not would become
/// `i64::MAX`, an id no account has, instead of wrapping to another user's (IMP-038).
pub fn telegram_id(id: teloxide::types::UserId) -> i64 {
    i64::try_from(id.0).unwrap_or(i64::MAX)
}

/// Common context for bot handlers containing shared dependencies
#[derive(Debug)]
pub struct HandlerContext<'a> {
    pub bot: &'a Bot,
    pub localization: &'a std::sync::Arc<LocalizationManager>,
    pub language_code: Option<&'a str>,
}

// Re-export main handler functions for use in main.rs
pub use callbacks::callback_handler::callback_handler_with_cache;
pub use message_handler::{MessageServices, message_handler_with_cache};

// Re-export utility functions that might be used elsewhere
pub use crate::validation::parse_ingredient_from_text;
pub use dialogue_manager::save_ingredients_to_database;
pub use image_processing::{download_and_process_image, download_file_bytes, extraction_error_key};
pub use ui_builder::{create_ingredient_review_keyboard, format_ingredients_list};
pub use ui_components::create_ingredient_editing_keyboard;

#[cfg(test)]
mod tests {
    use teloxide::types::UserId;

    #[test]
    fn telegram_ids_keep_their_value_and_never_wrap() {
        // The largest id Telegram can give: 52 significant bits
        let largest = (1_u64 << 52) - 1;
        assert_eq!(super::telegram_id(UserId(largest)), 4_503_599_627_370_495);
        assert_eq!(super::telegram_id(UserId(123_456_789)), 123_456_789);
        assert_eq!(
            super::telegram_id(UserId(u64::MAX)),
            i64::MAX,
            "no wrap to a negative id"
        );
    }
}
