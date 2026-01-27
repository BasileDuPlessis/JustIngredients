//! Metrics collection and Prometheus export module.
//!
//! This module provides:
//! - Rate limiting for HTTP requests
//! - Authentication for metrics endpoints
//! - Prometheus metrics server setup
//! - Comprehensive metrics recording functions

use anyhow::Result;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

use crate::observability_config::ObservabilityConfig;

/// Simple rate limiter for HTTP requests
#[derive(Debug)]
pub struct RateLimiter {
    requests: Mutex<HashMap<String, Vec<Instant>>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            max_requests,
            window_secs,
        }
    }

    /// Check if request is allowed for the given IP
    pub fn is_allowed(&self, ip: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);

        let mut requests = self
            .requests
            .lock()
            .expect("Failed to acquire mutex for rate limiting");
        let client_requests = requests.entry(ip.to_string()).or_default();

        // Remove old requests outside the window
        client_requests.retain(|&time| now.duration_since(time) < window);

        // Check if under limit
        if client_requests.len() >= self.max_requests as usize {
            return false;
        }

        // Add current request
        client_requests.push(now);
        true
    }
}

/// Check authentication token from Authorization header
pub fn check_auth(req: &hyper::Request<hyper::body::Incoming>) -> bool {
    // Get auth token from environment
    let expected_token = match std::env::var("METRICS_AUTH_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return true, // No token required if not set (for development)
    };

    // Check Authorization header
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token == expected_token;
            }
        }
    }

    false
}

/// Check request size limit
pub fn check_request_size(req: &hyper::Request<hyper::body::Incoming>) -> bool {
    const MAX_REQUEST_SIZE: u64 = 1024 * 1024; // 1MB limit

    if let Some(content_length) = req.headers().get("content-length") {
        if let Ok(size_str) = content_length.to_str() {
            if let Ok(size) = size_str.parse::<u64>() {
                return size <= MAX_REQUEST_SIZE;
            }
        }
        return false; // Invalid content-length header
    }

    true // No content-length header (GET requests)
}

/// Initialize metrics collection with Prometheus exporter and configuration
pub fn init_metrics_with_config(config: &ObservabilityConfig) -> Result<PrometheusHandle> {
    // Create Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    tracing::info!(
        metrics_enabled = %config.enable_metrics_export,
        "Metrics collection initialized"
    );
    Ok(handle)
}

/// Initialize metrics collection with Prometheus exporter
#[allow(dead_code)]
pub fn init_metrics() -> Result<PrometheusHandle> {
    // Create Prometheus recorder
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    tracing::info!("Metrics collection initialized");
    Ok(handle)
}

/// Start basic metrics server with basic health checks (no dependencies yet)
pub async fn start_metrics_server_basic_with_config(
    metrics_handle: PrometheusHandle,
    port: u16,
) -> Result<()> {
    // Determine bind address - localhost for security unless explicitly configured
    let bind_all = std::env::var("METRICS_BIND_ALL_INTERFACES")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let addr = if bind_all {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port)
    } else {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) // localhost only
    };

    tracing::info!(
        "Starting basic metrics server on {} (bind_all: {})",
        addr,
        bind_all
    );

    // Initialize rate limiter (10 requests per minute per IP)
    let rate_limiter = Arc::new(RateLimiter::new(10, 60));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Metrics server listening on {}", addr);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let metrics_handle = metrics_handle.clone();
                    let rate_limiter = rate_limiter.clone();

                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);

                        let service = hyper::service::service_fn(
                            move |req: hyper::Request<hyper::body::Incoming>| {
                                let metrics_handle = metrics_handle.clone();
                                let peer_ip = peer_addr.ip().to_string();
                                let rate_limiter = rate_limiter.clone();
                                async move {
                                    // Rate limiting check
                                    if !rate_limiter.is_allowed(&peer_ip) {
                                        let mut response =
                                            hyper::Response::new("Rate limit exceeded".to_string());
                                        *response.status_mut() =
                                            hyper::StatusCode::TOO_MANY_REQUESTS;
                                        return Ok::<_, std::convert::Infallible>(response);
                                    }

                                    // Request size check
                                    if !check_request_size(&req) {
                                        let mut response =
                                            hyper::Response::new("Request too large".to_string());
                                        *response.status_mut() =
                                            hyper::StatusCode::PAYLOAD_TOO_LARGE;
                                        return Ok(response);
                                    }

                                    // Authentication check
                                    if !check_auth(&req) {
                                        let mut response =
                                            hyper::Response::new("Unauthorized".to_string());
                                        *response.status_mut() = hyper::StatusCode::UNAUTHORIZED;
                                        response.headers_mut().insert(
                                            "www-authenticate",
                                            hyper::header::HeaderValue::from_static("Bearer"),
                                        );
                                        return Ok(response);
                                    }

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
                            crate::errors::error_logging::log_network_error(
                                &err,
                                "serve_http_connection",
                                Some(&format!("{}:{}", peer_addr.ip(), peer_addr.port())),
                                None,
                            );
                        }
                    });
                }
                Err(e) => {
                    crate::errors::error_logging::log_network_error(
                        &e,
                        "accept_tcp_connection",
                        Some(&addr.to_string()),
                        None,
                    );
                }
            }
        }
    });

    Ok(())
}

