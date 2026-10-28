//! Configuration of the ingredient extraction (Gemini), loaded from environment variables.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `GEMINI_API_KEY` | (none) | required |
//! | `GEMINI_MODEL` | `gemini-3.5-flash-lite` | model id |
//! | `GEMINI_TIMEOUT_SECS` | `40` | time budget for one image, retries included, 1–300 |
//! | `GEMINI_THINKING_LEVEL` | `minimal` | `minimal`, `low`, `medium` or `high` |
//! | `GEMINI_MAX_CONCURRENCY` | `2` | concurrent API calls, 1–32 (each holds up to ~25 MB) |
//! | `EXTRACTION_DAILY_LIMIT_PER_USER` | `50` | photos read per user and UTC day, 1–10000 |
//!
//! Values are parsed and checked at startup: the bot does not start without `GEMINI_API_KEY` or
//! with a malformed or out-of-range `GEMINI_*` value. `EXTRACTION_BACKEND` is no longer read: the
//! Tesseract fallback was removed (migration Phase 5).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::str::FromStr;

use crate::errors::{AppError, AppResult};
use crate::secret::SecretString;

/// Default Gemini model, chosen with `scripts/llm_benchmark` (2026-10-26).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";

/// Gemini thinking level (`generationConfig.thinkingConfig.thinkingLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

impl ThinkingLevel {
    /// Value sent to the API.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl FromStr for ThinkingLevel {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(AppError::Config(format!(
                "GEMINI_THINKING_LEVEL must be minimal, low, medium or high, got '{other}'"
            ))),
        }
    }
}

/// Settings of the Gemini API client.
#[derive(Debug, Clone, PartialEq)]
pub struct GeminiConfig {
    /// API key; redacted in `Debug`.
    pub api_key: SecretString,
    pub model: String,
    pub timeout_secs: u64,
    pub thinking_level: ThinkingLevel,
    pub max_concurrency: usize,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key: SecretString::default(),
            model: DEFAULT_GEMINI_MODEL.to_string(),
            timeout_secs: 40,
            thinking_level: ThinkingLevel::Minimal,
            max_concurrency: 2,
        }
    }
}

/// Extraction settings: the Gemini client settings and the per-user daily limit.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionConfig {
    pub gemini: GeminiConfig,
    /// Paid extractions per user and UTC day (about 0.0017 USD each).
    pub daily_limit_per_user: u32,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            gemini: GeminiConfig::default(),
            daily_limit_per_user: 50,
        }
    }
}

impl ExtractionConfig {
    /// Loads the configuration from environment variables.
    pub fn from_env() -> AppResult<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Loads the configuration from `lookup`, which returns the value of a variable if set.
    /// Tests pass a map instead of mutating the process environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> AppResult<Self> {
        let defaults = Self::default();
        let get = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());

        Ok(Self {
            gemini: GeminiConfig {
                // Pasted secrets often end with a newline, which would break the request header.
                api_key: get("GEMINI_API_KEY")
                    .map(|value| SecretString::new(value.trim()))
                    .unwrap_or_default(),
                model: get("GEMINI_MODEL")
                    .map(|value| value.trim().to_string())
                    .unwrap_or(defaults.gemini.model),
                timeout_secs: parse_number(get("GEMINI_TIMEOUT_SECS"), "GEMINI_TIMEOUT_SECS")?
                    .unwrap_or(defaults.gemini.timeout_secs),
                thinking_level: get("GEMINI_THINKING_LEVEL")
                    .map(|value| value.parse())
                    .transpose()?
                    .unwrap_or(defaults.gemini.thinking_level),
                max_concurrency: parse_number(
                    get("GEMINI_MAX_CONCURRENCY"),
                    "GEMINI_MAX_CONCURRENCY",
                )?
                .unwrap_or(defaults.gemini.max_concurrency),
            },
            daily_limit_per_user: parse_number(
                get("EXTRACTION_DAILY_LIMIT_PER_USER"),
                "EXTRACTION_DAILY_LIMIT_PER_USER",
            )?
            .unwrap_or(defaults.daily_limit_per_user),
        })
    }

    /// Checks the values.
    pub fn validate(&self) -> AppResult<()> {
        let gemini = &self.gemini;
        if gemini.api_key.is_empty() {
            return Err(AppError::Config("GEMINI_API_KEY is required".to_string()));
        }
        if !is_valid_model_id(&gemini.model) {
            return Err(AppError::Config(
                "GEMINI_MODEL must contain only letters, digits, '.', '-' and '_'".to_string(),
            ));
        }
        if !(1..=300).contains(&gemini.timeout_secs) {
            return Err(AppError::Config(
                "GEMINI_TIMEOUT_SECS must be between 1 and 300".to_string(),
            ));
        }
        if !(1..=32).contains(&gemini.max_concurrency) {
            return Err(AppError::Config(
                "GEMINI_MAX_CONCURRENCY must be between 1 and 32".to_string(),
            ));
        }
        if !(1..=10_000).contains(&self.daily_limit_per_user) {
            return Err(AppError::Config(
                "EXTRACTION_DAILY_LIMIT_PER_USER must be between 1 and 10000".to_string(),
            ));
        }
        Ok(())
    }
}

