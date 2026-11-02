//! The bot's two HTTP servers (plan O1, `specs/tasks/observability.md`):
//!
//! - the **public** one answers the health checks of Fly and of "Production Uptime"
//!   (`/health/live`, `/health/ready`), with a rate limit per client;
//! - the **internal** one serves the metrics (`/metrics`) that Fly collects every 15 seconds
//!   (`[metrics]` in `fly.toml`). Its port is no Fly service: the internet cannot reach it, only
//!   Fly's private network, so it needs no token.

use anyhow::Result;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::collections::HashMap;
use std::future::Future;
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

/// Where the two servers listen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Addresses {
    pub health: SocketAddr,
    pub metrics: SocketAddr,
}

/// What a health check request needs.
struct HealthState {
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
    rate_limiter: RateLimiter,
}

fn text_response(status: hyper::StatusCode, body: &str) -> hyper::Response<String> {
    let mut response = hyper::Response::new(body.to_string());
    *response.status_mut() = status;
    response
}

/// Starts both servers: on all interfaces with `bind_all` (on Fly), else on localhost. Port 0
/// picks a free port (tests); the addresses really used are returned.
pub async fn start(
    metrics_handle: PrometheusHandle,
    health_port: u16,
    metrics_port: u16,
    bind_all: bool,
    db_pool: Option<Arc<PgPool>>,
    bot_token: Option<String>,
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
        bot_token,
        // 10 requests per minute per client
        rate_limiter: RateLimiter::new(10, 60),
    });
    serve(health_listener, move |req, peer| {
        let health = Arc::clone(&health);
        async move { handle_health(req, peer, &health).await }
    });
    let metrics_handle = Arc::new(metrics_handle);
    serve(metrics_listener, move |req, _| {
        let metrics_handle = Arc::clone(&metrics_handle);
        async move { handle_metrics(&req, &metrics_handle) }
    });

    tracing::info!(
        health = %addresses.health,
        metrics = %addresses.metrics,
        "Health and metrics servers listening"
    );
    Ok(addresses)
}

/// Accepts connections on `listener` and answers each request with `handler`.
fn serve<H, F>(listener: TcpListener, handler: H)
where
    H: Fn(hyper::Request<hyper::body::Incoming>, IpAddr) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = hyper::Response<String>> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        let service = hyper::service::service_fn(move |req| {
                            let response = handler(req, peer.ip());
                            async move { Ok::<_, std::convert::Infallible>(response.await) }
                        });
                        if let Err(err) = http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await
                        {
                            crate::errors::error_logging::log_network_error(
                                &err,
                                "serve_http_connection",
                                Some(&peer.to_string()),
                                None,
                            );
                        }
                    });
                }
                Err(e) => crate::errors::error_logging::log_network_error(
                    &e,
                    "accept_tcp_connection",
                    None,
                    None,
                ),
            }
        }
    });
}

/// The public server: rate limit, size limit, then the health routes.
async fn handle_health(
    req: hyper::Request<hyper::body::Incoming>,
    peer: IpAddr,
    state: &HealthState,
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
    match (req.method(), req.uri().path()) {
        // Liveness: the process runs and can reach Telegram
        (&hyper::Method::GET, "/health/live") => {
            match super::health_checks::check_telegram_health() {
                Ok(()) => text_response(StatusCode::OK, "OK"),
                Err(_) => text_response(StatusCode::SERVICE_UNAVAILABLE, "NOT LIVE"),
            }
        }
        // Readiness: database and Telegram. The reason is logged, never sent to the caller.
        (&hyper::Method::GET, "/health/ready") => {
            match super::health_checks::perform_readiness_checks(
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

/// The internal server: the metrics in Prometheus text format, nothing else.
fn handle_metrics<B>(
    req: &hyper::Request<B>,
    handle: &PrometheusHandle,
) -> hyper::Response<String> {
    if req.method() != hyper::Method::GET || req.uri().path() != "/metrics" {
        return text_response(hyper::StatusCode::NOT_FOUND, "Not Found");
    }
    let mut response = hyper::Response::new(handle.render());
    response.headers_mut().insert(
        "content-type",
        hyper::header::HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)]) -> hyper::Request<()> {
        let mut builder = hyper::Request::builder().uri("/health/ready");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(()).unwrap_or_default()
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

    #[test]
    fn the_internal_server_serves_only_metrics() {
        let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder()
            .handle();
        let get = |path: &str| {
            hyper::Request::builder()
                .uri(path)
                .body(())
                .unwrap_or_default()
        };
        assert_eq!(handle_metrics(&get("/metrics"), &handle).status(), 200);
        assert_eq!(handle_metrics(&get("/health/live"), &handle).status(), 404);
        assert_eq!(handle_metrics(&get("/"), &handle).status(), 404);
    }
}
