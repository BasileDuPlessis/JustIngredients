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

        let english_message =
            manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        let french_message =
            manager.get_message_in_language("multiple-recipes-found", "fr", Some(&args));

        assert!(!english_message.is_empty());
        assert!(!french_message.is_empty());
        assert!(english_message.contains("3"));
        assert!(french_message.contains("3"));
        assert_ne!(english_message, french_message);

        // Test recipe created message with date
        let mut date_args = HashMap::new();
        date_args.insert("date", "2024-01-15");

        let english_created =
            manager.get_message_in_language("recipe-created", "en", Some(&date_args));
        let french_created =
            manager.get_message_in_language("recipe-created", "fr", Some(&date_args));

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

            assert!(
                !english.is_empty(),
                "English message for '{}' should not be empty",
                message_key
            );
            assert!(
                !french.is_empty(),
                "French message for '{}' should not be empty",
                message_key
            );
            assert_ne!(
                english, french,
                "English and French messages for '{}' should be different",
                message_key
            );
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

            assert!(
                !english.is_empty(),
                "English message for '{}' should not be empty",
                message_key
            );
            assert!(
                !french.is_empty(),
                "French message for '{}' should not be empty",
                message_key
            );
        }
    }

    #[test]
    fn test_pluralization_and_formatting() {
        let manager = setup_localization();

        // Test that messages with variables are properly formatted
        let mut args = HashMap::new();
        args.insert("count", "1");

        let singular_message =
            manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        assert!(singular_message.contains("1"));
        assert!(singular_message.contains("recipes"));

        args.insert("count", "5");
        let plural_message =
            manager.get_message_in_language("multiple-recipes-found", "en", Some(&args));
        assert!(plural_message.contains("5"));
        assert!(plural_message.contains("recipes"));

        // Test French pluralization
        let french_singular = manager.get_message_in_language(
            "multiple-recipes-found",
            "fr",
            Some(&HashMap::from([("count", "1")])),
        );
        let french_plural = manager.get_message_in_language(
            "multiple-recipes-found",
            "fr",
            Some(&HashMap::from([("count", "5")])),
        );

        assert!(french_singular.contains("1"));
        assert!(french_plural.contains("5"));
    }
}

/// User messages read as plain sentences: no technical "[CODE]" prefix, no literal Markdown
/// "**" (messages are sent without parse mode, so it would show as is) and no broken character
/// (U+FFFD, left by a bad encoding).
#[test]
fn user_messages_have_no_technical_prefix_or_markdown() {
    for (language, source) in [
        ("en", include_str!("../locales/en/main.ftl")),
        ("fr", include_str!("../locales/fr/main.ftl")),
    ] {
        for line in source.lines() {
            if let Some((key, value)) = line.split_once(" = ") {
                assert!(
                    !value.trim_start().starts_with('['),
                    "{language}: {key} starts with a technical prefix: {value}"
                );
            }
            assert!(!line.contains("**"), "{language}: literal ** in: {line}");
            assert!(
                !line.contains('\u{FFFD}'),
                "{language}: broken character in: {line}"
            );
        }
    }
}

/// Every message key the code asks for exists in both locales (IMP-045): a missing key shows
/// "Missing translation: <key>" to the user.
#[test]
fn every_key_used_in_code_exists_in_both_locales() {
    use std::collections::BTreeSet;
    use std::path::Path;

    fn rust_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                rust_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    // The key is the literal right after the localization manager argument
    let calls = [
        r#"t_(?:args_)?lang\(\s*[^,()"]+,\s*"([a-z0-9-]+)""#,
        r#"create_localized_button\(\s*[^,()"]+,\s*"([a-z0-9-]+)""#,
        r#"create_localized_button_with_emoji\(\s*[^,()"]+,\s*"[^"]*",\s*"([a-z0-9-]+)""#,
    ]
    .map(|pattern| regex::Regex::new(pattern).expect("valid pattern"));

    let mut files = Vec::new();
    rust_files(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut files,
    );
    let mut used = BTreeSet::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("read source file");
        for call in &calls {
            used.extend(call.captures_iter(&source).map(|c| c[1].to_string()));
        }
    }
    assert!(
        used.len() > 50,
        "the key search found too few keys: {used:?}"
    );

    let defined = regex::Regex::new(r"(?m)^([a-z][a-z0-9-]*)\s*=").expect("valid pattern");
    for (language, source) in [
        ("en", include_str!("../locales/en/main.ftl")),
        ("fr", include_str!("../locales/fr/main.ftl")),
    ] {
        let keys: BTreeSet<&str> = defined
            .captures_iter(source)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        let missing: Vec<&String> = used.iter().filter(|k| !keys.contains(k.as_str())).collect();
        assert!(
            missing.is_empty(),
            "{language}: keys used in code but missing: {missing:?}"
        );
    }
}