/// Start metrics server with health checks
pub async fn start_metrics_server_with_health_checks(
    metrics_handle: PrometheusHandle,
    port: u16,
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
) -> Result<()> {
    // Determine bind address - localhost for security unless explicitly configured
    let bind_all = std::env::var("METRICS_BIND_ALL_INTERFACES")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let addr = if bind_all {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port)
    } else {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) // localhost only
    };

    tracing::info!(
        "Starting metrics server with health checks on {} (bind_all: {})",
        addr,
        bind_all
    );

    // Initialize rate limiter (10 requests per minute per IP)
    let rate_limiter = Arc::new(RateLimiter::new(10, 60));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Metrics server listening on {}", addr);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let metrics_handle = metrics_handle.clone();
                    let db_pool = db_pool.clone();
                    let bot_token = bot_token.clone();
                    let rate_limiter = rate_limiter.clone();

                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);

                        let service = hyper::service::service_fn(
                            move |req: hyper::Request<hyper::body::Incoming>| {
                                let metrics_handle = metrics_handle.clone();
                                let db_pool = db_pool.clone();
                                let bot_token = bot_token.clone();
                                let peer_ip = peer_addr.ip().to_string();
                                let rate_limiter = rate_limiter.clone();
                                async move {
                                    // Rate limiting check
                                    if !rate_limiter.is_allowed(&peer_ip) {
                                        let mut response =
                                            hyper::Response::new("Rate limit exceeded".to_string());
                                        *response.status_mut() =
                                            hyper::StatusCode::TOO_MANY_REQUESTS;
                                        return Ok::<_, std::convert::Infallible>(response);
                                    }

                                    // Request size check
                                    if !check_request_size(&req) {
                                        let mut response =
                                            hyper::Response::new("Request too large".to_string());
                                        *response.status_mut() =
                                            hyper::StatusCode::PAYLOAD_TOO_LARGE;
                                        return Ok(response);
                                    }

                                    // Authentication check
                                    if !check_auth(&req) {
                                        let mut response =
                                            hyper::Response::new("Unauthorized".to_string());
                                        *response.status_mut() = hyper::StatusCode::UNAUTHORIZED;
                                        response.headers_mut().insert(
                                            "www-authenticate",
                                            hyper::header::HeaderValue::from_static("Bearer"),
                                        );
                                        return Ok(response);
                                    }

                                    match (req.method(), req.uri().path()) {
                                        (&hyper::Method::GET, "/metrics") => {
                                            // Ensure at least one metric is registered to avoid empty render
                                            metrics::gauge!("uptime_seconds").set(1.0);
                                            let metrics = metrics_handle.render();
                                            let mut response = hyper::Response::new(metrics);
                                            response.headers_mut().insert(
                                                "content-type",
                                                hyper::header::HeaderValue::from_static(
                                                    "text/plain; version=0.0.4; charset=utf-8",
                                                ),
                                            );
                                            Ok::<_, std::convert::Infallible>(response)
                                        }
                                        (&hyper::Method::GET, "/health/live") => {
                                            // Liveness probe - just check if the service is running
                                            Ok(hyper::Response::new("OK".to_string()))
                                        }
                                        (&hyper::Method::GET, "/health/ready") => {
                                            // Readiness probe - check if all dependencies are available
                                            match crate::observability::health_checks::perform_readiness_checks(
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
                            crate::errors::error_logging::log_network_error(
                                &err,
                                "serve_http_connection",
                                Some(&format!("{}:{}", peer_addr.ip(), peer_addr.port())),
                                None,
                            );
                        }
                    });
                }
                Err(e) => {
                    crate::errors::error_logging::log_network_error(
                        &e,
                        "accept_tcp_connection",
                        Some(&addr.to_string()),
                        None,
                    );
                }
            }
        }
    });

    Ok(())
}

