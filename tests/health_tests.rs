//! What the public health server answers, written down before it moves to another HTTP stack
//! (`specs/tasks/engine-api.md`, tranche 1): the same requests must get the same answers after.
//! Each test starts its own server, so each has its own rate limiter.

use just_ingredients::observability::server;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The public server, without a database unless one is given.
async fn health_server(db_pool: Option<Arc<sqlx::PgPool>>) -> anyhow::Result<SocketAddr> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let addresses = server::start(recorder.handle(), 0, 0, false, db_pool).await?;
    Ok(addresses.health)
}

/// A request, with the client address Fly's proxy would set when given.
async fn call(
    address: SocketAddr,
    method: reqwest::Method,
    path: &str,
    fly_client_ip: Option<&str>,
) -> anyhow::Result<(u16, String)> {
    let client = reqwest::Client::new();
    let mut request = client.request(method, format!("http://{address}{path}"));
    if let Some(ip) = fly_client_ip {
        request = request.header("Fly-Client-IP", ip);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    Ok((status, response.text().await?))
}

async fn get(address: SocketAddr, path: &str) -> anyhow::Result<(u16, String)> {
    call(address, reqwest::Method::GET, path, None).await
}

/// `/health/live` is not counted (Fly calls it every 15 s, twice); every other request is,
/// unknown paths included: the 11th of a minute is refused, before routing.
#[tokio::test]
async fn liveness_is_free_and_other_requests_are_limited_to_10_a_minute() -> anyhow::Result<()> {
    let address = health_server(None).await?;
    for _ in 0..15 {
        assert_eq!(get(address, "/health/live").await?, (200, "OK".to_string()));
    }
    for _ in 0..9 {
        assert_eq!(
            get(address, "/health/ready").await?,
            (200, "OK".to_string())
        );
    }
    assert_eq!(
        get(address, "/nothing").await?,
        (404, "Not Found".to_string()),
        "the 10th, an unknown path, counts"
    );
    assert_eq!(
        get(address, "/health/ready").await?,
        (429, "Rate limit exceeded".to_string())
    );
    assert_eq!(
        get(address, "/nothing").await?,
        (429, "Rate limit exceeded".to_string()),
        "refused before routing"
    );
    assert_eq!(
        get(address, "/health/live").await?,
        (200, "OK".to_string()),
        "liveness still answers"
    );
    Ok(())
}

/// Clients are told apart by `Fly-Client-IP` (Fly's proxy is the TCP peer of every request); a
/// value that is not an address counts for the TCP peer.
#[tokio::test]
async fn the_limit_is_per_client() -> anyhow::Result<()> {
    let address = health_server(None).await?;
    let ready = |ip| call(address, reqwest::Method::GET, "/health/ready", ip);
    for _ in 0..10 {
        assert_eq!(ready(Some("203.0.113.1")).await?.0, 200);
    }
    assert_eq!(ready(Some("203.0.113.1")).await?.0, 429);
    assert_eq!(ready(Some("203.0.113.2")).await?.0, 200, "another client");
    assert_eq!(ready(Some(" 2001:db8::7 ")).await?.0, 200, "trimmed, IPv6");
    for _ in 0..10 {
        assert_eq!(ready(Some("not an address")).await?.0, 200);
    }
    assert_eq!(
        ready(None).await?.0,
        429,
        "the bad header counted for the peer"
    );
    Ok(())
}

/// Only GET on the two health paths answers; anything else is "Not Found", a wrong method too.
#[tokio::test]
async fn other_paths_and_methods_are_not_found() -> anyhow::Result<()> {
    let address = health_server(None).await?;
    for (method, path) in [
        (reqwest::Method::GET, "/"),
        (reqwest::Method::GET, "/metrics"),
        (reqwest::Method::GET, "/health"),
        (reqwest::Method::GET, "/health/live/"),
        (reqwest::Method::POST, "/health/live"),
        (reqwest::Method::PUT, "/health/ready"),
        (reqwest::Method::DELETE, "/health/ready"),
    ] {
        assert_eq!(
            call(address, method.clone(), path, None).await?,
            (404, "Not Found".to_string()),
            "{method} {path}"
        );
    }
    Ok(())
}

/// The first line of the answer to a request announcing a body of `length` bytes, sent as
/// headers only: the server answers from them, before any body.
async fn status_line_for_length(address: SocketAddr, length: usize) -> anyhow::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(
            format!(
                "POST /health/ready HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    let mut answer = vec![0_u8; 512];
    let read = stream.read(&mut answer).await?;
    Ok(String::from_utf8_lossy(&answer[..read]).into_owned())
}

/// A request announcing a body over 1 MB is refused before it is read; 1 MB is let through
/// (then not found: not a GET).
#[tokio::test]
async fn a_large_request_is_refused() -> anyhow::Result<()> {
    let address = health_server(None).await?;
    let over = status_line_for_length(address, 2_000_000).await?;
    assert!(over.starts_with("HTTP/1.1 413"), "{over}");
    assert!(over.ends_with("Request too large"), "{over}");
    let limit = status_line_for_length(address, 1024 * 1024).await?;
    assert!(limit.starts_with("HTTP/1.1 404"), "{limit}");
    Ok(())
}

/// Readiness checks the database: unreachable, the answer is 503 without the reason.
#[tokio::test]
async fn readiness_fails_without_the_database() -> anyhow::Result<()> {
    let unreachable = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")?;
    let address = health_server(Some(Arc::new(unreachable))).await?;
    assert_eq!(
        get(address, "/health/ready").await?,
        (503, "NOT READY".to_string())
    );
    assert_eq!(
        get(address, "/health/live").await?,
        (200, "OK".to_string()),
        "liveness does not read the database"
    );
    Ok(())
}
