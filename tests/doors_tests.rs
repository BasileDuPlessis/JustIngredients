//! The guard of the doors other than Telegram (`specs/tasks/engine-api.md`, §7), over a real
//! socket and a throwaway database: a test route behind it, each step tried.
//!
//! They need Postgres: `cargo test --test doors_tests -- --include-ignored` with `DATABASE_URL`
//! set to a throwaway database.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use axum::extract::Extension;
use axum::http::HeaderMap;
use axum::routing::get;
use just_ingredients::db::init_database_schema;
use just_ingredients::doors::guard::{Guard, Limits, guarded};
use just_ingredients::engine::Owner;
use just_ingredients::engine::tokens::{self, Authenticated, Realm, SCOPE_RECIPES_READ};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const OWNER: i64 = 501;

/// Who the guard let through, and whether the token was still in the request.
async fn echo(Extension(authenticated): Extension<Authenticated>, headers: HeaderMap) -> String {
    format!(
        "{:?} authorization={}",
        authenticated.owner,
        headers.contains_key(axum::http::header::AUTHORIZATION)
    )
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(500)).await;
    "done"
}

/// The test routes behind a guard with `limits`, served on a free port.
async fn serve(pool: &PgPool, limits: Limits) -> Result<SocketAddr> {
    let guard = Guard::new(
        pool.clone(),
        Realm::Production,
        "test",
        SCOPE_RECIPES_READ,
        limits,
    );
    let app = guarded(
        axum::Router::new()
            .route("/echo", get(echo))
            .route("/slow", get(slow)),
        guard,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok(address)
}

/// A GET with an optional `Authorization` value and client address; the status and the body.
async fn call(
    address: SocketAddr,
    path: &str,
    authorization: Option<&str>,
    from: Option<&str>,
) -> Result<(u16, String)> {
    let mut request = client()?.get(format!("http://{address}{path}"));
    if let Some(value) = authorization {
        request = request.header("Authorization", value);
    }
    if let Some(ip) = from {
        request = request.header("Fly-Client-IP", ip);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    Ok((status, response.text().await?))
}

/// A client for the local server: no proxy lookup (on macOS, reading the system's proxy settings
/// made the first request of a test take seconds).
fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().no_proxy().build()?)
}

fn bearer(secret: &str) -> String {
    format!("Bearer {secret}")
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn only_an_active_token_of_the_realm_opens_the_door(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let address = serve(&pool, Limits::default()).await?;
    let token = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;

    // Let through, as its owner, without its Authorization header
    let (status, body) = call(address, "/echo", Some(&bearer(&token.secret)), None).await?;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("501"), "{body}");
    assert!(body.ends_with("authorization=false"), "{body}");
    assert_eq!(
        call(
            address,
            "/echo",
            Some(&format!("bearer {}", token.secret)),
            None
        )
        .await?
        .0,
        200,
        "the scheme in any case"
    );

    // Refused, with the scheme to use
    let staging = tokens::create(&pool, Owner::telegram(OWNER), Realm::Staging).await?;
    let unknown = format!("ji_{}", "A".repeat(43));
    for authorization in [
        None,
        Some("Basic dXNlcjpwYXNz".to_string()),
        Some("Bearer ".to_string()),
        Some(bearer("ji_short")),
        Some(bearer(&unknown)),
        Some(bearer(&staging.secret)),
        Some(token.secret.clone()),
    ] {
        let response = client()?.get(format!("http://{address}/echo"));
        let response = match &authorization {
            Some(value) => response.header("Authorization", value),
            None => response,
        }
        .header("Fly-Client-IP", "198.51.100.1")
        .send()
        .await?;
        assert_eq!(response.status().as_u16(), 401, "{authorization:?}");
        assert_eq!(
            response
                .headers()
                .get("www-authenticate")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer")
        );
    }

    // Revoked: refused at once
    tokens::revoke(&pool, Owner::telegram(OWNER), token.id).await?;
    assert_eq!(
        call(address, "/echo", Some(&bearer(&token.secret)), None)
            .await?
            .0,
        401
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_token_has_so_many_requests_a_minute(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let limits = Limits {
        per_token_per_minute: 3,
        ..Limits::default()
    };
    let address = serve(&pool, limits).await?;
    let first = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;
    let second = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;
    for _ in 0..3 {
        assert_eq!(
            call(address, "/echo", Some(&bearer(&first.secret)), None)
                .await?
                .0,
            200
        );
    }
    assert_eq!(
        call(address, "/echo", Some(&bearer(&first.secret)), None).await?,
        (429, "Too many requests".to_string())
    );
    assert_eq!(
        call(address, "/echo", Some(&bearer(&second.secret)), None)
            .await?
            .0,
        200,
        "another token has its own count"
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn an_address_that_keeps_failing_is_refused(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let limits = Limits {
        failures_per_address_per_minute: 2,
        ..Limits::default()
    };
    let address = serve(&pool, limits).await?;
    let token = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;
    let guesser = Some("203.0.113.5");
    let guess = format!("ji_{}", "B".repeat(43));
    for _ in 0..2 {
        assert_eq!(
            call(address, "/echo", Some(&bearer(&guess)), guesser)
                .await?
                .0,
            401
        );
    }
    assert_eq!(
        call(address, "/echo", Some(&bearer(&token.secret)), guesser).await?,
        (429, "Too many failed attempts".to_string()),
        "even a good token, from that address"
    );
    assert_eq!(
        call(
            address,
            "/echo",
            Some(&bearer(&token.secret)),
            Some("203.0.113.6")
        )
        .await?
        .0,
        200,
        "another address"
    );
    Ok(())
}

/// The first line of the answer to a request announcing a body of `length` bytes, sent as
/// headers only: the guard answers from them.
async fn status_line_for_length(
    address: SocketAddr,
    secret: &str,
    length: usize,
) -> Result<String> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(
            format!(
                "POST /echo HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {secret}\r\n\
                 Content-Length: {length}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    let mut answer = vec![0_u8; 512];
    let read = stream.read(&mut answer).await?;
    Ok(String::from_utf8_lossy(&answer[..read]).into_owned())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_large_request_is_refused_before_the_token(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let address = serve(&pool, Limits::default()).await?;
    let token = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;
    let answer = status_line_for_length(address, &token.secret, 64 * 1024 + 1).await?;
    assert!(answer.starts_with("HTTP/1.1 413"), "{answer}");
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn requests_at_once_are_capped_and_slow_ones_cut(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let token = tokens::create(&pool, Owner::telegram(OWNER), Realm::Production).await?;
    let authorization = bearer(&token.secret);

    // One place: a second request at the same time waits a little, then is told the door is busy
    let one_place = Limits {
        max_in_flight: 1,
        wait_for_place: Duration::from_millis(100),
        ..Limits::default()
    };
    let address = serve(&pool, one_place).await?;
    let (first, second) = tokio::join!(call(address, "/slow", Some(&authorization), None), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        call(address, "/slow", Some(&authorization), None).await
    });
    assert_eq!(first?, (200, "done".to_string()));
    assert_eq!(second?, (503, "Busy".to_string()));

    // A request longer than the limit is cut
    let short = Limits {
        timeout: Duration::from_millis(100),
        ..Limits::default()
    };
    let address = serve(&pool, short).await?;
    assert_eq!(
        call(address, "/slow", Some(&authorization), None).await?,
        (504, "Timeout".to_string())
    );
    Ok(())
}