/// Record OCR operation metrics
pub fn record_ocr_metrics(success: bool, duration: std::time::Duration, image_size: u64) {
    metrics::counter!("ocr_operations_total", "result" => if success { "success" } else { "failure" }).increment(1);
    metrics::histogram!("ocr_duration_seconds").record(duration.as_secs_f64());
    metrics::histogram!("ocr_image_size_bytes").record(image_size as f64);
}

/// Parameters for OCR performance metrics recording
#[derive(Debug, Clone)]
pub struct OcrPerformanceMetricsParams {
    pub success: bool,
    pub total_duration: std::time::Duration,
    pub ocr_duration: std::time::Duration,
    pub image_size: u64,
    pub attempt_count: u32,
    pub memory_estimate_mb: f64,
}

/// Record detailed OCR performance metrics including memory and throughput
pub fn record_ocr_performance_metrics(params: OcrPerformanceMetricsParams) {
    let OcrPerformanceMetricsParams {
        success,
        total_duration,
        ocr_duration,
        image_size,
        attempt_count,
        memory_estimate_mb,
    } = params;

    // Basic metrics
    record_ocr_metrics(success, total_duration, image_size);

    // Detailed performance metrics
    metrics::histogram!("ocr_processing_duration_seconds").record(ocr_duration.as_secs_f64());
    metrics::histogram!("ocr_overhead_duration_seconds")
        .record((total_duration - ocr_duration).as_secs_f64());
    metrics::histogram!("ocr_memory_estimate_mb").record(memory_estimate_mb);
    metrics::histogram!("ocr_retry_attempts").record(attempt_count as f64);

    // Throughput metrics (operations per second)
    let ops_per_sec = if total_duration.as_secs_f64() > 0.0 {
        1.0 / total_duration.as_secs_f64()
    } else {
        0.0
    };
    metrics::histogram!("ocr_throughput_ops_per_sec").record(ops_per_sec);

    // Efficiency metrics (processing time vs total time)
    let efficiency = if total_duration.as_secs_f64() > 0.0 {
        ocr_duration.as_secs_f64() / total_duration.as_secs_f64()
    } else {
        0.0
    };
    metrics::histogram!("ocr_efficiency_ratio").record(efficiency);
}

/// Record database operation metrics
pub fn record_db_metrics(operation: &str, duration: std::time::Duration) {
    let operation = operation.to_string();
    metrics::counter!("db_operations_total", "operation" => operation).increment(1);
    metrics::histogram!("db_operation_duration_seconds").record(duration.as_secs_f64());
}

/// Record detailed database performance metrics
pub fn record_db_performance_metrics(
    operation: &str,
    duration: std::time::Duration,
    rows_affected: u64,
    query_complexity: QueryComplexity,
) {
    // Basic metrics
    record_db_metrics(operation, duration);

    // Detailed performance metrics
    let operation = operation.to_string();
    metrics::histogram!("db_rows_affected", "operation" => operation.clone())
        .record(rows_affected as f64);

    // Query complexity metrics
    let complexity_score = match query_complexity {
        QueryComplexity::Simple => 1.0,
        QueryComplexity::Medium => 2.0,
        QueryComplexity::Complex => 3.0,
    };
    metrics::histogram!("db_query_complexity", "operation" => operation.clone())
        .record(complexity_score);

    // Performance classification
    let perf_class = if duration.as_millis() < 10 {
        "fast"
    } else if duration.as_millis() < 100 {
        "medium"
    } else {
        "slow"
    };
    metrics::counter!("db_performance_class_total", "operation" => operation, "class" => perf_class.to_string()).increment(1);
}

