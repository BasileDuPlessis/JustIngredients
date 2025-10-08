use anyhow::Result;
use fluent_bundle::{FluentBundle, FluentResource};
use std::collections::HashMap;
use std::sync::Arc;
use unic_langid::LanguageIdentifier;

/// Localization manager for the Ingredients Bot
#[derive(Debug)]
pub struct LocalizationManager {
    // No shared state - bundles are created on demand
}

impl LocalizationManager {
    /// Create a new localization manager with embedded resources
    pub fn new() -> Result<Self> {
        // No initialization needed - bundles are created on demand
        Ok(Self {})
    }

    /// Create a fluent bundle for a specific locale using embedded resources
    fn create_bundle(
        locale_str: &str,
        locale: &LanguageIdentifier,
    ) -> Result<FluentBundle<FluentResource>> {
        let mut bundle = FluentBundle::new(vec![locale.clone()]);

        // Load embedded resource based on locale
        let content = match locale_str {
            "en" => include_str!("../locales/en/main.ftl"),
            "fr" => include_str!("../locales/fr/main.ftl"),
            _ => return Err(anyhow::anyhow!("Unsupported locale: {}", locale_str)),
        };

        let resource = FluentResource::try_new(content.to_string()).map_err(|(_, errors)| {
            anyhow::anyhow!(
                "Failed to parse localization resource for {}: {:?}",
                locale_str,
                errors
            )
        })?;

        bundle
            .add_resource(resource)
            .map_err(|e| anyhow::anyhow!("Failed to add resource for {}: {:?}", locale_str, e))?;

        Ok(bundle)
    }

    /// Create a bundle for a specific language
    fn create_bundle_for_language(language: &str) -> Result<FluentBundle<FluentResource>> {
        let locale: LanguageIdentifier = language.parse()?;
        Self::create_bundle(language, &locale)
    }

    /// Get a localized message in a specific language with graceful fallback
    pub fn get_message_in_language(
        &self,
        key: &str,
        language: &str,
        args: Option<&HashMap<&str, &str>>,
    ) -> String {
        // Try requested language first, then fallback to English
        let languages_to_try = vec![language, "en"];

        for lang in languages_to_try {
            if let Ok(bundle) = Self::create_bundle_for_language(lang) {
                if let Some(msg) = bundle.get_message(key) {
                    if let Some(pattern) = msg.value() {
                        let mut value = String::new();

                        let fluent_args = args.map(|args_map| {
                            fluent_bundle::FluentArgs::from_iter(
                                args_map
                                    .iter()
                                    .map(|(k, v)| (*k, fluent_bundle::FluentValue::from(*v))),
                            )
                        });

                        if bundle
                            .write_pattern(&mut value, pattern, fluent_args.as_ref(), &mut vec![])
                            .is_ok()
                        {
                            return value;
                        }
                    }
                }
            }
        }

        // Fallback: return a user-friendly message
        format!("Missing translation: {}", key)
    }

    /// Get a localized message with arguments in a specific language
    pub fn get_message_with_args_in_language(
        &self,
        key: &str,
        language: &str,
        args: &[(&str, &str)],
    ) -> String {
        let args_map: HashMap<&str, &str> = args.iter().cloned().collect();
        self.get_message_in_language(key, language, Some(&args_map))
    }

    /// Check if a language is supported
    pub fn is_language_supported(&self, language: &str) -> bool {
        matches!(language, "en" | "fr")
    }
}

/// Create a new shared localization manager
/// This should be called once at application startup
pub fn create_localization_manager() -> Result<Arc<LocalizationManager>> {
    Ok(Arc::new(LocalizationManager::new()?))
}

/// Convenience function to get a localized message in user's language
pub fn t_lang(
    manager: &Arc<LocalizationManager>,
    key: &str,
    language_code: Option<&str>,
) -> String {
    let language = detect_language(manager, language_code);
    manager.get_message_in_language(key, &language, None)
}

/// Convenience function to get a localized message with arguments in user's language
pub fn t_args_lang(
    manager: &Arc<LocalizationManager>,
    key: &str,
    args: &[(&str, &str)],
    language_code: Option<&str>,
) -> String {
    let language = detect_language(manager, language_code);
    manager.get_message_with_args_in_language(key, &language, args)
}

/// Detect the appropriate language based on user's Telegram language code
pub fn detect_language(manager: &Arc<LocalizationManager>, language_code: Option<&str>) -> String {
    if let Some(code) = language_code {
        // Extract language code (e.g., "fr-FR" -> "fr", "en-US" -> "en")
        let lang = if code.contains('-') {
            code.split('-').next().unwrap_or("en")
        } else {
            code
        };

        // Check if we support this language using the shared manager
        if manager.is_language_supported(lang) {
            return lang.to_string();
        }
    }

    // Default to English if language not supported or not provided
    "en".to_string()
}
