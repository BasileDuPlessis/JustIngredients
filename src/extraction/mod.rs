//! # Ingredient Extraction
//!
//! Turns a recipe image into structured ingredients. The Gemini vision API replaces the
//! Tesseract OCR pipeline (see `specs/tasks/llm-extraction-migration.md`).

pub mod config;

pub use config::{ExtractionBackend, ExtractionConfig, GeminiConfig, ThinkingLevel};