/// Query complexity classification for performance monitoring
#[derive(Debug, Clone, Copy)]
pub enum QueryComplexity {
    Simple,  // Basic CRUD operations
    Medium,  // Joins, aggregations
    Complex, // Full-text search, complex queries
}

/// Record request metrics
pub fn record_request_metrics(method: &str, status: u16, duration: std::time::Duration) {
    let method = method.to_string();
    let status = status.to_string();
    metrics::counter!("requests_total", "method" => method, "status" => status).increment(1);
    metrics::histogram!("request_duration_seconds").record(duration.as_secs_f64());
}

/// Record health check metrics
pub fn record_health_check_metrics(check_type: &str, success: bool, duration: std::time::Duration) {
    let check_type = check_type.to_string();
    metrics::counter!("health_checks_total", "type" => check_type.clone(), "result" => if success { "success" } else { "failure" }.to_string()).increment(1);
    metrics::histogram!("health_check_duration_seconds", "type" => check_type.clone())
        .record(duration.as_secs_f64());

    // Update health status gauge
    metrics::gauge!("health_check_status", "type" => check_type).set(if success {
        1.0
    } else {
        0.0
    });
}

/// Record error rate metrics
pub fn record_error_metrics(error_type: &str, component: &str) {
    let error_type = error_type.to_string();
    let component = component.to_string();
    metrics::counter!("errors_total", "type" => error_type, "component" => component).increment(1);
}

/// Record queue/depth metrics for async operations
pub fn record_queue_metrics(queue_name: &str, depth: usize, capacity: usize) {
    let queue_name = queue_name.to_string();
    metrics::gauge!("queue_depth", "queue" => queue_name.clone()).set(depth as f64);
    metrics::gauge!("queue_capacity", "queue" => queue_name).set(capacity as f64);
}

/// Record throughput metrics
pub fn record_throughput_metrics(component: &str, operations: u64, time_window_secs: f64) {
    let component = component.to_string();
    let throughput = operations as f64 / time_window_secs;
    metrics::histogram!("component_throughput_ops_per_sec", "component" => component)
        .record(throughput);
}

/// Record application startup metrics
pub fn record_startup_metrics(duration: std::time::Duration) {
    metrics::histogram!("application_startup_duration_seconds").record(duration.as_secs_f64());
    metrics::counter!("application_starts_total").increment(1);
}

