//! Tracing and logging setup module.
//!
//! This module provides:
//! - Structured logging configuration
//! - OpenTelemetry distributed tracing
//! - Tracing span creation utilities

use anyhow::Result;
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::prelude::*;

use crate::observability_config::ObservabilityConfig;

/// Initialize structured logging with tracing and configuration
pub fn init_tracing_with_config(config: &ObservabilityConfig) -> Result<()> {
    // Create the filter based on configuration
    let mut filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(format!("just_ingredients={}", config.log_level).parse()?)
        .add_directive("sqlx=warn".parse()?)
        .add_directive("teloxide=warn".parse()?);

    // Add observability-specific log level
    if let Ok(obs_log) = std::env::var("OBSERVABILITY_LOG_LEVEL") {
        filter =
            filter.add_directive(format!("just_ingredients::observability={}", obs_log).parse()?);
    }

    // Initialize based on environment (pretty for development, JSON for others)
    if config.is_development()
        || std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string()) == "pretty"
    {
        // Pretty formatting for development
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            )
            .init();
    } else {
        // JSON formatting for production (default)
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true),
            )
            .init();
    }

    tracing::info!(
        environment = %config.environment,
        log_level = %config.log_level,
        "Tracing initialized with structured logging"
    );
    Ok(())
}

/// Initialize OpenTelemetry distributed tracing with configuration
pub async fn init_opentelemetry_tracing_with_config(config: &ObservabilityConfig) -> Result<()> {
    // Only initialize if OTLP endpoint is configured
    if let Some(endpoint) = &config.otlp_endpoint {
        // Configure OTLP exporter
        let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .build()?;

        // Configure tracer provider with batch exporter
        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(otlp_exporter)
            .build();

        // Set global tracer provider
        global::set_tracer_provider(tracer_provider);

        tracing::info!(
            otlp_endpoint = %endpoint,
            trace_sampling_enabled = %config.enable_trace_sampling,
            trace_sampling_ratio = %config.trace_sampling_ratio,
            "OpenTelemetry tracing initialized with OTLP export"
        );
    } else {
        tracing::info!("OpenTelemetry tracing disabled (no OTLP endpoint configured)");
    }

    Ok(())
}

/// Initialize structured logging with tracing
#[allow(dead_code)]
pub fn init_tracing() -> Result<()> {
    // Determine log format from environment variable (default to JSON for production)
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());

    // Create the filter
    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("just_ingredients=info".parse()?)
        .add_directive("sqlx=warn".parse()?)
        .add_directive("teloxide=warn".parse()?);

    // Initialize based on format
    if log_format == "pretty" {
        // Pretty formatting for development
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            )
            .init();
    } else {
        // JSON formatting for production (default)
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true),
            )
            .init();
    }

    tracing::info!(log_format = %log_format, "Tracing initialized with structured logging");
    Ok(())
}

/// Initialize OpenTelemetry distributed tracing
#[allow(dead_code)]
pub async fn init_opentelemetry_tracing() -> Result<()> {
    // Configure OTLP exporter (can be configured via environment variables)
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(
            std::env::var("OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".to_string()),
        )
        .build()?;

    // Configure tracer provider with batch exporter
    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .build();

    // Set global tracer provider
    global::set_tracer_provider(tracer_provider);

    tracing::info!("OpenTelemetry tracing initialized with OTLP export");
    Ok(())
}

/// Create a span for OCR operations
pub fn ocr_span(operation: &str) -> tracing::Span {
    tracing::info_span!("ocr_operation", operation = operation, component = "ocr")
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
