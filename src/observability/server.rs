//! The bot's two HTTP servers (plan O1, `specs/tasks/observability.md`), on axum since
//! 2026-11-06 (`specs/tasks/engine-api.md`, tranche 1 task 4):
//!
//! - the **public** one answers the health checks of Fly and of "Production Uptime"
//!   (`/health/live`, `/health/ready`), with a rate limit per client; the MCP server will be
//!   mounted next to them, with limits of its own;
//! - the **internal** one serves the metrics (`/metrics`) that Fly collects every 15 seconds
//!   (`[metrics]` in `fly.toml`). Its port is no Fly service: the internet cannot reach it, only
//!   Fly's private network, so it needs no token.
//!
//! What the health routes answer is written down in `tests/health_tests.rs` (from before the move
//! to axum: the same requests get the same answers).

use anyhow::Result;
use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

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

    /// Whether `client` still has room in the window, without counting a request (a limit on
    /// failures checks before trying, and counts only a failure).
    pub fn has_room(&self, client: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);
        let requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        requests.get(client).is_none_or(|times| {
            times
                .iter()
                .filter(|time| now.duration_since(**time) < window)
                .count()
                < self.max_requests as usize
        })
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
pub fn client_key(headers: &HeaderMap, peer: IpAddr) -> String {
    headers
        .get("fly-client-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .unwrap_or(peer)
        .to_string()
}

/// Check request size limit
pub fn check_request_size(headers: &HeaderMap) -> bool {
    const MAX_REQUEST_SIZE: u64 = 1024 * 1024; // 1MB limit

    if let Some(content_length) = headers.get("content-length") {
        if let Ok(size_str) = content_length.to_str()
            && let Ok(size) = size_str.parse::<u64>()
        {
            return size <= MAX_REQUEST_SIZE;
        }
        return false; // Invalid content-length header
    }

    true // No content-length header (GET requests)
}

/// Where the two servers listen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Addresses {
    pub health: SocketAddr,
    pub metrics: SocketAddr,
}

/// What a health check request needs.
struct HealthState {
    db_pool: Option<Arc<PgPool>>,
    rate_limiter: RateLimiter,
}

fn text_response(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

/// Starts both servers: on all interfaces with `bind_all` (on Fly), else on localhost. Port 0
/// picks a free port (tests); the addresses really used are returned.
pub async fn start(
    metrics_handle: PrometheusHandle,
    health_port: u16,
    metrics_port: u16,
    bind_all: bool,
    db_pool: Option<Arc<PgPool>>,
) -> Result<Addresses> {
    start_with_doors(
        metrics_handle,
        health_port,
        metrics_port,
        bind_all,
        db_pool,
        Router::new(),
    )
    .await
}

/// [`start`], with `doors` (the MCP server, `crate::doors`) served by the public server next to
/// the health routes. Their paths do not reach the health handler nor its rate limit.
pub async fn start_with_doors(
    metrics_handle: PrometheusHandle,
    health_port: u16,
    metrics_port: u16,
    bind_all: bool,
    db_pool: Option<Arc<PgPool>>,
    doors: Router,
) -> Result<Addresses> {
    let ip = if bind_all {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    let health_listener = TcpListener::bind(SocketAddr::new(ip, health_port)).await?;
    let metrics_listener = TcpListener::bind(SocketAddr::new(ip, metrics_port)).await?;
    let addresses = Addresses {
        health: health_listener.local_addr()?,
        metrics: metrics_listener.local_addr()?,
    };

    let health = Arc::new(HealthState {
        db_pool,
        // 10 requests per minute per client
        rate_limiter: RateLimiter::new(10, 60),
    });
    serve(health_listener, public_router(health, doors), "public");
    serve(
        metrics_listener,
        Router::new()
            .fallback(metrics)
            .with_state(Arc::new(metrics_handle)),
        "internal",
    );

    tracing::info!(
        health = %addresses.health,
        metrics = %addresses.metrics,
        "Health and metrics servers listening"
    );
    Ok(addresses)
}

/// The public server's routes: the doors' own paths, and everything else to [`health`], which
/// answers the two health paths and "Not Found" for anything else, after its limits, exactly as
/// before axum.
fn public_router(health_state: Arc<HealthState>, doors: Router) -> Router {
    Router::new()
        .fallback(health)
        .with_state(health_state)
        .merge(doors)
}

/// Serves `router` on `listener` in the background, with the client's address for the rate
/// limit. A failure of the server is logged.
fn serve(listener: TcpListener, router: Router, name: &'static str) {
    tokio::spawn(async move {
        let service = router.into_make_service_with_connect_info::<SocketAddr>();
        if let Err(error) = axum::serve(listener, service).await {
            crate::errors::error_logging::log_network_error(&error, "serve_http", Some(name), None);
        }
    });
}

/// The public server: rate limit, size limit, then the health routes.
async fn health(
    State(state): State<Arc<HealthState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    // Fly's machine and service checks call /health/live every 15 s each; it does no I/O, so it
    // is not counted (they would use most of a client's 10 requests per minute)
    let counted = req.uri().path() != "/health/live";
    if counted
        && !state
            .rate_limiter
            .is_allowed(&client_key(req.headers(), peer.ip()))
    {
        return text_response(StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded");
    }
    if !check_request_size(req.headers()) {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request too large");
    }
    match (req.method(), req.uri().path()) {
        // Liveness: the process runs and can reach Telegram
        (&Method::GET, "/health/live") => match super::health_checks::check_telegram_health() {
            Ok(()) => text_response(StatusCode::OK, "OK"),
            Err(_) => text_response(StatusCode::SERVICE_UNAVAILABLE, "NOT LIVE"),
        },
        // Readiness: database and Telegram. The reason is logged, never sent to the caller.
        (&Method::GET, "/health/ready") => {
            match super::health_checks::perform_readiness_checks(state.db_pool.clone()).await {
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

/// The internal server: the metrics in Prometheus text format, nothing else.
async fn metrics(State(handle): State<Arc<PrometheusHandle>>, req: Request) -> Response {
    metrics_response(req.method(), req.uri().path(), &handle)
}

fn metrics_response(method: &Method, path: &str, handle: &PrometheusHandle) -> Response {
    if method != Method::GET || path != "/metrics" {
        return text_response(StatusCode::NOT_FOUND, "Not Found");
    }
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        handle.render(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            if let (Ok(name), Ok(value)) = (
                header::HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                map.insert(name, value);
            }
        }
        map
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
    fn room_is_looked_at_without_counting() {
        let limiter = RateLimiter::new(2, 60);
        assert!(limiter.has_room("a"));
        assert!(limiter.has_room("a"), "looking does not count");
        assert!(limiter.is_allowed("a"));
        assert!(limiter.has_room("a"));
        assert!(limiter.is_allowed("a"));
        assert!(!limiter.has_room("a"));
        assert!(limiter.has_room("b"));
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

    #[test]
    fn the_internal_server_serves_only_metrics() {
        let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder()
            .handle();
        let status = |method: Method, path: &str| metrics_response(&method, path, &handle).status();
        assert_eq!(status(Method::GET, "/metrics"), StatusCode::OK);
        assert_eq!(status(Method::POST, "/metrics"), StatusCode::NOT_FOUND);
        assert_eq!(status(Method::GET, "/health/live"), StatusCode::NOT_FOUND);
        assert_eq!(status(Method::GET, "/"), StatusCode::NOT_FOUND);
    }
}
