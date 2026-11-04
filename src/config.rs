//! # Unified Application Configuration
//!
//! This module provides a centralized configuration system that consolidates
//! all application settings into a single, structured configuration object.
//! It supports loading from environment variables, validation, and provides
//! a clean interface for accessing configuration throughout the application.

use crate::errors::{AppError, AppResult};
use crate::extraction::ExtractionConfig;
use serde::{Deserialize, Serialize};
use std::env;

/// Bot-specific configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Telegram bot token
    pub token: String,
    /// HTTP client timeout in seconds
    pub http_timeout_secs: u64,
    /// How long a delivered message is remembered to drop a duplicate delivery, in seconds
    pub deduplication_ttl_secs: u64,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            http_timeout_secs: 30,
            deduplication_ttl_secs: 300, // 5 minutes
        }
    }
}

impl BotConfig {
    /// Validate bot configuration
    pub fn validate(&self) -> AppResult<()> {
        if self.token.trim().is_empty() {
            return Err(AppError::Config("Bot token cannot be empty".to_string()));
        }

        // Basic bot token format validation
        if !self.token.contains(':') {
            return Err(AppError::Config(
                "Bot token format is invalid. Expected format: 'bot_id:bot_token'".to_string(),
            ));
        }

        let parts: Vec<&str> = self.token.split(':').collect();
        if parts.len() != 2 {
            return Err(AppError::Config(
                "Bot token format is invalid. Expected format: 'bot_id:bot_token'".to_string(),
            ));
        }

        // Validate bot ID is numeric
        if parts[0].parse::<u64>().is_err() {
            return Err(AppError::Config(
                "Bot token bot ID must be numeric".to_string(),
            ));
        }

        // Validate bot token length
        if parts[1].len() < 20 {
            return Err(AppError::Config(
                "Bot token appears to be too short. Please verify it's a valid token".to_string(),
            ));
        }

        if self.http_timeout_secs == 0 {
            return Err(AppError::Config("HTTP timeout cannot be 0".to_string()));
        }

        if self.http_timeout_secs > 300 {
            return Err(AppError::Config(
                "HTTP timeout cannot be greater than 300 seconds".to_string(),
            ));
        }

        if self.deduplication_ttl_secs == 0 {
            return Err(AppError::Config(
                "Deduplication TTL cannot be 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Database configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    pub url: String,
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
    /// Minimum number of idle connections
    pub min_connections: u32,
    /// Maximum lifetime of a connection in seconds
    pub max_lifetime_secs: Option<u64>,
    /// Maximum time a connection can be idle in seconds
    pub idle_timeout_secs: Option<u64>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            connect_timeout_secs: 30,
            min_connections: 1,
            max_lifetime_secs: Some(1800), // 30 minutes
            idle_timeout_secs: Some(600),  // 10 minutes
        }
    }
}

impl DatabaseConfig {
    /// Validate database configuration
    pub fn validate(&self) -> AppResult<()> {
        if self.url.trim().is_empty() {
            return Err(AppError::Config("Database URL cannot be empty".to_string()));
        }

        // Basic PostgreSQL URL validation
        if !self.url.starts_with("postgresql://") && !self.url.starts_with("postgres://") {
            return Err(AppError::Config(
                "Database URL must start with 'postgresql://' or 'postgres://'".to_string(),
            ));
        }

        // Check for required components
        let url_parts: Vec<&str> = self.url.split("://").collect();
        if url_parts.len() != 2 {
            return Err(AppError::Config(
                "Database URL format is invalid".to_string(),
            ));
        }

        let connection_part = url_parts[1];
        if !connection_part.contains('@') {
            return Err(AppError::Config(
                "Database URL must contain authentication information".to_string(),
            ));
        }

        if self.max_connections == 0 {
            return Err(AppError::Config("Max connections cannot be 0".to_string()));
        }

        if self.max_connections > 100 {
            return Err(AppError::Config(
                "Max connections cannot be greater than 100".to_string(),
            ));
        }

        if self.connect_timeout_secs == 0 {
            return Err(AppError::Config("Connect timeout cannot be 0".to_string()));
        }

        if self.connect_timeout_secs > 300 {
            return Err(AppError::Config(
                "Connect timeout cannot be greater than 300 seconds".to_string(),
            ));
        }

        if self.min_connections > self.max_connections {
            return Err(AppError::Config(
                "Min connections cannot be greater than max connections".to_string(),
            ));
        }

        Ok(())
    }
}

/// Server configuration for health checks and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Health check server port
    pub health_port: u16,
    /// Port of the internal metrics server, collected by Fly (`[metrics]` in `fly.toml`)
    pub metrics_port: u16,
    /// Whether to allow privileged ports (< 1024)
    pub allow_privileged_ports: bool,
    /// Listen on all interfaces (on Fly) instead of localhost only
    /// (`METRICS_BIND_ALL_INTERFACES`)
    pub bind_all_interfaces: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            health_port: 8080,
            metrics_port: 9091,
            allow_privileged_ports: false,
            bind_all_interfaces: false,
        }
    }
}

