//! # Production Configuration
//!
//! Environment-specific configuration for observability features
//! in production deployments.

use std::env;

/// Observability configuration for different environments
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// Environment name (development, staging, production)
    pub environment: String,
    /// OTLP endpoint for trace export
    pub otlp_endpoint: Option<String>,
    /// Prometheus metrics endpoint port
    pub metrics_port: u16,
    /// Log level for observability components
    pub log_level: String,
    /// Whether to enable trace sampling
    pub enable_trace_sampling: bool,
    /// Trace sampling ratio (0.0-1.0)
    pub trace_sampling_ratio: f64,
    /// Whether to export metrics to external Prometheus
    pub enable_metrics_export: bool,
    /// Additional tags for metrics and traces
    pub tags: Vec<(String, String)>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            environment: "development".to_string(),
            otlp_endpoint: None,
            metrics_port: 9090,
            log_level: "info".to_string(),
            enable_trace_sampling: false,
            trace_sampling_ratio: 1.0,
            enable_metrics_export: true,
            tags: Vec::new(),
        }
    }
}

impl ObservabilityConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            environment: env::var("ENVIRONMENT")
                .unwrap_or_else(|_| "development".to_string()),
            otlp_endpoint: env::var("OTLP_ENDPOINT").ok(),
            metrics_port: env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9090".to_string())
                .parse()
                .unwrap_or(9090),
            log_level: env::var("OBSERVABILITY_LOG_LEVEL")
                .unwrap_or_else(|_| "info".to_string()),
            enable_trace_sampling: env::var("ENABLE_TRACE_SAMPLING")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            trace_sampling_ratio: env::var("TRACE_SAMPLING_RATIO")
                .unwrap_or_else(|_| "1.0".to_string())
                .parse()
                .unwrap_or(1.0),
            enable_metrics_export: env::var("ENABLE_METRICS_EXPORT")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            tags: Vec::new(),
        }
    }

    /// Add default tags based on environment and configuration
    #[allow(dead_code)]
    fn add_default_tags(&mut self) {
        // Add environment tag
        self.tags.push(("environment".to_string(), self.environment.clone()));

        // Add service name
        self.tags.push(("service".to_string(), "just-ingredients-bot".to_string()));

        // Add version if available
        if let Ok(version) = env::var("SERVICE_VERSION") {
            self.tags.push(("version".to_string(), version));
        }

        // Add hostname if available
        if let Ok(hostname) = env::var("HOSTNAME") {
            self.tags.push(("hostname".to_string(), hostname));
        }
    }

    /// Check if running in production environment
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }

    /// Check if running in development environment
    pub fn is_development(&self) -> bool {
        self.environment == "development"
    }

    /// Get formatted tags for metrics/tracing
    pub fn get_tags(&self) -> Vec<(String, String)> {
        self.tags.clone()
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate OTLP endpoint format if provided
        if let Some(endpoint) = &self.otlp_endpoint {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(format!("Invalid OTLP endpoint format: {}", endpoint));
            }
        }

        // Validate sampling ratio
        if !(0.0..=1.0).contains(&self.trace_sampling_ratio) {
            return Err(format!("Invalid trace sampling ratio: {}", self.trace_sampling_ratio));
        }

        // Validate port range
        if self.metrics_port == 0 {
            return Err(format!("Invalid metrics port: {}", self.metrics_port));
        }

        Ok(())
    }
}

/// Parse tags from environment variable string
/// Format: "key1=value1,key2=value2,key3=value3"
    #[allow(dead_code)]
    fn parse_tags(tags_str: &str) -> Vec<(String, String)> {
    tags_str
        .split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    Some((key.trim().to_string(), value.trim().to_string()))
                }
                _ => None,
            }
        })
        .collect()
}

/// Environment-specific configuration presets
pub mod presets {
    use super::ObservabilityConfig;

    /// Development configuration with full observability
    pub fn development() -> ObservabilityConfig {
        ObservabilityConfig {
            environment: "development".to_string(),
            enable_trace_sampling: false,
            trace_sampling_ratio: 1.0, // Sample all traces in development
            enable_metrics_export: true,
            log_level: "debug".to_string(),
            ..Default::default()
        }
    }

    /// Staging configuration with moderate observability
    pub fn staging() -> ObservabilityConfig {
        ObservabilityConfig {
            environment: "staging".to_string(),
            enable_trace_sampling: true,
            trace_sampling_ratio: 0.5, // Sample 50% of traces
            enable_metrics_export: true,
            log_level: "info".to_string(),
            ..Default::default()
        }
    }

    /// Production configuration with optimized observability
    pub fn production() -> ObservabilityConfig {
        ObservabilityConfig {
            environment: "production".to_string(),
            enable_trace_sampling: true,
            trace_sampling_ratio: 0.1, // Sample 10% of traces for performance
            enable_metrics_export: true,
            log_level: "warn".to_string(),
            ..Default::default()
        }
    }

    /// Minimal configuration for resource-constrained environments
    pub fn minimal() -> ObservabilityConfig {
        ObservabilityConfig {
            environment: "minimal".to_string(),
            enable_trace_sampling: true,
            trace_sampling_ratio: 0.01, // Sample only 1% of traces
            enable_metrics_export: false, // Disable metrics export
            log_level: "error".to_string(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ObservabilityConfig::default();
        assert_eq!(config.environment, "development");
        assert_eq!(config.metrics_port, 9090);
        assert_eq!(config.log_level, "info");
        assert!(!config.enable_trace_sampling);
        assert_eq!(config.trace_sampling_ratio, 1.0);
        assert!(config.enable_metrics_export);
    }

    #[test]
    fn test_config_validation() {
        let mut config = ObservabilityConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid OTLP endpoint
        config.otlp_endpoint = Some("invalid-endpoint".to_string());
        assert!(config.validate().is_err());

        // Reset and test invalid sampling ratio
        config.otlp_endpoint = None;
        config.trace_sampling_ratio = 1.5;
        assert!(config.validate().is_err());

        // Reset and test invalid port
        config.trace_sampling_ratio = 1.0;
        config.metrics_port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_tag_parsing() {
        let tags_str = "env=prod,version=1.2.3,service=bot";
        let tags = parse_tags(tags_str);

        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0], ("env".to_string(), "prod".to_string()));
        assert_eq!(tags[1], ("version".to_string(), "1.2.3".to_string()));
        assert_eq!(tags[2], ("service".to_string(), "bot".to_string()));
    }

    #[test]
    fn test_presets() {
        let dev = presets::development();
        assert_eq!(dev.environment, "development");
        assert!(!dev.enable_trace_sampling);
        assert_eq!(dev.trace_sampling_ratio, 1.0);

        let prod = presets::production();
        assert_eq!(prod.environment, "production");
        assert!(prod.enable_trace_sampling);
        assert_eq!(prod.trace_sampling_ratio, 0.1);

        let minimal = presets::minimal();
        assert_eq!(minimal.environment, "minimal");
        assert!(!minimal.enable_metrics_export);
    }

    #[test]
    fn test_environment_detection() {
        let dev = presets::development();
        assert!(dev.is_development());
        assert!(!dev.is_production());

        let prod = presets::production();
        assert!(!prod.is_development());
        assert!(prod.is_production());
    }
}