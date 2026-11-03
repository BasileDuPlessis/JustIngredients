//! # JustIngredients Telegram Bot
//!
//! A Telegram bot that reads the ingredient list of a recipe photo (Gemini vision API), lets the
//! user check it, and stores the recipe and its ingredients in Postgres.

// No panic outside tests (IMP-018): a panic stops the handling of an update without a reply.
// No wrapping cast (IMP-038): a Telegram id goes through `bot::telegram_id`.
// Errors go up with `?`; the few constant patterns that cannot fail say so with an `allow`.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::cast_possible_wrap
    )
)]

pub mod bot;
pub mod config;
pub mod conversion;
pub mod db;
pub mod db_connection;
pub mod deduplication;
pub mod dialogue;
pub mod dialogue_storage;
pub mod errors;
pub mod extraction;
pub mod ingredient_editing;
pub mod localization;
pub mod observability;
pub mod recipe_parts;
pub mod retry;
pub mod search;
pub mod secret;
pub mod telegram_connection;
pub mod text_processing;
pub mod unit_view;
pub mod units;
pub mod validation;

// Re-export types for easier access
pub use config::AppConfig;
pub use deduplication::{RequestDeduplicator, RequestId, SharedDeduplicator};
pub use text_processing::MeasurementMatch;
