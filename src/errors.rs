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
    /// A typed recipe name, recipe note or ingredient line refused, with its reason
    Validation(ValidationError),
    /// Database operation errors
    Database(String),
    /// Ingredient parser errors (measurement detector setup)
    Parser(String),
    /// File system errors
    FileSystem(String),
    /// Network/communication errors
    Network(String),
    /// Internal application errors
    Internal(String),
}

/// Why a typed recipe name, recipe note or ingredient line is refused (backlog IMP-017). Each reason has the
/// message the user gets (`l10n_key`); callers match the reason, never a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("the recipe name is empty")]
    EmptyName,
    #[error("the recipe name is too long")]
    NameTooLong,
    #[error("the ingredient text is empty")]
    EmptyLine,
    #[error("the ingredient line is too long")]
    LineTooLong,
    #[error("the ingredient name is too long")]
    IngredientNameTooLong,
    #[error("the line has no ingredient name")]
    NoIngredientName,
    #[error("the quantity is not readable")]
    InvalidQuantity,
    #[error("the answer is not a quantity")]
    InvalidQuantityAnswer,
    #[error("several lines were typed")]
    SeveralLines,
    #[error("the recipe note is empty")]
    EmptyNote,
    #[error("the recipe note is too long")]
    NoteTooLong,
}

impl ValidationError {
    /// Every reason, for the check that each message exists in both locales.
    pub const ALL: [ValidationError; 11] = [
        Self::EmptyName,
        Self::NameTooLong,
        Self::EmptyLine,
        Self::LineTooLong,
        Self::IngredientNameTooLong,
        Self::NoIngredientName,
        Self::InvalidQuantity,
        Self::InvalidQuantityAnswer,
        Self::SeveralLines,
        Self::EmptyNote,
        Self::NoteTooLong,
    ];

    /// The locale key of the message the user gets.
    pub fn l10n_key(self) -> &'static str {
        match self {
            Self::EmptyName => "recipe-name-invalid",
            Self::NameTooLong => "recipe-name-too-long",
            Self::EmptyLine => "edit-empty",
            Self::LineTooLong => "edit-too-long",
            Self::IngredientNameTooLong => "edit-ingredient-name-too-long",
            Self::NoIngredientName => "edit-no-ingredient-name",
            Self::InvalidQuantity => "edit-invalid-quantity",
            Self::InvalidQuantityAnswer => "edit-invalid-quantity-answer",
            Self::SeveralLines => "edit-one-line",
            Self::EmptyNote => "note-empty",
            Self::NoteTooLong => "note-too-long",
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(msg) => write!(f, "[CONFIG] {}", msg),
            AppError::Validation(reason) => write!(f, "[VALIDATION] {reason}"),
            AppError::Database(msg) => write!(f, "[DATABASE] {}", msg),
            AppError::Parser(msg) => write!(f, "[PARSER] {}", msg),
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

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;

/// Standardized error logging utilities for consistent error reporting across the application
pub mod error_logging {
    use tracing::error;

    /// Log database operation errors with contextual information
    pub fn log_database_error(
        error: &impl std::fmt::Display,
        operation: &str,
        user_id: Option<i64>,
        additional_context: Option<&[(&str, &dyn std::fmt::Display)]>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            user_id = ?user_id,
            additional_context = ?additional_context.map(|ctx| ctx.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", ")),
            "Database operation failed"
        );
    }

    /// Log network/communication errors with connection context
    pub fn log_network_error(
        error: &impl std::fmt::Display,
        operation: &str,
        endpoint: Option<&str>,
        attempt_count: Option<u32>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            endpoint = ?endpoint,
            attempt_count = ?attempt_count,
            "Network operation failed"
        );
    }

    /// Log file system errors with path and operation context
    pub fn log_filesystem_error(
        error: &impl std::fmt::Display,
        operation: &str,
        path: Option<&str>,
        file_size: Option<u64>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            path = ?path,
            file_size_bytes = ?file_size,
            "File system operation failed"
        );
    }

    /// Log internal application errors with component context
    pub fn log_internal_error(
        error: &impl std::fmt::Display,
        component: &str,
        operation: &str,
        user_id: Option<i64>,
    ) {
        error!(
            error = %error,
            component = %component,
            operation = %operation,
            user_id = ?user_id,
            "Internal application error"
        );
    }

    /// Log configuration errors during startup/initialization
    pub fn log_config_error(error: &impl std::fmt::Display, config_key: &str, operation: &str) {
        error!(
            error = %error,
            config_key = %config_key,
            operation = %operation,
            "Configuration error"
        );
    }
}
