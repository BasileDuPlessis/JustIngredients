//! Observability module for centralized metrics, tracing, and logging setup.
//!
//! This module provides:
//! - Metrics collection and Prometheus export
//! - Distributed tracing with OpenTelemetry
//! - Structured logging with configurable levels
//! - Health check endpoints for monitoring
//! - Environment-specific configuration support

use anyhow;

pub mod health_checks;
pub mod metrics;
pub mod system_monitoring;
pub mod tracing_mod;

pub use health_checks::*;
pub use metrics::*;
pub use system_monitoring::*;
pub use tracing_mod::*;

/// Initialize the complete observability stack
pub async fn init_observability() -> anyhow::Result<()> {
    let config = crate::observability_config::ObservabilityConfig::from_env();
    init_observability_with_config(config).await
}

/// Initialize the complete observability stack with custom configuration
pub async fn init_observability_with_config(
    config: crate::observability_config::ObservabilityConfig,
) -> anyhow::Result<()> {
    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid observability configuration: {}", e))?;

    // Initialize tracing first
    init_tracing_with_config(&config)?;

    // Initialize metrics
    let metrics_handle = metrics::init_metrics_with_config(&config)?;

    // Initialize OpenTelemetry tracing
    init_opentelemetry_tracing_with_config(&config).await?;

    // Start metrics server with basic health checks (no dependencies yet)
    metrics::start_metrics_server_basic_with_config(metrics_handle, config.metrics_port).await?;

    tracing::info!(
        environment = %config.environment,
        otlp_endpoint = ?config.otlp_endpoint,
        metrics_port = %config.metrics_port,
        "Observability stack initialized successfully"
    );
    Ok(())
}

/// Initialize observability with health check dependencies
pub async fn init_observability_with_health_checks(
    db_pool: Option<std::sync::Arc<sqlx::PgPool>>,
    bot_token: Option<String>,
) -> anyhow::Result<()> {
    let config = crate::config::AppConfig::from_env()?;
    init_observability_with_health_checks_and_config(db_pool, bot_token, &config).await
}

/// Initialize observability with health check dependencies and custom configuration
pub async fn init_observability_with_health_checks_and_config(
    db_pool: Option<std::sync::Arc<sqlx::PgPool>>,
    bot_token: Option<String>,
    config: &crate::config::AppConfig,
) -> anyhow::Result<()> {
    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid observability configuration: {}", e))?;

    // Initialize tracing first
    init_tracing_with_config(&config.observability)?;

    // Initialize metrics
    let metrics_handle = metrics::init_metrics_with_config(&config.observability)?;

    // Initialize OpenTelemetry tracing
    init_opentelemetry_tracing_with_config(&config.observability).await?;

    // Start metrics server with health checks
    metrics::start_metrics_server_with_health_checks(
        metrics_handle,
        config.server.health_port,
        db_pool.clone(),
        bot_token.clone(),
    )
    .await?;

    tracing::info!(
        environment = %config.observability.environment,
        has_db_pool = %db_pool.is_some(),
        has_bot_token = %bot_token.is_some(),
        "Observability stack with health checks initialized successfully"
    );
    Ok(())
}