/// Record application uptime
pub fn record_uptime(uptime_secs: f64) {
    metrics::gauge!("application_uptime_seconds").set(uptime_secs);
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

/// Record duplicate Telegram message detection
pub fn record_telegram_duplicate_message() {
    metrics::counter!("telegram_duplicate_messages_total").increment(1);
}

/// Record detailed Telegram bot performance metrics
pub fn record_telegram_performance_metrics(
    message_type: &str,
    processing_duration: std::time::Duration,
    user_id: Option<i64>,
    message_size: usize,
    has_media: bool,
) {
    // Basic metrics
    record_telegram_message(message_type);

    // Detailed performance metrics
    let message_type = message_type.to_string();
    metrics::histogram!("telegram_processing_duration_seconds", "type" => message_type.clone())
        .record(processing_duration.as_secs_f64());
    metrics::histogram!("telegram_message_size_bytes", "type" => message_type.clone())
        .record(message_size as f64);
    metrics::counter!("telegram_media_messages_total", "has_media" => has_media.to_string())
        .increment(1);

    // User engagement metrics
    if user_id.is_some() {
        metrics::counter!("telegram_active_users_total").increment(1);
    }
}

/// Record text processing performance metrics
pub fn record_text_processing_metrics(
    operation: &str,
    duration: std::time::Duration,
    text_length: usize,
    line_count: usize,
    matches_found: usize,
) {
    let operation = operation.to_string();
    metrics::counter!("text_processing_operations_total", "operation" => operation.clone())
        .increment(1);
    metrics::histogram!("text_processing_duration_seconds", "operation" => operation.clone())
        .record(duration.as_secs_f64());
    metrics::histogram!("text_processing_input_length", "operation" => operation.clone())
        .record(text_length as f64);
    metrics::histogram!("text_processing_line_count", "operation" => operation.clone())
        .record(line_count as f64);
    metrics::histogram!("text_processing_matches_found", "operation" => operation.clone())
        .record(matches_found as f64);

    // Throughput metrics (characters processed per second)
    let throughput = if duration.as_secs_f64() > 0.0 {
        text_length as f64 / duration.as_secs_f64()
    } else {
        0.0
    };
    metrics::histogram!("text_processing_throughput_chars_per_sec", "operation" => operation)
        .record(throughput);
}

/// Record UI interaction metrics
pub fn record_ui_metrics(
    operation: &str,
    duration: std::time::Duration,
    element_count: usize,
    ui_elements_created: usize,
) {
    let operation = operation.to_string();
    metrics::counter!("ui_operations_total", "operation" => operation.clone()).increment(1);
    metrics::histogram!("ui_operation_duration_seconds", "operation" => operation.clone())
        .record(duration.as_secs_f64());
    metrics::histogram!("ui_element_count", "operation" => operation.clone())
        .record(element_count as f64);
    metrics::histogram!("ui_elements_created", "operation" => operation.clone())
        .record(ui_elements_created as f64);
}

/// Record recipe processing business metrics
pub fn record_recipe_metrics(
    recipe_name: &str,
    ingredient_count: usize,
    naming_method: RecipeNamingMethod,
    processing_duration: std::time::Duration,
    user_id: i64,
) {
    let recipe_name = recipe_name.to_string();

    // Basic recipe metrics
    metrics::counter!("recipes_created_total").increment(1);
    metrics::histogram!("recipe_ingredients_count").record(ingredient_count as f64);
    metrics::histogram!("recipe_processing_duration_seconds")
        .record(processing_duration.as_secs_f64());

    // Recipe naming method metrics
    let naming_method_str = match naming_method {
        RecipeNamingMethod::Caption => "caption",
        RecipeNamingMethod::Manual => "manual",
        RecipeNamingMethod::Default => "default",
    };
    metrics::counter!("recipe_naming_method_total", "method" => naming_method_str.to_string())
        .increment(1);

    // User engagement metrics
    metrics::counter!("user_recipe_creations_total").increment(1);

    // Recipe name length distribution
    metrics::histogram!("recipe_name_length").record(recipe_name.len() as f64);

    tracing::info!(
        user_id = %user_id,
        recipe_name = %recipe_name,
        ingredient_count = %ingredient_count,
        naming_method = %naming_method_str,
        processing_duration_ms = %processing_duration.as_millis(),
        "Recipe created successfully"
    );
}

/// Recipe naming method enumeration
#[derive(Debug, Clone, Copy)]
pub enum RecipeNamingMethod {
    /// Recipe named using photo caption
    Caption,
    /// Recipe named manually by user
    Manual,
    /// Recipe used default name ("Recipe")
    Default,
}

/// Record user engagement business metrics
pub fn record_user_engagement_metrics(
    user_id: i64,
    action: UserAction,
    session_duration: Option<std::time::Duration>,
    language_code: Option<&str>,
) {
    // Basic user action metrics
    let action_str = match action {
        UserAction::StartCommand => "start_command",
        UserAction::HelpCommand => "help_command",
        UserAction::RecipesCommand => "recipes_command",
        UserAction::PhotoUpload => "photo_upload",
        UserAction::DocumentUpload => "document_upload",
        UserAction::IngredientEdit => "ingredient_edit",
        UserAction::IngredientDelete => "ingredient_delete",
        UserAction::RecipeConfirm => "recipe_confirm",
        UserAction::RecipeSearch => "recipe_search",
        UserAction::WorkflowContinue => "workflow_continue",
    };
    metrics::counter!("user_actions_total", "action" => action_str.to_string()).increment(1);

    // Language usage metrics
    if let Some(lang) = language_code {
        metrics::counter!("user_language_usage_total", "language" => lang.to_string()).increment(1);
    }

    // Session duration tracking (when available)
    if let Some(duration) = session_duration {
        metrics::histogram!("user_session_duration_seconds").record(duration.as_secs_f64());
    }

    // Daily active users (simplified - would need proper time windowing in production)
    metrics::counter!("daily_active_users").increment(1);

    tracing::debug!(
        user_id = %user_id,
        action = %action_str,
        language_code = ?language_code,
        session_duration_secs = ?session_duration.map(|d| d.as_secs()),
        "User engagement recorded"
    );
}

/// User action enumeration for engagement tracking
#[derive(Debug, Clone, Copy)]
pub enum UserAction {
    /// User sent /start command
    StartCommand,
    /// User sent /help command
    HelpCommand,
    /// User sent /recipes command
    RecipesCommand,
    /// User uploaded a photo
    PhotoUpload,
    /// User uploaded a document
    DocumentUpload,
    /// User edited an ingredient
    IngredientEdit,
    /// User deleted an ingredient
    IngredientDelete,
    /// User confirmed recipe creation
    RecipeConfirm,
    /// User searched for recipes
    RecipeSearch,
    /// User continued workflow (add another, list recipes, etc.)
    WorkflowContinue,
}

/// Record dialogue completion and abandonment metrics
pub fn record_dialogue_metrics(
    user_id: i64,
    dialogue_type: DialogueType,
    completed: bool,
    step_count: usize,
    duration: std::time::Duration,
) {
    let dialogue_type_str = match dialogue_type {
        DialogueType::RecipeCreation => "recipe_creation",
        DialogueType::IngredientReview => "ingredient_review",
        DialogueType::RecipeNaming => "recipe_naming",
    };

    // Completion rate metrics
    metrics::counter!("dialogue_started_total", "type" => dialogue_type_str.to_string())
        .increment(1);
    if completed {
        metrics::counter!("dialogue_completed_total", "type" => dialogue_type_str.to_string())
            .increment(1);
    } else {
        metrics::counter!("dialogue_abandoned_total", "type" => dialogue_type_str.to_string())
            .increment(1);
    }

    // Dialogue performance metrics
    metrics::histogram!("dialogue_step_count", "type" => dialogue_type_str.to_string())
        .record(step_count as f64);
    metrics::histogram!("dialogue_duration_seconds", "type" => dialogue_type_str.to_string())
        .record(duration.as_secs_f64());

    // Calculate completion rate (rolling average would be better in production)
    let completion_rate = if completed { 1.0 } else { 0.0 };
    metrics::histogram!("dialogue_completion_rate", "type" => dialogue_type_str.to_string())
        .record(completion_rate);

    tracing::info!(
        user_id = %user_id,
        dialogue_type = %dialogue_type_str,
        completed = %completed,
        step_count = %step_count,
        duration_secs = %duration.as_secs(),
        "Dialogue metrics recorded"
    );
}

/// Dialogue type enumeration
#[derive(Debug, Clone, Copy)]
pub enum DialogueType {
    /// Full recipe creation workflow
    RecipeCreation,
    /// Ingredient review and editing phase
    IngredientReview,
    /// Recipe naming phase
    RecipeNaming,
}

/// Record business KPI metrics
pub fn record_business_kpi_metrics() {
    // These would be calculated periodically from stored data
    // For now, we record them as gauges that can be updated by background tasks

    // OCR success rate (updated by background monitoring)
    metrics::gauge!("ocr_success_rate_percent").set(0.0); // Placeholder

    // Average recipe processing time (updated by background monitoring)
    metrics::gauge!("avg_recipe_processing_time_seconds").set(0.0); // Placeholder

    // User retention rate (updated by background monitoring)
    metrics::gauge!("user_retention_rate_percent").set(0.0); // Placeholder

    // Feature adoption rates
    metrics::gauge!("caption_naming_adoption_percent").set(0.0); // Placeholder
    metrics::gauge!("ingredient_editing_usage_percent").set(0.0); // Placeholder
}

/// Record recipe search and discovery metrics
pub fn record_recipe_discovery_metrics(
    user_id: i64,
    search_query: Option<&str>,
    result_count: usize,
    search_duration: std::time::Duration,
) {
    metrics::counter!("recipe_searches_total").increment(1);
    metrics::histogram!("recipe_search_result_count").record(result_count as f64);
    metrics::histogram!("recipe_search_duration_seconds").record(search_duration.as_secs_f64());

    if let Some(query) = search_query {
        // Search query length and complexity metrics
        metrics::histogram!("recipe_search_query_length").record(query.len() as f64);

        // Check if it's a simple or complex search
        let is_complex = query.contains(' ') || query.len() > 20;
        metrics::counter!("recipe_search_complexity_total", "complex" => is_complex.to_string())
            .increment(1);
    }

    tracing::info!(
        user_id = %user_id,
        search_query = ?search_query,
        result_count = %result_count,
        search_duration_ms = %search_duration.as_millis(),
        "Recipe discovery metrics recorded"
    );
}

/// Record user retention and cohort metrics
pub fn record_user_retention_metrics(
    user_id: i64,
    days_since_first_use: u32,
    recipes_created: u32,
    is_returning: bool,
) {
    // User retention buckets
    let retention_bucket = match days_since_first_use {
        0..=1 => "day_1",
        2..=7 => "week_1",
        8..=30 => "month_1",
        31..=90 => "quarter_1",
        _ => "long_term",
    };

    metrics::counter!("user_retention_total", "bucket" => retention_bucket.to_string())
        .increment(1);

    if is_returning {
        metrics::counter!("returning_users_total").increment(1);
    }

    // User engagement level based on recipes created
    let engagement_level = match recipes_created {
        0 => "newcomer",
        1..=5 => "casual",
        6..=20 => "regular",
        21..=50 => "power",
        _ => "expert",
    };

    metrics::counter!("user_engagement_level_total", "level" => engagement_level.to_string())
        .increment(1);

    tracing::debug!(
        user_id = %user_id,
        days_since_first_use = %days_since_first_use,
        recipes_created = %recipes_created,
        is_returning = %is_returning,
        retention_bucket = %retention_bucket,
        engagement_level = %engagement_level,
        "User retention metrics recorded"
    );
}

/// Record feature usage analytics
pub fn record_feature_usage_metrics(user_id: i64, feature: FeatureType, usage_count: u32) {
    let feature_str = match feature {
        FeatureType::PhotoCaptionNaming => "photo_caption_naming",
        FeatureType::IngredientEditing => "ingredient_editing",
        FeatureType::RecipeSearch => "recipe_search",
        FeatureType::MultiLanguage => "multi_language",
        FeatureType::WorkflowButtons => "workflow_buttons",
    };

    metrics::counter!("feature_usage_total", "feature" => feature_str.to_string()).increment(1);
    metrics::histogram!("feature_usage_count_per_user", "feature" => feature_str.to_string())
        .record(usage_count as f64);

    tracing::debug!(
        user_id = %user_id,
        feature = %feature_str,
        usage_count = %usage_count,
        "Feature usage metrics recorded"
    );
}

/// Feature type enumeration for usage tracking
#[derive(Debug, Clone, Copy)]
pub enum FeatureType {
    /// Photo caption used for recipe naming
    PhotoCaptionNaming,
    /// Ingredient editing functionality
    IngredientEditing,
    /// Recipe search functionality
    RecipeSearch,
    /// Multi-language support usage
    MultiLanguage,
    /// Workflow continuation buttons
    WorkflowButtons,
}

/// Record mutex poisoning incidents for monitoring critical system health
pub fn record_mutex_poisoning(component: &str, operation: &str) {
    metrics::counter!("mutex_poisoning_total", "component" => component.to_string(), "operation" => operation.to_string())
        .increment(1);

    tracing::error!(
        component = %component,
        operation = %operation,
        "Mutex poisoning detected - this indicates a critical system error requiring investigation"
    );
}

/// Record build time metrics for delivery performance tracking
pub fn record_build_time(duration: std::time::Duration) {
    metrics::histogram!("cargo_build_time_seconds").record(duration.as_secs_f64());
}

/// Record deployment events for velocity tracking
pub fn record_deployment() {
    metrics::counter!("deployments_total").increment(1);
}

/// Set AI velocity gain metric (e.g., percentage improvement)
pub fn set_ai_velocity_gain(gain: f64) {
    metrics::gauge!("ai_velocity_gain").set(gain);
}

/// Set classical development velocity baseline
pub fn set_classical_velocity_baseline(baseline: f64) {
    metrics::gauge!("classical_velocity_baseline").set(baseline);
}

/// Record bug fixes for quality tracking
pub fn record_bug_fix() {
    metrics::counter!("bugs_fixed_total").increment(1);
}
