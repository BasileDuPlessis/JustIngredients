//! # Application Error Types
//!
//! This module defines common error types used throughout the JustIngredients application.
//! It provides structured error handling for various application components.

use std::fmt;

/// General application error type for consistent error handling
#[derive(Debug, Clone, PartialEq)]
pub enum AppError {
    /// Configuration validation errors
    Config(String),
    /// Validation errors (recipe names, inputs, etc.)
    Validation(String),
    /// Database operation errors
    Database(String),
    /// OCR processing errors
    Ocr(String),
    /// File system errors
    FileSystem(String),
    /// Network/communication errors
    Network(String),
    /// Internal application errors
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(msg) => write!(f, "[CONFIG] {}", msg),
            AppError::Validation(msg) => write!(f, "[VALIDATION] {}", msg),
            AppError::Database(msg) => write!(f, "[DATABASE] {}", msg),
            AppError::Ocr(msg) => write!(f, "[OCR] {}", msg),
            AppError::FileSystem(msg) => write!(f, "[FILESYSTEM] {}", msg),
            AppError::Network(msg) => write!(f, "[NETWORK] {}", msg),
            AppError::Internal(msg) => write!(f, "[INTERNAL] {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<crate::ocr_errors::OcrError> for AppError {
    fn from(err: crate::ocr_errors::OcrError) -> Self {
        AppError::Ocr(err.to_string())
    }
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;