impl ServerConfig {
    /// Validate server configuration
    pub fn validate(&self) -> AppResult<()> {
        if !self.allow_privileged_ports {
            if self.health_port < 1024 {
                return Err(AppError::Config(format!(
                    "Health port {} is privileged. Set allow_privileged_ports=true or use port >= 1024",
                    self.health_port
                )));
            }
            if self.metrics_port < 1024 {
                return Err(AppError::Config(format!(
                    "Metrics port {} is privileged. Set allow_privileged_ports=true or use port >= 1024",
                    self.metrics_port
                )));
            }
        }

        if self.health_port == self.metrics_port {
            return Err(AppError::Config(
                "Health port and metrics port cannot be the same".to_string(),
            ));
        }

        Ok(())
    }
}

/// Unified application configuration
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    /// Bot configuration
    pub bot: BotConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Server configuration
    pub server: ServerConfig,
    /// Ingredient extraction (Gemini client) configuration
    pub extraction: ExtractionConfig,
}

impl AppConfig {
    /// Loads the configuration from environment variables (read once, in `main`).
    pub fn from_env() -> AppResult<Self> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    /// Loads the configuration from `lookup`, which returns the value of a variable if set.
    /// Tests pass a map instead of mutating the process environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> AppResult<Self> {
        let defaults = Self::default();
        let get = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());
        let required = |name: &str| {
            get(name)
                .ok_or_else(|| AppError::Config(format!("{name} environment variable is required")))
        };
        let flag =
            |name: &str| get(name).is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));

        Ok(Self {
            bot: BotConfig {
                token: required("TELEGRAM_BOT_TOKEN")?,
                http_timeout_secs: number(
                    get("HTTP_CLIENT_TIMEOUT_SECS"),
                    "HTTP_CLIENT_TIMEOUT_SECS",
                )?
                .unwrap_or(defaults.bot.http_timeout_secs),
                deduplication_ttl_secs: number(
                    get("REQUEST_DEDUPLICATION_TTL_SECS"),
                    "REQUEST_DEDUPLICATION_TTL_SECS",
                )?
                .unwrap_or(defaults.bot.deduplication_ttl_secs),
            },
            database: DatabaseConfig {
                url: required("DATABASE_URL")?,
                max_connections: number(
                    get("DATABASE_MAX_CONNECTIONS"),
                    "DATABASE_MAX_CONNECTIONS",
                )?
                .unwrap_or(defaults.database.max_connections),
                min_connections: number(
                    get("DATABASE_MIN_CONNECTIONS"),
                    "DATABASE_MIN_CONNECTIONS",
                )?
                .unwrap_or(defaults.database.min_connections),
                connect_timeout_secs: number(
                    get("DATABASE_CONNECT_TIMEOUT_SECS"),
                    "DATABASE_CONNECT_TIMEOUT_SECS",
                )?
                .unwrap_or(defaults.database.connect_timeout_secs),
                ..DatabaseConfig::default()
            },
            server: ServerConfig {
                health_port: number(get("HEALTH_PORT"), "HEALTH_PORT")?
                    .unwrap_or(defaults.server.health_port),
                metrics_port: number(get("METRICS_PORT"), "METRICS_PORT")?
                    .unwrap_or(defaults.server.metrics_port),
                allow_privileged_ports: flag("ALLOW_PRIVILEGED_PORTS"),
                bind_all_interfaces: flag("METRICS_BIND_ALL_INTERFACES"),
            },
            extraction: ExtractionConfig::from_lookup(&lookup)?,
        })
    }

    /// Validate all configuration sections
    pub fn validate(&self) -> AppResult<()> {
        self.bot.validate()?;
        self.database.validate()?;
        self.server.validate()?;
        self.extraction.validate()?;
        Ok(())
    }

    /// Get a summary of the current configuration for logging
    pub fn summary(&self) -> String {
        format!(
            "Configuration: bot_token=[REDACTED], db_url=[REDACTED], health_port={}, metrics_port={}, gemini_model={}",
            self.server.health_port, self.server.metrics_port, self.extraction.gemini.model
        )
    }
}

