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

    /// Log recipe processing errors with recipe-specific context
    pub fn log_recipe_error(
        error: &impl std::fmt::Display,
        operation: &str,
        user_id: i64,
        recipe_name: Option<&str>,
        ingredient_count: Option<usize>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            user_id = %user_id,
            recipe_name = ?recipe_name,
            ingredient_count = ?ingredient_count,
            "Recipe processing failed"
        );
    }

    /// Log OCR processing errors with image and processing context
    pub fn log_ocr_error(
        error: &impl std::fmt::Display,
        operation: &str,
        user_id: Option<i64>,
        image_size: Option<u64>,
        processing_duration: Option<std::time::Duration>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            user_id = ?user_id,
            image_size_bytes = ?image_size,
            processing_duration_ms = ?processing_duration.map(|d| d.as_millis()),
            "OCR processing failed"
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

    /// Log validation errors with input context
    pub fn log_validation_error(
        error: &impl std::fmt::Display,
        operation: &str,
        user_id: Option<i64>,
        input_type: &str,
        input_value: Option<&str>,
    ) {
        error!(
            error = %error,
            operation = %operation,
            user_id = ?user_id,
            input_type = %input_type,
            input_value = ?input_value.map(|v| if v.len() > 100 { format!("{}...", &v[..100]) } else { v.to_string() }),
            "Validation failed"
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
    pub fn log_config_error(
        error: &impl std::fmt::Display,
        config_key: &str,
        operation: &str,
    ) {
        error!(
            error = %error,
            config_key = %config_key,
            operation = %operation,
            "Configuration error"
        );
    }
}
