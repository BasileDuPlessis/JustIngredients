//! Common UI Components for Telegram Bot
//!
//! This module provides reusable UI components and patterns to reduce code duplication
//! across the bot's UI building functions.

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use std::sync::Arc;

/// Create a localized inline keyboard button
pub fn create_localized_button(
    localization: &Arc<crate::localization::LocalizationManager>,
    text_key: &str,
    callback_data: String,
    language_code: Option<&str>,
) -> InlineKeyboardButton {
    InlineKeyboardButton::callback(
        crate::localization::t_lang(localization, text_key, language_code),
        callback_data,
    )
}

/// Create a localized inline keyboard button with an emoji prefix
pub fn create_localized_button_with_emoji(
    localization: &Arc<crate::localization::LocalizationManager>,
    emoji: &str,
    text_key: &str,
    callback_data: String,
    language_code: Option<&str>,
) -> InlineKeyboardButton {
    InlineKeyboardButton::callback(
        format!(
            "{} {}",
            emoji,
            crate::localization::t_lang(localization, text_key, language_code)
        ),
        callback_data,
    )
}

/// Truncate text to a maximum length, adding ellipsis if truncated
pub fn truncate_text(text: &str, max_length: usize) -> String {
    if text.len() <= max_length {
        text.to_string()
    } else {
        format!("{}...", &text[..max_length.saturating_sub(3)])
    }
}

/// Create a button with truncated text and emoji prefix
pub fn create_truncated_button(
    emoji: &str,
    text: &str,
    callback_data: String,
    max_length: usize,
) -> InlineKeyboardButton {
    let truncated_text = truncate_text(text, max_length);
    InlineKeyboardButton::callback(format!("{} {}", emoji, truncated_text), callback_data)
}

/// Create a confirmation dialog with Yes/No buttons
pub fn create_confirmation_dialog(
    localization: &Arc<crate::localization::LocalizationManager>,
    _title_key: &str,
    confirm_callback: String,
    cancel_callback: String,
    language_code: Option<&str>,
) -> InlineKeyboardMarkup {
    let buttons = vec![
        vec![
            create_localized_button_with_emoji(
                localization,
                "✅",
                "confirm",
                confirm_callback,
                language_code,
            ),
            create_localized_button_with_emoji(
                localization,
                "❌",
                "cancel",
                cancel_callback,
                language_code,
            ),
        ],
        vec![create_localized_button_with_emoji(
            localization,
            "⬅️",
            "back",
            "back".to_string(),
            language_code,
        )],
    ];

    InlineKeyboardMarkup::new(buttons)
}

/// Create pagination buttons for a list
pub fn create_pagination_buttons(
    localization: &Arc<crate::localization::LocalizationManager>,
    current_page: usize,
    total_pages: usize,
    language_code: Option<&str>,
) -> Vec<InlineKeyboardButton> {
    let mut buttons = Vec::new();

    // Previous button
    if current_page > 0 {
        buttons.push(create_localized_button_with_emoji(
            localization,
            "⬅️",
            "previous",
            format!("page:{}", current_page - 1),
            language_code,
        ));
    }

    // Page info (disabled button for display)
    let page_info = format!(
        "{} {} {} {}",
        crate::localization::t_lang(localization, "page", language_code),
        current_page + 1,
        crate::localization::t_lang(localization, "of", language_code),
        total_pages
    );
    buttons.push(InlineKeyboardButton::callback(page_info, "noop".to_string()));

    // Next button
    if current_page + 1 < total_pages {
        buttons.push(create_localized_button_with_emoji(
            localization,
            "➡️",
            "next",
            format!("page:{}", current_page + 1),
            language_code,
        ));
    }

    buttons
}

/// Create a back button
pub fn create_back_button(
    localization: &Arc<crate::localization::LocalizationManager>,
    callback_data: String,
    language_code: Option<&str>,
) -> InlineKeyboardButton {
    create_localized_button_with_emoji(localization, "⬅️", "back", callback_data, language_code)
}

/// Create a cancel button
pub fn create_cancel_button(
    localization: &Arc<crate::localization::LocalizationManager>,
    callback_data: String,
    language_code: Option<&str>,
) -> InlineKeyboardButton {
    create_localized_button_with_emoji(localization, "❌", "cancel", callback_data, language_code)
}

/// Create an add button
pub fn create_add_button(
    localization: &Arc<crate::localization::LocalizationManager>,
    text_key: &str,
    callback_data: String,
    language_code: Option<&str>,
) -> InlineKeyboardButton {
    create_localized_button_with_emoji(localization, "➕", text_key, callback_data, language_code)
}

/// Wrapper function that records UI metrics around an operation
pub async fn with_ui_metrics<F, Fut, T>(
    operation_name: &str,
    input_count: usize,
    operation: F,
) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start_time = std::time::Instant::now();
    let result = operation().await;
    let duration = start_time.elapsed();

    // For now, we'll assume the output count is 1 (the result)
    // In the future, this could be made more sophisticated
    let output_count = 1;

    crate::observability::record_ui_metrics(
        operation_name,
        duration,
        input_count,
        output_count,
    );

    result
}

/// Synchronous version of with_ui_metrics for non-async operations
pub fn with_ui_metrics_sync<F, T>(
    operation_name: &str,
    input_count: usize,
    operation: F,
) -> T
where
    F: FnOnce() -> T,
{
    let start_time = std::time::Instant::now();
    let result = operation();
    let duration = start_time.elapsed();

    // For now, we'll assume the output count is 1 (the result)
    let output_count = 1;

    crate::observability::record_ui_metrics(
        operation_name,
        duration,
        input_count,
        output_count,
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::LocalizationManager;

    #[tokio::test]
    async fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a very long text", 10), "this is...");
        assert_eq!(truncate_text("exactly", 7), "exactly");
        assert_eq!(truncate_text("toolong", 5), "to...");
    }

    #[tokio::test]
    async fn test_create_localized_button() {
        let localization = Arc::new(LocalizationManager::new().unwrap());

        let button = create_localized_button(
            &localization,
            "confirm",
            "test_callback".to_string(),
            Some("en"),
        );

        assert_eq!(button.text, "Confirm");
        if let teloxide::types::InlineKeyboardButtonKind::CallbackData(data) = &button.kind {
            assert_eq!(data, "test_callback");
        } else {
            panic!("Expected callback button");
        }
    }

    #[tokio::test]
    async fn test_create_confirmation_dialog() {
        let localization = Arc::new(LocalizationManager::new().unwrap());

        let keyboard = create_confirmation_dialog(
            &localization,
            "confirm-delete",
            "confirm_delete".to_string(),
            "cancel_delete".to_string(),
            Some("en"),
        );

        let buttons = keyboard.inline_keyboard;
        assert_eq!(buttons.len(), 2); // Two rows
        assert_eq!(buttons[0].len(), 2); // Two buttons in first row (confirm/cancel)
        assert_eq!(buttons[1].len(), 1); // One button in second row (back)
    }
}