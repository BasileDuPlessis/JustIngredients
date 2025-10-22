//! # Unified Application Configuration
//!
//! This module provides a centralized configuration system that consolidates
//! all application settings into a single, structured configuration object.
//! It supports loading from environment variables, validation, and provides
//! a clean interface for accessing configuration throughout the application.

use crate::errors::{AppError, AppResult};
use crate::observability_config::ObservabilityConfig;
use crate::ocr_config::OcrConfig;
use crate::text_processing::{MeasurementConfig, MeasurementUnitsConfig};
use serde::{Deserialize, Serialize};
use std::env;

/// Bot-specific configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Telegram bot token
    pub token: String,
    /// HTTP client timeout in seconds
    pub http_timeout_secs: u64,
    /// Request deduplication TTL in seconds
    pub deduplication_ttl_secs: u64,
    /// Maximum concurrent requests per user
    pub max_concurrent_requests_per_user: usize,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            http_timeout_secs: 30,
            deduplication_ttl_secs: 300, // 5 minutes
            max_concurrent_requests_per_user: 3,
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

        if self.max_concurrent_requests_per_user == 0 {
            return Err(AppError::Config(
                "Max concurrent requests per user cannot be 0".to_string(),
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
    /// Metrics server port
    pub metrics_port: u16,
    /// Whether to allow privileged ports (< 1024)
    pub allow_privileged_ports: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            health_port: 8080,
            metrics_port: 9090,
            allow_privileged_ports: false,
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
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Bot configuration
    pub bot: BotConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Server configuration
    pub server: ServerConfig,
    /// OCR processing configuration
    pub ocr: OcrConfig,
    /// Observability configuration
    pub observability: ObservabilityConfig,
    /// Text processing configuration
    pub text_processing: MeasurementConfig,
    /// Measurement units configuration
    pub measurement_units: MeasurementUnitsConfig,
}

impl AppConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> AppResult<Self> {
        let mut config = Self::default();

        // Load bot configuration
        config.bot.token = env::var("TELEGRAM_BOT_TOKEN").map_err(|_| {
            AppError::Config("TELEGRAM_BOT_TOKEN environment variable is required".to_string())
        })?;
        config.bot.http_timeout_secs = env::var("HTTP_CLIENT_TIMEOUT_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("HTTP_CLIENT_TIMEOUT_SECS must be a valid number".to_string())
            })?;
        config.bot.deduplication_ttl_secs = env::var("REQUEST_DEDUPLICATION_TTL_SECS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config(
                    "REQUEST_DEDUPLICATION_TTL_SECS must be a valid number".to_string(),
                )
            })?;
        config.bot.max_concurrent_requests_per_user = env::var("MAX_CONCURRENT_REQUESTS_PER_USER")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config(
                    "MAX_CONCURRENT_REQUESTS_PER_USER must be a valid number".to_string(),
                )
            })?;

        // Load database configuration
        config.database.url = env::var("DATABASE_URL").map_err(|_| {
            AppError::Config("DATABASE_URL environment variable is required".to_string())
        })?;
        config.database.max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("DATABASE_MAX_CONNECTIONS must be a valid number".to_string())
            })?;
        config.database.connect_timeout_secs = env::var("DATABASE_CONNECT_TIMEOUT_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("DATABASE_CONNECT_TIMEOUT_SECS must be a valid number".to_string())
            })?;
        config.database.min_connections = env::var("DATABASE_MIN_CONNECTIONS")
            .unwrap_or_else(|_| "1".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("DATABASE_MIN_CONNECTIONS must be a valid number".to_string())
            })?;

        // Load server configuration
        config.server.health_port = env::var("HEALTH_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|_| AppError::Config("HEALTH_PORT must be a valid port number".to_string()))?;
        config.server.metrics_port = env::var("METRICS_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("METRICS_PORT must be a valid port number".to_string())
            })?;
        config.server.allow_privileged_ports = env::var("ALLOW_PRIVILEGED_PORTS")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        // Load OCR configuration (uses existing defaults and validation)
        config.ocr = OcrConfig::default();

        // Load observability configuration (uses existing defaults and validation)
        config.observability = ObservabilityConfig::default();

        // Load text processing configuration (uses existing defaults and validation)
        config.text_processing = MeasurementConfig::default();

        // Load measurement units configuration (from file)
        config.measurement_units = crate::text_processing::load_measurement_units_config();

        Ok(config)
    }

    /// Validate all configuration sections
    pub fn validate(&self) -> AppResult<()> {
        self.bot.validate()?;
        self.database.validate()?;
        self.server.validate()?;
        self.ocr.validate()?;
        self.observability.validate()?;
        self.text_processing.validate()?;
        self.measurement_units.validate()?;
        Ok(())
    }

    /// Get a summary of the current configuration for logging
    pub fn summary(&self) -> String {
        format!(
            "Configuration: bot_token=[REDACTED], db_url=[REDACTED], health_port={}, metrics_port={}, ocr_languages={}, observability_enabled={}",
            self.server.health_port,
            self.server.metrics_port,
            self.ocr.languages,
            self.observability.enable_metrics_export
        )
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bot: BotConfig::default(),
            database: DatabaseConfig::default(),
            server: ServerConfig::default(),
            ocr: OcrConfig::default(),
            observability: ObservabilityConfig::default(),
            text_processing: MeasurementConfig::default(),
            measurement_units: crate::text_processing::load_measurement_units_config(),
        }
    }
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

        // Invalid: zero max concurrent requests
        config.max_concurrent_requests_per_user = 0;
        assert!(config.validate().is_err());
        config.max_concurrent_requests_per_user = 3;

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
}
