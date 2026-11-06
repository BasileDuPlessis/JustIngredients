//! What every request through a door passes before the engine (`specs/tasks/engine-api.md`,
//! §7), in this order:
//!
//! 1. an address with too many failed authentications in the last minute is refused (`429`),
//!    so tokens cannot be guessed;
//! 2. a request announcing a body over the limit is refused (`413`);
//! 3. the `Authorization: Bearer` token must be an active token of this realm allowing the
//!    door's scope (`401` with `WWW-Authenticate: Bearer`, counted as a failure of the address);
//! 4. at most so many requests a minute per token (`429`);
//! 5. at most so many requests at once for all doors (`503` after a short wait): the bot keeps
//!    its database connections;
//! 6. a request that takes too long is cut (`504`).
//!
//! The request then reaches the door with the [`Authenticated`] token in its extensions and
//! without its `Authorization` header. One log line per request: door, status, duration, token
//! id; never the path's query, the body nor the token.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::PgPool;
use tokio::sync::Semaphore;

use crate::engine::tokens::{self, Authenticated, Realm};
use crate::observability::server::{RateLimiter, client_key};

/// The limits of a door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub per_token_per_minute: u32,
    pub failures_per_address_per_minute: u32,
    pub max_in_flight: usize,
    /// How long a request waits for a free place before `503`.
    pub wait_for_place: Duration,
    pub timeout: Duration,
    pub max_body_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            per_token_per_minute: 60,
            failures_per_address_per_minute: 10,
            // The bot's pool has 10 connections
            max_in_flight: 4,
            wait_for_place: Duration::from_secs(2),
            timeout: Duration::from_secs(10),
            max_body_bytes: 64 * 1024,
        }
    }
}

/// What the guard needs, shared by its requests.
#[derive(Debug)]
pub struct Guard {
    pool: PgPool,
    realm: Realm,
    /// The name of the door, in logs and metrics (`"mcp"`).
    door: &'static str,
    /// What a token must allow (`tokens::SCOPE_RECIPES_READ`).
    scope: &'static str,
    limits: Limits,
    per_token: RateLimiter,
    failures: RateLimiter,
    in_flight: Arc<Semaphore>,
}

impl Guard {
    pub fn new(
        pool: PgPool,
        realm: Realm,
        door: &'static str,
        scope: &'static str,
        limits: Limits,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            realm,
            door,
            scope,
            per_token: RateLimiter::new(limits.per_token_per_minute, 60),
            failures: RateLimiter::new(limits.failures_per_address_per_minute, 60),
            in_flight: Arc::new(Semaphore::new(limits.max_in_flight)),
            limits,
        })
    }
}

/// `routes` behind the guard.
pub fn guarded(routes: Router, guard: Arc<Guard>) -> Router {
    routes.layer(axum::middleware::from_fn_with_state(guard, check))
}

async fn check(
    State(guard): State<Arc<Guard>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let (response, token_id) = pass(&guard, peer, request, next).await;
    let status = response.status().as_u16();
    crate::observability::api_request(guard.door, status, started.elapsed());
    tracing::info!(
        door = guard.door,
        status,
        duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        token_id,
        "Door request"
    );
    response
}

/// The steps of the module's documentation; returns the answer and the token's id once known.
async fn pass(
    guard: &Guard,
    peer: SocketAddr,
    mut request: Request,
    next: Next,
) -> (Response, Option<i64>) {
    let address = client_key(request.headers(), peer.ip());
    if !guard.failures.has_room(&address) {
        return (
            refusal(StatusCode::TOO_MANY_REQUESTS, "Too many failed attempts"),
            None,
        );
    }
    if announced_length(request.headers())
        .is_some_and(|length| length > guard.limits.max_body_bytes)
    {
        return (
            refusal(StatusCode::PAYLOAD_TOO_LARGE, "Request too large"),
            None,
        );
    }
    let authenticated = match bearer(request.headers()) {
        Some(presented) => tokens::authenticate(&guard.pool, guard.realm, presented).await,
        None => Ok(None),
    };
    let authenticated: Authenticated = match authenticated {
        Ok(Some(authenticated)) if authenticated.allows(guard.scope) => authenticated,
        Ok(_) => {
            guard.failures.is_allowed(&address);
            return (unauthorized(), None);
        }
        Err(error) => {
            tracing::warn!(error = %error, door = guard.door, "Could not check a token");
            return (
                refusal(StatusCode::SERVICE_UNAVAILABLE, "Unavailable"),
                None,
            );
        }
    };
    let token_id = Some(authenticated.token_id);
    if !guard
        .per_token
        .is_allowed(&authenticated.token_id.to_string())
    {
        return (
            refusal(StatusCode::TOO_MANY_REQUESTS, "Too many requests"),
            token_id,
        );
    }
    let place = tokio::time::timeout(
        guard.limits.wait_for_place,
        Arc::clone(&guard.in_flight).acquire_owned(),
    )
    .await;
    let Ok(Ok(_place)) = place else {
        return (refusal(StatusCode::SERVICE_UNAVAILABLE, "Busy"), token_id);
    };
    request.headers_mut().remove(header::AUTHORIZATION);
    request.extensions_mut().insert(authenticated);
    match tokio::time::timeout(guard.limits.timeout, next.run(request)).await {
        Ok(response) => (response, token_id),
        Err(_) => (refusal(StatusCode::GATEWAY_TIMEOUT, "Timeout"), token_id),
    }
}

/// The token of `Authorization: Bearer <token>` (the scheme in any case), if any.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

fn announced_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn refusal(status: StatusCode, text: &'static str) -> Response {
    (status, text).into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
        "Unauthorized",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(value) {
            headers.insert(header::AUTHORIZATION, value);
        }
        headers
    }

    #[test]
    fn the_token_is_read_from_a_bearer_header() {
        assert_eq!(bearer(&headers("Bearer ji_abc")), Some("ji_abc"));
        assert_eq!(bearer(&headers("bearer  ji_abc ")), Some("ji_abc"));
        assert_eq!(bearer(&headers("Basic dXNlcjpwYXNz")), None);
        assert_eq!(bearer(&headers("Bearer ")), None);
        assert_eq!(bearer(&headers("ji_abc")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }

    #[test]
    fn the_default_limits_keep_the_bots_connections() {
        let limits = Limits::default();
        assert!(
            limits.max_in_flight < 10,
            "the bot's pool has 10 connections"
        );
        assert_eq!(limits.per_token_per_minute, 60);
        assert_eq!(limits.max_body_bytes, 65_536);
    }
}
