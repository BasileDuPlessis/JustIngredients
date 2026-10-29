//! # JustIngredients Telegram Bot
//!
//! A Telegram bot that reads the ingredient list of a recipe photo (Gemini vision API), lets the
//! user check it, and stores the recipe and its ingredients in Postgres.

pub mod bot;
pub mod cache;
pub mod config;
pub mod db;
pub mod db_connection;
pub mod deduplication;
pub mod dialogue;
pub mod errors;
pub mod extraction;
pub mod ingredient_editing;
pub mod localization;
pub mod observability;
pub mod observability_config;
pub mod retry;
pub mod search;
pub mod secret;
pub mod telegram_connection;
pub mod text_processing;
pub mod units;
pub mod validation;

// Re-export types for easier access
pub use config::AppConfig;
pub use deduplication::{RequestDeduplicator, RequestId, SharedDeduplicator};
pub use text_processing::MeasurementMatch;