/// Model ids go into the request path, so only plain id characters are accepted.
pub fn is_valid_model_id(model: &str) -> bool {
    !model.is_empty()
        && !model.starts_with('.')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn parse_number<T: FromStr>(value: Option<String>, name: &str) -> AppResult<Option<T>> {
    value
        .map(|value| {
            value
                .trim()
                .parse()
                .map_err(|_| AppError::Config(format!("{name} must be a valid number")))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn load(vars: &[(&str, &str)]) -> AppResult<ExtractionConfig> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        ExtractionConfig::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn defaults_use_gemini_and_require_a_key() -> AppResult<()> {
        let config = load(&[])?;
        assert_eq!(config, ExtractionConfig::default());
        assert_eq!(config.gemini.model, DEFAULT_GEMINI_MODEL);
        assert_eq!(config.gemini.thinking_level, ThinkingLevel::Minimal);
        assert!(config.validate().is_err(), "no GEMINI_API_KEY");
        load(&[("GEMINI_API_KEY", "key")])?.validate()
    }

    #[test]
    fn a_leftover_backend_variable_is_ignored() -> AppResult<()> {
        let config = load(&[("EXTRACTION_BACKEND", "tesseract"), ("GEMINI_API_KEY", "k")])?;
        assert_eq!(config.gemini.api_key.expose(), "k");
        config.validate()
    }

    #[test]
    fn all_variables_are_read() -> AppResult<()> {
        let config = load(&[
            ("GEMINI_API_KEY", "key"),
            ("GEMINI_MODEL", "gemini-3.8-flash"),
            ("GEMINI_TIMEOUT_SECS", "45"),
            ("GEMINI_THINKING_LEVEL", "LOW"),
            ("GEMINI_MAX_CONCURRENCY", "2"),
            ("EXTRACTION_DAILY_LIMIT_PER_USER", "20"),
        ])?;
        assert_eq!(config.gemini.api_key.expose(), "key");
        assert_eq!(config.gemini.model, "gemini-3.8-flash");
        assert_eq!(config.gemini.timeout_secs, 45);
        assert_eq!(config.gemini.thinking_level, ThinkingLevel::Low);
        assert_eq!(config.gemini.max_concurrency, 2);
        assert_eq!(config.daily_limit_per_user, 20);
        config.validate()
    }

    #[test]
    fn invalid_values_are_rejected() -> AppResult<()> {
        assert!(load(&[("GEMINI_TIMEOUT_SECS", "soon")]).is_err());
        assert!(load(&[("GEMINI_THINKING_LEVEL", "extreme")]).is_err());
        for (name, value) in [
            ("GEMINI_TIMEOUT_SECS", "0"),
            ("GEMINI_MAX_CONCURRENCY", "64"),
            ("EXTRACTION_DAILY_LIMIT_PER_USER", "0"),
        ] {
            let config = load(&[("GEMINI_API_KEY", "k"), (name, value)])?;
            assert!(config.validate().is_err(), "{name}={value}");
        }
        Ok(())
    }

    #[test]
    fn model_ids_are_restricted_to_path_safe_characters() {
        for valid in ["gemini-3.5-flash-lite", "gemini_flash-latest"] {
            assert!(is_valid_model_id(valid), "{valid}");
        }
        for invalid in ["", "../x", "a/b", "model?key=1", ".hidden", "gemini flash"] {
            assert!(!is_valid_model_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn api_key_is_trimmed() -> AppResult<()> {
        let config = load(&[("GEMINI_API_KEY", "  AIza-key\n")])?;
        assert_eq!(config.gemini.api_key.expose(), "AIza-key");
        Ok(())
    }

    #[test]
    fn debug_output_redacts_the_api_key() -> AppResult<()> {
        let config = load(&[("GEMINI_API_KEY", "AIza-secret-value")])?;
        let printed = format!("{config:?}");
        assert!(!printed.contains("AIza-secret-value"), "{printed}");
        Ok(())
    }
}