/// A number from a variable, `None` when it is not set.
fn number<T: std::str::FromStr>(value: Option<String>, name: &str) -> AppResult<Option<T>> {
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

    #[test]
    fn test_default_config_validation() {
        let config = AppConfig::default();
        // Note: Default config may not be fully valid due to empty tokens/URLs
        // This test mainly checks that validation doesn't panic
        let _ = config.validate(); // We don't assert success since defaults may be invalid
    }

    #[test]
    fn test_bot_config_validation() {
        let mut config = BotConfig::default();

        // Invalid: empty token
        assert!(config.validate().is_err());

        // Invalid: malformed token
        config.token = "invalid-token".to_string();
        assert!(config.validate().is_err());

        // Invalid: short token
        config.token = "123:short".to_string();
        assert!(config.validate().is_err());

        // Valid token format
        config.token = "123456789:AAFakeTokenForTestingPurposes1234567890".to_string();
        assert!(config.validate().is_ok());

        // Invalid: zero timeout
        config.http_timeout_secs = 0;
        assert!(config.validate().is_err());
        config.http_timeout_secs = 30;

        // Invalid: zero deduplication TTL
        config.deduplication_ttl_secs = 0;
        assert!(config.validate().is_err());
        config.deduplication_ttl_secs = 300;

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_database_config_validation() {
        let mut config = DatabaseConfig::default();

        // Invalid: empty URL
        assert!(config.validate().is_err());

        // Invalid: wrong protocol
        config.url = "mysql://user:pass@localhost/db".to_string();
        assert!(config.validate().is_err());

        // Invalid: missing auth
        config.url = "postgresql://localhost/db".to_string();
        assert!(config.validate().is_err());

        // Valid URL
        config.url = "postgresql://user:pass@localhost:5432/db".to_string();
        assert!(config.validate().is_ok());

        // Invalid: zero max connections
        config.max_connections = 0;
        assert!(config.validate().is_err());
        config.max_connections = 10;

        // Invalid: min > max connections
        config.min_connections = 15;
        assert!(config.validate().is_err());
        config.min_connections = 1;

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_server_config_validation() {
        let mut config = ServerConfig::default();

        // Valid default config
        assert!(config.validate().is_ok());

        // Invalid: same ports
        config.health_port = 8080;
        config.metrics_port = 8080;
        assert!(config.validate().is_err());
        config.metrics_port = 9090;

        // Invalid: privileged ports without permission
        config.health_port = 80;
        assert!(config.validate().is_err());

        // Valid: privileged ports with permission
        config.allow_privileged_ports = true;
        assert!(config.validate().is_ok());
    }

    const TOKEN: &str = "123456789:AAFakeTokenForTestingPurposes1234567890";

    fn load(vars: &[(&str, &str)]) -> AppResult<AppConfig> {
        let vars: std::collections::HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        AppConfig::from_lookup(|name| vars.get(name).cloned())
    }

    /// What production sets (`fly.toml` and the Fly secrets): it loads and validates.
    #[test]
    fn production_like_configuration_is_valid() -> AppResult<()> {
        let config = load(&[
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            (
                "DATABASE_URL",
                "postgres://user:pass@db.internal:5432/app?sslmode=disable",
            ),
            ("GEMINI_API_KEY", "key"),
            ("HEALTH_PORT", "8080"),
            ("METRICS_PORT", "9091"),
            ("METRICS_BIND_ALL_INTERFACES", "true"),
        ])?;
        config.validate()?;
        assert_eq!(config.bot.http_timeout_secs, 30);
        assert_eq!(config.bot.deduplication_ttl_secs, 300);
        assert!(config.server.bind_all_interfaces);
        assert!(!config.summary().contains(TOKEN));
        Ok(())
    }

    /// Each variable fills its field (IMP-016: settings were read but not applied).
    #[test]
    fn each_variable_sets_its_field() -> AppResult<()> {
        let config = load(&[
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("DATABASE_URL", "postgres://user:pass@localhost/db"),
            ("HTTP_CLIENT_TIMEOUT_SECS", "45"),
            ("REQUEST_DEDUPLICATION_TTL_SECS", " 120 "),
            ("DATABASE_MAX_CONNECTIONS", "7"),
            ("DATABASE_MIN_CONNECTIONS", "2"),
            ("DATABASE_CONNECT_TIMEOUT_SECS", "12"),
            ("HEALTH_PORT", "8081"),
            ("METRICS_PORT", "9092"),
            ("ALLOW_PRIVILEGED_PORTS", "TRUE"),
            ("METRICS_BIND_ALL_INTERFACES", "false"),
        ])?;
        assert_eq!(config.bot.token, TOKEN);
        assert_eq!(config.bot.http_timeout_secs, 45);
        assert_eq!(config.bot.deduplication_ttl_secs, 120);
        assert_eq!(config.database.max_connections, 7);
        assert_eq!(config.database.min_connections, 2);
        assert_eq!(config.database.connect_timeout_secs, 12);
        assert_eq!(config.server.health_port, 8081);
        assert_eq!(config.server.metrics_port, 9092);
        assert!(config.server.allow_privileged_ports);
        assert!(!config.server.bind_all_interfaces);
        Ok(())
    }

    #[test]
    fn missing_or_unreadable_variables_are_refused() {
        assert!(
            load(&[("DATABASE_URL", "postgres://u:p@h/d")]).is_err(),
            "no token"
        );
        assert!(
            load(&[("TELEGRAM_BOT_TOKEN", TOKEN)]).is_err(),
            "no database"
        );
        assert!(
            load(&[
                ("TELEGRAM_BOT_TOKEN", "  "),
                ("DATABASE_URL", "postgres://u:p@h/d")
            ])
            .is_err()
        );
        for name in [
            "HTTP_CLIENT_TIMEOUT_SECS",
            "REQUEST_DEDUPLICATION_TTL_SECS",
            "HEALTH_PORT",
        ] {
            let loaded = load(&[
                ("TELEGRAM_BOT_TOKEN", TOKEN),
                ("DATABASE_URL", "postgres://u:p@h/d"),
                (name, "soon"),
            ]);
            assert!(loaded.is_err(), "{name}");
        }
    }
}
