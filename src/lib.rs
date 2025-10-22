//! # JustIngredients Telegram Bot
//!
//! A Telegram bot that extracts text from images using OCR and stores
//! ingredient measurements in a database with full-text search capabilities.

pub mod bot;
pub mod cache;
pub mod circuit_breaker;
pub mod db;
pub mod dialogue;
pub mod errors;
pub mod ingredient_editing;
pub mod instance_manager;
pub mod localization;
pub mod measurement_patterns;
pub mod observability;
pub mod observability_config;
pub mod ocr;
pub mod ocr_config;
pub mod ocr_errors;
pub mod text_processing;
pub mod validation;

// Re-export types for easier access
pub use text_processing::{MeasurementConfig, MeasurementDetector, MeasurementMatch};
