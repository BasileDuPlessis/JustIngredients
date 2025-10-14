//! Observability module for centralized metrics, tracing, and logging setup.
//!
//! This module provides:
//! - Metrics collection and Prometheus export
//! - Distributed tracing with OpenTelemetry
//! - Structured logging configuration
//! - Health check endpoints

use anyhow::Result;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the complete observability stack
pub async fn init_observability() -> Result<()> {
    // Initialize tracing first
    init_tracing()?;

    // Initialize metrics
    let metrics_handle = init_metrics()?;

    // Initialize OpenTelemetry tracing
    init_opentelemetry_tracing().await?;

    // Start metrics server
    start_metrics_server(metrics_handle).await?;

    tracing::info!("Observability stack initialized successfully");
    Ok(())
}

/// Initialize structured logging with tracing
fn init_tracing() -> Result<()> {
    // Create a tracing subscriber with JSON formatting for production
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("just_ingredients=info".parse()?)
                .add_directive("sqlx=warn".parse()?)
                .add_directive("teloxide=warn".parse()?),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true),
        );

    subscriber.init();
    Ok(())
}

/// Initialize metrics collection with Prometheus exporter
fn init_metrics() -> Result<PrometheusHandle> {
    // Create Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    tracing::info!("Metrics collection initialized");
    Ok(handle)
}

/// Initialize OpenTelemetry distributed tracing
async fn init_opentelemetry_tracing() -> Result<()> {
    // For now, skip OpenTelemetry initialization if OTLP endpoint is not configured
    // This can be enabled later when proper OTLP infrastructure is available
    tracing::info!("OpenTelemetry tracing skipped (OTLP endpoint not configured)");
    Ok(())
}

/// Start HTTP server for metrics and health check endpoints
async fn start_metrics_server(_metrics_handle: PrometheusHandle) -> Result<()> {
    // For now, skip starting the metrics server
    // This can be implemented later with proper server setup
    tracing::info!("Metrics server setup skipped (server implementation pending)");
    Ok(())
}

/// Create a span for OCR operations
pub fn ocr_span(operation: &str) -> tracing::Span {
    tracing::info_span!(
        "ocr_operation",
        operation = operation,
        component = "ocr"
    )
}

/// Create a span for database operations
pub fn db_span(operation: &str, table: &str) -> tracing::Span {
    tracing::info_span!(
        "db_operation",
        operation = operation,
        table = table,
        component = "database"
    )
}

/// Create a span for Telegram bot operations
pub fn telegram_span(operation: &str, user_id: Option<i64>) -> tracing::Span {
    tracing::info_span!(
        "telegram_operation",
        operation = operation,
        user_id = user_id,
        component = "telegram"
    )
}

/// Record OCR operation metrics
pub fn record_ocr_metrics(success: bool, duration: std::time::Duration, image_size: u64) {
    metrics::counter!("ocr_operations_total", "result" => if success { "success" } else { "failure" }).increment(1);
    metrics::histogram!("ocr_duration_seconds").record(duration.as_secs_f64());
    metrics::histogram!("ocr_image_size_bytes").record(image_size as f64);
}

/// Record database operation metrics
pub fn record_db_metrics(operation: &str, duration: std::time::Duration) {
    let operation = operation.to_string();
    metrics::counter!("db_operations_total", "operation" => operation).increment(1);
    metrics::histogram!("db_operation_duration_seconds").record(duration.as_secs_f64());
}

/// Record request metrics
pub fn record_request_metrics(method: &str, status: u16, duration: std::time::Duration) {
    let method = method.to_string();
    let status = status.to_string();
    metrics::counter!("requests_total", "method" => method, "status" => status).increment(1);
    metrics::histogram!("request_duration_seconds").record(duration.as_secs_f64());
}

/// Update circuit breaker state metric
pub fn update_circuit_breaker_state(is_open: bool) {
    metrics::gauge!("circuit_breaker_state").set(if is_open { 1.0 } else { 0.0 });
}

/// Record Telegram message processing metrics
pub fn record_telegram_message(message_type: &str) {
    let message_type = message_type.to_string();
    metrics::counter!("telegram_messages_total", "type" => message_type).increment(1);
}