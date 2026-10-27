//! Configuration of the ingredient extraction backend, loaded from environment variables.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `EXTRACTION_BACKEND` | `tesseract` | `gemini` or `tesseract` |
//! | `GEMINI_API_KEY` | (none) | required when the backend is `gemini` |
//! | `GEMINI_MODEL` | `gemini-3.5-flash-lite` | model id |
//! | `GEMINI_TIMEOUT_SECS` | `30` | total timeout of one API call, 1–300 |
//! | `GEMINI_THINKING_LEVEL` | `minimal` | `minimal`, `low`, `medium` or `high` |
//! | `GEMINI_MAX_CONCURRENCY` | `4` | concurrent API calls, 1–32 |
//!
//! The backend stays `tesseract` by default until the Gemini path is wired into the bot
//! (migration Task 3.1), so deploying this code changes nothing.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt;
use std::str::FromStr;

use crate::errors::{AppError, AppResult};
use crate::secret::SecretString;

/// Default Gemini model, chosen with `scripts/llm_benchmark` (2026-10-26).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";

/// Engine that extracts ingredients from photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionBackend {
    Gemini,
    Tesseract,
}

impl FromStr for ExtractionBackend {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gemini" => Ok(Self::Gemini),
            "tesseract" => Ok(Self::Tesseract),
            other => Err(AppError::Config(format!(
                "EXTRACTION_BACKEND must be 'gemini' or 'tesseract', got '{other}'"
            ))),
        }
    }
}

impl fmt::Display for ExtractionBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Gemini => "gemini",
            Self::Tesseract => "tesseract",
        })
    }
}

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
            timeout_secs: 30,
            thinking_level: ThinkingLevel::Minimal,
            max_concurrency: 4,
        }
    }
}

/// Extraction settings: which backend runs, and the Gemini client settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionConfig {
    pub backend: ExtractionBackend,
    pub gemini: GeminiConfig,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            backend: ExtractionBackend::Tesseract,
            gemini: GeminiConfig::default(),
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
            backend: get("EXTRACTION_BACKEND")
                .map(|value| value.parse())
                .transpose()?
                .unwrap_or(defaults.backend),
            gemini: GeminiConfig {
                api_key: get("GEMINI_API_KEY")
                    .map(SecretString::new)
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
        })
    }

    /// Checks the values; the Gemini settings are only required when Gemini is the backend.
    pub fn validate(&self) -> AppResult<()> {
        if self.backend != ExtractionBackend::Gemini {
            return Ok(());
        }
        let gemini = &self.gemini;
        if gemini.api_key.is_empty() {
            return Err(AppError::Config(
                "GEMINI_API_KEY is required when EXTRACTION_BACKEND is 'gemini'".to_string(),
            ));
        }
        if gemini.model.is_empty() {
            return Err(AppError::Config("GEMINI_MODEL cannot be empty".to_string()));
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
        Ok(())
    }
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
    fn defaults_keep_tesseract_and_need_no_key() -> AppResult<()> {
        let config = load(&[])?;
        assert_eq!(config, ExtractionConfig::default());
        assert_eq!(config.backend, ExtractionBackend::Tesseract);
        assert_eq!(config.gemini.model, DEFAULT_GEMINI_MODEL);
        assert_eq!(config.gemini.thinking_level, ThinkingLevel::Minimal);
        config.validate()
    }

    #[test]
    fn gemini_backend_requires_an_api_key() -> AppResult<()> {
        let config = load(&[("EXTRACTION_BACKEND", "gemini")])?;
        assert!(config.validate().is_err());
        let config = load(&[("EXTRACTION_BACKEND", "Gemini"), ("GEMINI_API_KEY", "k")])?;
        config.validate()
    }

    #[test]
    fn all_variables_are_read() -> AppResult<()> {
        let config = load(&[
            ("EXTRACTION_BACKEND", "gemini"),
            ("GEMINI_API_KEY", "key"),
            ("GEMINI_MODEL", "gemini-3.8-flash"),
            ("GEMINI_TIMEOUT_SECS", "45"),
            ("GEMINI_THINKING_LEVEL", "LOW"),
            ("GEMINI_MAX_CONCURRENCY", "2"),
        ])?;
        assert_eq!(config.backend, ExtractionBackend::Gemini);
        assert_eq!(config.gemini.api_key.expose(), "key");
        assert_eq!(config.gemini.model, "gemini-3.8-flash");
        assert_eq!(config.gemini.timeout_secs, 45);
        assert_eq!(config.gemini.thinking_level, ThinkingLevel::Low);
        assert_eq!(config.gemini.max_concurrency, 2);
        config.validate()
    }

    #[test]
    fn invalid_values_are_rejected() -> AppResult<()> {
        assert!(load(&[("EXTRACTION_BACKEND", "openai")]).is_err());
        assert!(load(&[("GEMINI_TIMEOUT_SECS", "soon")]).is_err());
        assert!(load(&[("GEMINI_THINKING_LEVEL", "extreme")]).is_err());
        for (name, value) in [
            ("GEMINI_TIMEOUT_SECS", "0"),
            ("GEMINI_MAX_CONCURRENCY", "64"),
        ] {
            let config = load(&[
                ("EXTRACTION_BACKEND", "gemini"),
                ("GEMINI_API_KEY", "k"),
                (name, value),
            ])?;
            assert!(config.validate().is_err(), "{name}={value}");
        }
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
