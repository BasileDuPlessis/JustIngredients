//! # Database Connection
//!
//! Creates the PostgreSQL pool at startup. While the database is unreachable (for example
//! during a Postgres restart) the connection is retried with exponential backoff instead of
//! exiting, so the bot comes back on its own once the database is up.

use std::future::Future;
use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use tracing::{info, warn};

use crate::config::DatabaseConfig;
use crate::retry::{retry_with_backoff, Backoff, RetryDecision};

/// Retry schedule while the database is unreachable at startup: 1 s doubling up to 30 s,
/// for at most 10 minutes (a Postgres restart on Fly takes a few minutes).
pub const DEFAULT_CONNECT_BACKOFF: Backoff = Backoff {
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(30),
    max_elapsed: Duration::from_secs(600),
};

/// Describes a connection URL for logs: host, port and database name, never the credentials.
pub fn describe_database_target(url: &str) -> String {
    match PgConnectOptions::from_str(url) {
        Ok(options) => format!(
            "{}:{}/{}",
            options.get_host(),
            options.get_port(),
            options.get_database().unwrap_or("<default>")
        ),
        Err(_) => "<invalid database URL>".to_string(),
    }
}

/// Returns `true` for errors that can go away by waiting: the server is down, restarting or
/// not accepting connections yet. Configuration and authentication errors are not retried.
pub fn is_transient_connect_error(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut | sqlx::Error::Tls(_) => true,
        sqlx::Error::Database(db_error) => db_error.code().is_some_and(|code| {
            // 08xxx: connection exception, 57P0x: server shutting down or starting up.
            code.starts_with("08") || code.starts_with("57P")
        }),
        _ => false,
    }
}

/// Creates the pool from `config`, retrying transient failures according to `policy`.
///
/// # Errors
///
/// Returns the last connection error when it is not transient, or when retrying longer would
/// exceed `backoff.max_elapsed`.
pub async fn connect_with_retry(
    config: &DatabaseConfig,
    backoff: Backoff,
) -> Result<PgPool, sqlx::Error> {
    let connect_options = PgConnectOptions::from_str(&config.url)?;
    let pool_options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .max_lifetime(config.max_lifetime_secs.map(Duration::from_secs))
        .idle_timeout(config.idle_timeout_secs.map(Duration::from_secs));
    let target = describe_database_target(&config.url);

    retry_transient(backoff, &target, || {
        pool_options.clone().connect_with(connect_options.clone())
    })
    .await
}

/// Runs `attempt` until it succeeds, retrying transient errors with `backoff`.
async fn retry_transient<T, F, Fut>(
    backoff: Backoff,
    target: &str,
    attempt: F,
) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let mut retries = 0;
    let result = retry_with_backoff(
        backoff,
        |error| {
            if is_transient_connect_error(error) {
                RetryDecision::Retry
            } else {
                RetryDecision::Stop
            }
        },
        |retry, delay, error| {
            retries = retry;
            warn!(
                database = %target,
                retry,
                retry_in_ms = delay.as_millis(),
                error = %error,
                "Database unavailable, retrying"
            );
        },
        attempt,
    )
    .await;
    if result.is_ok() {
        info!(database = %target, retries, "Connected to database");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn database_target_never_contains_credentials() {
        let described =
            describe_database_target("postgres://bot:s3cr3t@db.internal:5433/ingredients");
        assert_eq!(described, "db.internal:5433/ingredients");
        assert!(!described.contains("s3cr3t"));
        assert!(!described.contains("bot"));
    }

    #[test]
    fn invalid_database_url_is_described_without_echoing_it() {
        let described = describe_database_target("not a url with s3cr3t");
        assert_eq!(described, "<invalid database URL>");
    }

    #[test]
    fn io_and_pool_timeouts_are_transient_configuration_errors_are_not() {
        let refused = sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        assert!(is_transient_connect_error(&refused));
        assert!(is_transient_connect_error(&sqlx::Error::PoolTimedOut));
        assert!(!is_transient_connect_error(&sqlx::Error::Configuration(
            "bad option".into()
        )));
    }

    fn fast_policy() -> Backoff {
        Backoff {
            initial_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            max_elapsed: Duration::from_millis(500),
        }
    }

    fn refused() -> sqlx::Error {
        sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused))
    }

    #[tokio::test]
    async fn transient_errors_are_retried_until_success() {
        let mut attempts = 0;
        let result = retry_transient(fast_policy(), "test", || {
            attempts += 1;
            let outcome = if attempts < 3 {
                Err(refused())
            } else {
                Ok(attempts)
            };
            async move { outcome }
        })
        .await;
        assert!(matches!(result, Ok(3)));
        assert_eq!(attempts, 3);
    }

    #[tokio::test]
    async fn non_transient_error_stops_after_one_attempt() {
        let mut attempts = 0;
        let result: Result<(), sqlx::Error> = retry_transient(fast_policy(), "test", || {
            attempts += 1;
            async { Err(sqlx::Error::Configuration("bad password".into())) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn retries_stop_once_max_elapsed_is_reached() {
        let mut attempts = 0;
        let started = Instant::now();
        let result: Result<(), sqlx::Error> = retry_transient(fast_policy(), "test", || {
            attempts += 1;
            async { Err(refused()) }
        })
        .await;
        assert!(result.is_err());
        assert!(attempts > 3, "retried: {attempts} attempts");
        assert!(started.elapsed() < Duration::from_secs(2), "bounded");
    }

    #[tokio::test]
    async fn invalid_url_fails_before_connecting() {
        let config = DatabaseConfig {
            url: "not a url".to_string(),
            ..DatabaseConfig::default()
        };
        let result = connect_with_retry(&config, fast_policy()).await;
        assert!(matches!(result, Err(sqlx::Error::Configuration(_))));
    }
}
