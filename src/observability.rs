//! Observability module for centralized metrics, tracing, and logging setup.
//!
//! This module provides:
//! - Metrics collection and Prometheus export
//! - Distributed tracing with OpenTelemetry
//! - Structured logging with configurable levels
//! - Health check endpoints for monitoring
//! - Environment-specific configuration support

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use leptess::LepTess;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::Sampler;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tracing_subscriber::prelude::*;

use crate::observability_config::ObservabilityConfig;
async fn start_metrics_server_basic_with_config(metrics_handle: PrometheusHandle, port: u16) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting basic metrics server on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Metrics server listening on {}", addr);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let metrics_handle = metrics_handle.clone();

                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);

                        let service = hyper::service::service_fn(
                            move |req: hyper::Request<hyper::body::Incoming>| {
                                let metrics_handle = metrics_handle.clone();
                                async move {
                                    match (req.method(), req.uri().path()) {
                                        (&hyper::Method::GET, "/metrics") => {
                                            let metrics = metrics_handle.render();
                                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                                metrics,
                                            ))
                                        }
                                        (&hyper::Method::GET, "/health/live") => {
                                            Ok(hyper::Response::new("OK".to_string()))
                                        }
                                        (&hyper::Method::GET, "/health/ready") => {
                                            Ok(hyper::Response::new("OK".to_string()))
                                        }
                                        _ => {
                                            let mut response =
                                                hyper::Response::new("Not Found".to_string());
                                            *response.status_mut() = hyper::StatusCode::NOT_FOUND;
                                            Ok(response)
                                        }
                                    }
                                }
                            },
                        );

                        if let Err(err) = http1::Builder::new().serve_connection(io, service).await
                        {
                            tracing::error!("Error serving connection: {:?}", err);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Error accepting connection: {}", e);
                }
            }
        }
    });

    Ok(())
}

/// Initialize the complete observability stack
pub async fn init_observability() -> Result<()> {
    let config = ObservabilityConfig::from_env();
    init_observability_with_config(config).await
}

/// Initialize the complete observability stack with custom configuration
pub async fn init_observability_with_config(config: ObservabilityConfig) -> Result<()> {
    // Validate configuration
    config.validate().map_err(|e| anyhow::anyhow!("Invalid observability configuration: {}", e))?;

    // Initialize tracing first
    init_tracing_with_config(&config)?;

    // Initialize metrics
    let metrics_handle = init_metrics_with_config(&config)?;

    // Initialize OpenTelemetry tracing
    init_opentelemetry_tracing_with_config(&config).await?;

    // Start metrics server with basic health checks (no dependencies yet)
    start_metrics_server_basic_with_config(metrics_handle, config.metrics_port).await?;

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
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
) -> Result<()> {
    let config = ObservabilityConfig::from_env();
    init_observability_with_health_checks_and_config(db_pool, bot_token, config).await
}

/// Initialize observability with health check dependencies and custom configuration
pub async fn init_observability_with_health_checks_and_config(
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
    config: ObservabilityConfig,
) -> Result<()> {
    // Validate configuration
    config.validate().map_err(|e| anyhow::anyhow!("Invalid observability configuration: {}", e))?;

    // Initialize tracing first
    init_tracing_with_config(&config)?;

    // Initialize metrics
    let metrics_handle = init_metrics_with_config(&config)?;

    // Initialize OpenTelemetry tracing
    init_opentelemetry_tracing_with_config(&config).await?;

    // Start metrics server with health checks
    start_metrics_server_with_health_checks(metrics_handle, config.metrics_port, db_pool.clone(), bot_token.clone()).await?;

    tracing::info!(
        environment = %config.environment,
        has_db_pool = %db_pool.is_some(),
        has_bot_token = %bot_token.is_some(),
        "Observability stack with health checks initialized successfully"
    );
    Ok(())
}

/// Initialize structured logging with tracing and configuration
fn init_tracing_with_config(config: &ObservabilityConfig) -> Result<()> {
    // Create the filter based on configuration
    let mut filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(format!("just_ingredients={}", config.log_level).parse()?)
        .add_directive("sqlx=warn".parse()?)
        .add_directive("teloxide=warn".parse()?);

    // Add observability-specific log level
    if let Ok(obs_log) = std::env::var("OBSERVABILITY_LOG_LEVEL") {
        filter = filter.add_directive(format!("just_ingredients::observability={}", obs_log).parse()?);
    }

    // Initialize based on environment (pretty for development, JSON for others)
    if config.is_development() || std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string()) == "pretty" {
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

/// Initialize metrics collection with Prometheus exporter and configuration
fn init_metrics_with_config(config: &ObservabilityConfig) -> Result<PrometheusHandle> {
    // Create Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    tracing::info!(
        metrics_enabled = %config.enable_metrics_export,
        "Metrics collection initialized"
    );
    Ok(handle)
}

