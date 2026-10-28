use anyhow::Result;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::FluentResource;
use std::collections::HashMap;
use std::sync::Arc;
use unic_langid::LanguageIdentifier;

/// Locales embedded in the binary, parsed once when the manager is created.
const LOCALES: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/main.ftl")),
    ("fr", include_str!("../locales/fr/main.ftl")),
];

/// Localization manager for the Ingredients Bot: one parsed bundle per locale, shared by all
/// handlers.
pub struct LocalizationManager {
    bundles: HashMap<&'static str, FluentBundle<FluentResource>>,
}

impl std::fmt::Debug for LocalizationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut locales: Vec<&str> = self.bundles.keys().copied().collect();
        locales.sort_unstable();
        f.debug_struct("LocalizationManager")
            .field("locales", &locales)
            .finish()
    }
}

impl LocalizationManager {
    /// Create a new localization manager with embedded resources
    pub fn new() -> Result<Self> {
        let bundles = LOCALES
            .iter()
            .map(|(locale, content)| Ok((*locale, Self::create_bundle(locale, content)?)))
            .collect::<Result<_>>()?;
        Ok(Self { bundles })
    }

    /// Parses one locale's resource into a bundle
    fn create_bundle(locale_str: &str, content: &str) -> Result<FluentBundle<FluentResource>> {
        let locale: LanguageIdentifier = locale_str.parse()?;
        let mut bundle = FluentBundle::new_concurrent(vec![locale]);
        // Telegram shows the U+2068/U+2069 marks Fluent puts around variables as stray characters
        bundle.set_use_isolating(false);

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

    /// Get a localized message in a specific language with graceful fallback
    pub fn get_message_in_language(
        &self,
        key: &str,
        language: &str,
        args: Option<&HashMap<&str, &str>>,
    ) -> String {
        let fluent_args = args.map(|args_map| {
            fluent_bundle::FluentArgs::from_iter(
                args_map
                    .iter()
                    .map(|(k, v)| (*k, fluent_bundle::FluentValue::from(*v))),
            )
        });

        // Try requested language first, then fallback to English
        for lang in [language, "en"] {
            let Some(bundle) = self.bundles.get(lang) else {
                continue;
            };
            let Some(pattern) = bundle.get_message(key).and_then(|msg| msg.value()) else {
                continue;
            };
            let mut errors = vec![];
            let value = bundle.format_pattern(pattern, fluent_args.as_ref(), &mut errors);
            if !errors.is_empty() {
                // e.g. a missing argument: the message is still sent, with the placeholder
                tracing::warn!(key, language = lang, errors = ?errors, "Localized message formatted with errors");
            }
            return value.into_owned();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Message ids of an `.ftl` file and the variables each one uses.
    fn messages(content: &str) -> BTreeMap<String, Vec<String>> {
        let id = regex::Regex::new(r"^([a-z][a-z0-9-]*) *=").unwrap_or_else(|e| panic!("{e}"));
        let variable = regex::Regex::new(r"\{ *\$([a-z_]+) *\}").unwrap_or_else(|e| panic!("{e}"));
        let mut messages: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut current = None;
        for line in content.lines() {
            if let Some(captures) = id.captures(line) {
                current = Some(captures[1].to_string());
            } else if !line.starts_with([' ', '\t']) {
                current = None;
            }
            if let Some(current) = &current {
                let variables = messages.entry(current.clone()).or_default();
                variables.extend(variable.captures_iter(line).map(|c| c[1].to_string()));
            }
        }
        messages
    }

    #[test]
    fn both_locales_have_the_same_keys() {
        let en: Vec<String> = messages(LOCALES[0].1).into_keys().collect();
        let fr: Vec<String> = messages(LOCALES[1].1).into_keys().collect();
        assert!(en.len() > 100, "the parser found the messages");
        assert_eq!(en, fr);
    }

    #[test]
    fn every_message_formats_without_errors() {
        let manager = LocalizationManager::new().unwrap_or_else(|e| panic!("{e}"));
        for (locale, content) in LOCALES {
            let bundle = &manager.bundles[locale];
            for (key, variables) in messages(content) {
                let mut args = fluent_bundle::FluentArgs::new();
                for variable in &variables {
                    args.set(variable.as_str(), "x");
                }
                let pattern = bundle
                    .get_message(&key)
                    .and_then(|message| message.value())
                    .unwrap_or_else(|| panic!("{locale} {key} has no value"));
                let mut errors = vec![];
                let text = bundle.format_pattern(pattern, Some(&args), &mut errors);
                assert!(errors.is_empty(), "{locale} {key}: {errors:?}");
                assert!(
                    !text.contains('\u{2068}') && !text.contains('\u{2069}'),
                    "{locale} {key} has isolation marks"
                );
            }
        }
    }
}
