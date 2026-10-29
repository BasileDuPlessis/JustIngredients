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
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

use sha2::{Digest, Sha256};

use crate::observability_config::ObservabilityConfig;
use crate::secret::SecretString;

/// Distinct clients the rate limiter remembers; beyond, the least recently seen is forgotten.
pub const MAX_TRACKED_CLIENTS: usize = 1024;

/// Per-client rate limiter for the HTTP server, bounded in memory.
#[derive(Debug)]
pub struct RateLimiter {
    requests: Mutex<HashMap<String, Vec<Instant>>>,
    max_requests: u32,
    window_secs: u64,
    max_clients: usize,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            max_requests,
            window_secs,
            max_clients: MAX_TRACKED_CLIENTS,
        }
    }

    /// Replaces the number of clients remembered.
    pub fn with_max_clients(mut self, max_clients: usize) -> Self {
        self.max_clients = max_clients.max(1);
        self
    }

    /// Check if request is allowed for the given client
    pub fn is_allowed(&self, client: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);
        let recent = |time: &Instant| now.duration_since(*time) < window;

        // Plain counters: a panic while holding the lock cannot leave them inconsistent.
        let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        if !requests.contains_key(client) && requests.len() >= self.max_clients {
            // Forget clients without a request in the window, then the least recently seen
            requests.retain(|_, times| times.last().is_some_and(recent));
            if requests.len() >= self.max_clients {
                let oldest = requests
                    .iter()
                    .min_by_key(|(_, times)| times.last().copied())
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    requests.remove(&oldest);
                }
            }
        }
        let client_requests = requests.entry(client.to_string()).or_default();

        // Remove old requests outside the window
        client_requests.retain(recent);

        // Check if under limit
        if client_requests.len() >= self.max_requests as usize {
            return false;
        }

        // Add current request
        client_requests.push(now);
        true
    }

    /// Number of clients currently remembered.
    pub fn tracked_clients(&self) -> usize {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// The client a request counts for: `Fly-Client-IP` (set by Fly's proxy, which is the TCP peer
/// of every request from the internet) when present, else the TCP peer.
pub fn client_key<B>(req: &hyper::Request<B>, peer: IpAddr) -> String {
    req.headers()
        .get("fly-client-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .unwrap_or(peer)
        .to_string()
}

/// Paths that require `METRICS_AUTH_TOKEN`. Health checks stay open: Fly and the uptime monitor
/// call them without a token.
pub fn is_protected_path(path: &str) -> bool {
    path == "/metrics"
}

/// Whether the request carries `Authorization: Bearer <expected>`. Without a configured token
/// nothing is authorized, so `/metrics` stays closed.
pub fn check_auth<B>(req: &hyper::Request<B>, expected: Option<&SecretString>) -> bool {
    let Some(expected) = expected.filter(|token| !token.is_empty()) else {
        return false;
    };
    req.headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| tokens_match(token, expected.expose()))
}

/// Compares two tokens in constant time; comparing SHA-256 digests hides the length too.
fn tokens_match(given: &str, expected: &str) -> bool {
    let given = Sha256::digest(given.as_bytes());
    let expected = Sha256::digest(expected.as_bytes());
    given
        .iter()
        .zip(expected.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

/// Check request size limit
pub fn check_request_size<B>(req: &hyper::Request<B>) -> bool {
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

/// Start the HTTP server without dependencies: readiness only checks what it is given.
pub async fn start_metrics_server_basic_with_config(
    metrics_handle: PrometheusHandle,
    port: u16,
) -> Result<()> {
    start_metrics_server_with_health_checks(metrics_handle, port, None, None).await
}

/// What every request of the HTTP server needs.
struct ServerState {
    metrics_handle: PrometheusHandle,
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
    rate_limiter: RateLimiter,
    metrics_token: Option<SecretString>,
}

fn text_response(status: hyper::StatusCode, body: &str) -> hyper::Response<String> {
    let mut response = hyper::Response::new(body.to_string());
    *response.status_mut() = status;
    response
}

/// Answers one request: rate limit, size limit, token for `/metrics`, then the route.
async fn handle_request(
    req: hyper::Request<hyper::body::Incoming>,
    peer: IpAddr,
    state: Arc<ServerState>,
) -> hyper::Response<String> {
    use hyper::StatusCode;

    // Fly's machine and service checks call /health/live every 15 s each; it does no I/O, so it
    // is not counted (they would use most of a client's 10 requests per minute)
    let counted = req.uri().path() != "/health/live";
    if counted && !state.rate_limiter.is_allowed(&client_key(&req, peer)) {
        return text_response(StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded");
    }
    if !check_request_size(&req) {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request too large");
    }
    if is_protected_path(req.uri().path()) && !check_auth(&req, state.metrics_token.as_ref()) {
        let mut response = text_response(StatusCode::UNAUTHORIZED, "Unauthorized");
        response.headers_mut().insert(
            "www-authenticate",
            hyper::header::HeaderValue::from_static("Bearer"),
        );
        return response;
    }

    match (req.method(), req.uri().path()) {
        (&hyper::Method::GET, "/metrics") => {
            // Ensure at least one metric is registered to avoid empty render
            metrics::gauge!("uptime_seconds").set(1.0);
            let mut response = hyper::Response::new(state.metrics_handle.render());
            response.headers_mut().insert(
                "content-type",
                hyper::header::HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
            );
            response
        }
        // Liveness: the process runs and can reach Telegram
        (&hyper::Method::GET, "/health/live") => {
            match crate::observability::health_checks::check_telegram_health() {
                Ok(()) => text_response(StatusCode::OK, "OK"),
                Err(_) => text_response(StatusCode::SERVICE_UNAVAILABLE, "NOT LIVE"),
            }
        }
        // Readiness: database and Telegram. The reason is logged, never sent to the caller.
        (&hyper::Method::GET, "/health/ready") => {
            match crate::observability::health_checks::perform_readiness_checks(
                state.db_pool.clone(),
                state.bot_token.clone(),
            )
            .await
            {
                Ok(_) => text_response(StatusCode::OK, "OK"),
                Err(error) => {
                    tracing::warn!(error = %error, "Readiness check failed");
                    text_response(StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
                }
            }
        }
        _ => text_response(StatusCode::NOT_FOUND, "Not Found"),
    }
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

    let metrics_token = std::env::var("METRICS_AUTH_TOKEN")
        .ok()
        .map(|token| SecretString::new(token.trim()))
        .filter(|token| !token.is_empty());
    if metrics_token.is_none() {
        tracing::warn!("METRICS_AUTH_TOKEN is not set: /metrics answers 401");
    }
    let state = Arc::new(ServerState {
        metrics_handle,
        db_pool,
        bot_token,
        // 10 requests per minute per client
        rate_limiter: RateLimiter::new(10, 60),
        metrics_token,
    });

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Metrics server listening on {}", addr);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let state = Arc::clone(&state);
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        let service = hyper::service::service_fn(move |req| {
                            let state = Arc::clone(&state);
                            async move {
                                Ok::<_, std::convert::Infallible>(
                                    handle_request(req, peer_addr.ip(), state).await,
                                )
                            }
                        });

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

/// Records one image sent to the extraction service: outcome (`success` or an
/// `ExtractionError::kind`), latency including retries, and retries.
pub fn record_llm_request(
    model: &str,
    outcome: &'static str,
    duration: std::time::Duration,
    retries: u32,
) {
    metrics::counter!("llm_requests_total", "model" => model.to_string(), "outcome" => outcome)
        .increment(1);
    metrics::histogram!("llm_request_duration_seconds", "model" => model.to_string())
        .record(duration.as_secs_f64());
    if retries > 0 {
        metrics::counter!("llm_retries_total", "model" => model.to_string())
            .increment(u64::from(retries));
    }
}

/// Records the tokens billed for one successful call and, when the model price is known, its
/// estimated cost in micro-USD (`llm_estimated_cost_micro_usd_total / 1e6` is in USD).
pub fn record_llm_usage(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cost_micro_usd: Option<u64>,
) {
    metrics::counter!("llm_tokens_total", "model" => model.to_string(), "kind" => "input")
        .increment(input_tokens);
    metrics::counter!("llm_tokens_total", "model" => model.to_string(), "kind" => "output")
        .increment(output_tokens);
    if let Some(cost) = cost_micro_usd {
        metrics::counter!("llm_estimated_cost_micro_usd_total", "model" => model.to_string())
            .increment(cost);
    }
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

    tracing::debug!(
        user_id = %user_id,
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

    tracing::debug!(
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

    tracing::debug!(
        user_id = %user_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};

    #[test]
    fn only_metrics_requires_the_token() {
        assert!(is_protected_path("/metrics"));
        for open in ["/health/live", "/health/ready", "/"] {
            assert!(!is_protected_path(open), "{open}");
        }
    }

    fn request(headers: &[(&str, &str)]) -> hyper::Request<()> {
        let mut builder = hyper::Request::builder().uri("/metrics");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(()).unwrap_or_default()
    }

    #[test]
    fn metrics_need_the_configured_token() {
        let token = SecretString::new("s3cr3t");
        let bearer = [("authorization", "Bearer s3cr3t")];
        assert!(check_auth(&request(&bearer), Some(&token)));
        assert!(!check_auth(
            &request(&[("authorization", "Bearer wrong")]),
            Some(&token)
        ));
        assert!(!check_auth(&request(&[]), Some(&token)));
        assert!(
            !check_auth(&request(&bearer), None),
            "no token configured: closed"
        );
    }

    #[test]
    fn clients_are_told_apart_by_fly_client_ip() {
        let proxy: IpAddr = "172.16.0.1"
            .parse()
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(
            client_key(&request(&[("fly-client-ip", "203.0.113.7")]), proxy),
            "203.0.113.7"
        );
        assert_eq!(client_key(&request(&[]), proxy), "172.16.0.1");
        assert_eq!(
            client_key(&request(&[("fly-client-ip", "not an ip")]), proxy),
            "172.16.0.1"
        );
    }

    #[test]
    fn the_rate_limiter_limits_each_client_and_stays_bounded() {
        let limiter = RateLimiter::new(2, 60).with_max_clients(3);
        assert!(limiter.is_allowed("a"));
        assert!(limiter.is_allowed("a"));
        assert!(!limiter.is_allowed("a"), "third request in the window");
        assert!(
            limiter.is_allowed("b"),
            "other clients have their own count"
        );
        for client in ["c", "d", "e"] {
            assert!(limiter.is_allowed(client));
        }
        assert_eq!(limiter.tracked_clients(), 3);
    }

    #[test]
    fn expired_clients_are_forgotten_first() {
        let limiter = RateLimiter::new(1, 0).with_max_clients(2);
        assert!(limiter.is_allowed("a"));
        assert!(limiter.is_allowed("b"));
        assert!(limiter.is_allowed("c"));
        assert_eq!(
            limiter.tracked_clients(),
            1,
            "a and b had no request left in the window"
        );
    }

    /// Metric name, labels and value from the debugging recorder.
    type Sample = (String, Vec<(String, String)>, DebugValue);

    #[test]
    fn llm_metrics_are_recorded_with_model_and_outcome() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            record_llm_request(
                "gemini-test",
                "success",
                std::time::Duration::from_millis(1500),
                2,
            );
            record_llm_usage("gemini-test", 1561, 492, Some(1698));
        });

        let values: Vec<Sample> = snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, value)| {
                let labels = key
                    .key()
                    .labels()
                    .map(|label| (label.key().to_string(), label.value().to_string()))
                    .collect();
                (key.key().name().to_string(), labels, value)
            })
            .collect();
        let counter = |name: &str, label: (&str, &str)| {
            values.iter().find_map(|(n, labels, value)| match value {
                DebugValue::Counter(count)
                    if n == name && labels.iter().any(|(k, v)| k == label.0 && v == label.1) =>
                {
                    Some(*count)
                }
                _ => None,
            })
        };
        assert_eq!(
            counter("llm_requests_total", ("outcome", "success")),
            Some(1)
        );
        assert_eq!(
            counter("llm_retries_total", ("model", "gemini-test")),
            Some(2)
        );
        assert_eq!(counter("llm_tokens_total", ("kind", "input")), Some(1561));
        assert_eq!(counter("llm_tokens_total", ("kind", "output")), Some(492));
        assert_eq!(
            counter(
                "llm_estimated_cost_micro_usd_total",
                ("model", "gemini-test")
            ),
            Some(1698)
        );
    }
}
