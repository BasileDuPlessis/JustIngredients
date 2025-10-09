//! # Localization Tests
//!
//! This module contains unit tests for the localization functionality,
//! testing message retrieval and formatting with various edge cases.

use just_ingredients::localization::{
    create_localization_manager, detect_language, t_args_lang, t_lang, LocalizationManager,
};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_localization() -> Arc<LocalizationManager> {
        // Create a new shared localization manager for each test
        create_localization_manager().expect("Failed to create localization manager")
    }

    #[test]
    fn test_get_message_existing_key() {
        let manager = setup_localization();

        let message = manager.get_message_in_language("help-commands", "en", None);
        assert!(!message.is_empty());
        assert!(message.contains("Commands"));
    }

    #[test]
    fn test_get_message_nonexistent_key() {
        let manager = setup_localization();

        let message = manager.get_message_in_language("nonexistent-key", "en", None);
        assert!(message.starts_with("Missing translation:"));
    }

    #[test]
    fn test_get_message_unsupported_language() {
        let manager = setup_localization();

        let message = manager.get_message_in_language("help-commands", "unsupported", None);
        // Should fall back to English
        assert!(!message.is_empty());
        assert!(message.contains("Commands"));
    }

    #[test]
    fn test_get_message_with_args() {
        let manager = setup_localization();

        let mut args = HashMap::new();
        args.insert("recipe_name", "Test Recipe");
        args.insert("ingredient_count", "5");

        let message = manager.get_message_in_language("recipe-complete", "en", Some(&args));
        assert!(!message.is_empty());
        assert!(message.contains("Test Recipe"));
        assert!(message.contains("5"));
    }

    #[test]
    fn test_get_message_missing_args() {
        let manager = setup_localization();

        // Test with missing required args - should handle gracefully
        let message = manager.get_message_in_language("recipe-complete", "en", None);
        // Either returns the message with placeholder or handles error
        assert!(!message.is_empty());
    }

    #[test]
    fn test_french_localization() {
        let manager = setup_localization();

        let message = manager.get_message_in_language("help-commands", "fr", None);
        assert!(!message.is_empty());
        // French message should be different from English
        let english_message = manager.get_message_in_language("help-commands", "en", None);
        assert_ne!(message, english_message);
    }

    #[test]
    fn test_language_detection() {
        let manager = setup_localization();

        assert_eq!(detect_language(&manager, Some("en")), "en");
        assert_eq!(detect_language(&manager, Some("en-US")), "en");
        assert_eq!(detect_language(&manager, Some("fr")), "fr");
        assert_eq!(detect_language(&manager, Some("fr-CA")), "fr");
        assert_eq!(detect_language(&manager, None), "en"); // Default to English
        assert_eq!(detect_language(&manager, Some("unsupported")), "en"); // Fallback to English
    }

    #[test]
    fn test_convenience_functions() {
        let manager = setup_localization();

        // Test t_lang function
        let message = t_lang(&manager, "help-commands", Some("en"));
        assert!(!message.is_empty());

        // Test t_args_lang function
        let args = vec![("recipe_name", "Test Recipe"), ("ingredient_count", "3")];
        let message_with_args = t_args_lang(&manager, "recipe-complete", &args, Some("en"));
        assert!(!message_with_args.is_empty());
        assert!(message_with_args.contains("Test Recipe"));
        assert!(message_with_args.contains("3"));
    }
}