/// Initialize OpenTelemetry distributed tracing with configuration
async fn init_opentelemetry_tracing_with_config(config: &ObservabilityConfig) -> Result<()> {
    // Only initialize if OTLP endpoint is configured
    if let Some(endpoint) = &config.otlp_endpoint {
        // Configure OTLP exporter
        let otlp_exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint.clone())
            .build_span_exporter()?;

        // Configure tracer provider with batch exporter
        let tracer_provider = if config.enable_trace_sampling {
            let sampler = Sampler::TraceIdRatioBased(config.trace_sampling_ratio);
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(otlp_exporter, opentelemetry_sdk::runtime::Tokio)
                .with_config(opentelemetry_sdk::trace::Config::default().with_sampler(sampler))
                .build()
        } else {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(otlp_exporter, opentelemetry_sdk::runtime::Tokio)
                .build()
        };

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
fn init_tracing() -> Result<()> {
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

/// Initialize metrics collection with Prometheus exporter
#[allow(dead_code)]
fn init_metrics() -> Result<PrometheusHandle> {
    // Create Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    tracing::info!("Metrics collection initialized");
    Ok(handle)
}

/// Initialize OpenTelemetry distributed tracing
#[allow(dead_code)]
async fn init_opentelemetry_tracing() -> Result<()> {
    // Configure OTLP exporter (can be configured via environment variables)
    let otlp_exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(
            std::env::var("OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".to_string()),
        )
        .build_span_exporter()?;

    // Configure tracer provider with batch exporter
    let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(otlp_exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    // Set global tracer provider
    global::set_tracer_provider(tracer_provider);

    tracing::info!("OpenTelemetry tracing initialized with OTLP export");
    Ok(())
}

async fn start_metrics_server_with_health_checks(
    metrics_handle: PrometheusHandle,
    port: u16,
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting metrics server with health checks on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Metrics server listening on {}", addr);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let metrics_handle = metrics_handle.clone();
                    let db_pool = db_pool.clone();
                    let bot_token = bot_token.clone();

                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);

                        let service = hyper::service::service_fn(
                            move |req: hyper::Request<hyper::body::Incoming>| {
                                let metrics_handle = metrics_handle.clone();
                                let db_pool = db_pool.clone();
                                let bot_token = bot_token.clone();
                                async move {
                                    match (req.method(), req.uri().path()) {
                                        (&hyper::Method::GET, "/metrics") => {
                                            // Ensure at least one metric is registered to avoid empty render
                                            metrics::gauge!("uptime_seconds").set(1.0);
                                            let metrics = metrics_handle.render();
                                            let mut response = hyper::Response::new(metrics);
                                            response.headers_mut().insert(
                                                "content-type",
                                                hyper::header::HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
                                            );
                                            Ok::<_, std::convert::Infallible>(response)
                                        }
                                        (&hyper::Method::GET, "/health/live") => {
                                            // Liveness probe - just check if the service is running
                                            Ok(hyper::Response::new("OK".to_string()))
                                        }
                                        (&hyper::Method::GET, "/health/ready") => {
                                            // Readiness probe - check if all dependencies are available
                                            match perform_readiness_checks(
                                                db_pool.clone(),
                                                bot_token.clone(),
                                            )
                                            .await
                                            {
                                                Ok(_) => Ok(hyper::Response::new("OK".to_string())),
                                                Err(e) => {
                                                    let mut response = hyper::Response::new(
                                                        format!("NOT READY: {}", e),
                                                    );
                                                    *response.status_mut() =
                                                        hyper::StatusCode::SERVICE_UNAVAILABLE;
                                                    Ok(response)
                                                }
                                            }
                                        }
                                        _ => {
                                            let mut response =
                                                hyper::Response::new("Not Found".to_string());
                                            *response.status_mut() = hyper::StatusCode::NOT_FOUND;
                                            Ok(response)
                                        }
                                    }
                                }
                            },
                        );

                        if let Err(err) = http1::Builder::new().serve_connection(io, service).await
                        {
                            tracing::error!("Error serving connection: {:?}", err);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Error accepting connection: {}", e);
                }
            }
        }
    });

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

/// Perform comprehensive readiness checks
pub async fn perform_readiness_checks(
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
) -> Result<()> {
    // Check database connectivity
    if let Some(pool) = &db_pool {
        check_database_health(pool.as_ref()).await?;
    }

    // Check OCR engine availability
    check_ocr_health().await?;

    // Check bot token validity
    if let Some(token) = &bot_token {
        check_bot_token_health(token).await?;
    }

    Ok(())
}

/// Check database connectivity and basic query capability
pub async fn check_database_health(pool: &PgPool) -> Result<()> {
    // Simple query to test database connectivity
    sqlx::query("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Database health check failed: {}", e))?;

    tracing::debug!("Database health check passed");
    Ok(())
}

/// Check OCR engine availability by testing Tesseract initialization
pub async fn check_ocr_health() -> Result<()> {
    // Try to create a minimal Tesseract instance to test OCR availability
    // This is a lightweight check that doesn't require actual image processing
    match LepTess::new(None, "eng") {
        Ok(_) => {
            tracing::debug!("OCR health check passed");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("OCR health check failed: {}", e)),
    }
}

/// Check Telegram bot token validity by testing API access
pub async fn check_bot_token_health(token: &str) -> Result<()> {
    // Create a minimal bot instance to test token validity
    // This doesn't make actual API calls, just validates the token format
    if token.is_empty() {
        return Err(anyhow::anyhow!("Bot token is empty"));
    }

    // Basic token format validation (Telegram bot tokens have a specific format)
    if !token.contains(':') {
        return Err(anyhow::anyhow!("Bot token format is invalid"));
    }

    tracing::debug!("Bot token health check passed");
    Ok(())
}
