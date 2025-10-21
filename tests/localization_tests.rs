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

    #[test]
    fn test_duplicate_recipe_messages() {
        let manager = setup_localization();

        // Test multiple recipes found message with count
        let mut args = HashMap::new();
        args.insert("count", "3");

        let english_message = manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        let french_message = manager.get_message_in_language("multiple-recipes-found", "fr", Some(&args));

        assert!(!english_message.is_empty());
        assert!(!french_message.is_empty());
        assert!(english_message.contains("3"));
        assert!(french_message.contains("3"));
        assert_ne!(english_message, french_message);

        // Test recipe created message with date
        let mut date_args = HashMap::new();
        date_args.insert("date", "2024-01-15");

        let english_created = manager.get_message_in_language("recipe-created", "en", Some(&date_args));
        let french_created = manager.get_message_in_language("recipe-created", "fr", Some(&date_args));

        assert!(!english_created.is_empty());
        assert!(!french_created.is_empty());
        assert!(english_created.contains("2024-01-15"));
        assert!(french_created.contains("2024-01-15"));
    }

    #[test]
    fn test_recipe_management_messages() {
        let manager = setup_localization();

        // Test recipe details and action messages
        let messages = vec![
            "recipe-details-title",
            "recipe-actions",
            "edit-recipe-name",
            "delete-recipe",
            "back-to-recipes",
            "delete-recipe-title",
            "delete-recipe-confirmation",
        ];

        for message_key in messages {
            let english = manager.get_message_in_language(message_key, "en", None);
            let french = manager.get_message_in_language(message_key, "fr", None);

            assert!(!english.is_empty(), "English message for '{}' should not be empty", message_key);
            assert!(!french.is_empty(), "French message for '{}' should not be empty", message_key);
            assert_ne!(english, french, "English and French messages for '{}' should be different", message_key);
        }
    }

    #[test]
    fn test_recipe_workflow_messages() {
        let manager = setup_localization();

        // Test workflow-related messages that were added
        let workflow_messages = vec![
            "confirm",
            "cancel",
            "recipe-deleted",
            "recipe-deleted-help",
            "error-deleting-recipe",
            "error-deleting-recipe-help",
            "recipe-not-found",
            "delete-cancelled",
        ];

        for message_key in workflow_messages {
            let english = manager.get_message_in_language(message_key, "en", None);
            let french = manager.get_message_in_language(message_key, "fr", None);

            assert!(!english.is_empty(), "English message for '{}' should not be empty", message_key);
            assert!(!french.is_empty(), "French message for '{}' should not be empty", message_key);
        }
    }

    #[test]
    fn test_pluralization_and_formatting() {
        let manager = setup_localization();

        // Test that messages with variables are properly formatted
        let mut args = HashMap::new();
        args.insert("count", "1");

        let singular_message = manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        assert!(singular_message.contains("1"));
        assert!(singular_message.contains("recipes"));

        args.insert("count", "5");
        let plural_message = manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        assert!(plural_message.contains("5"));
        assert!(plural_message.contains("recipes"));

        // Test French pluralization
        let french_singular = manager.get_message_in_language("multiple-recipes-found", "fr", Some(&HashMap::from([("count", "1")])));
        let french_plural = manager.get_message_in_language("multiple-recipes-found", "fr", Some(&HashMap::from([("count", "5")])));

        assert!(french_singular.contains("1"));
        assert!(french_plural.contains("5"));
    }
}
