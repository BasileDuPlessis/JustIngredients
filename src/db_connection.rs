//! # Database Connection
//!
//! Creates the PostgreSQL pool at startup. While the database is unreachable (for example
//! during a Postgres restart) the connection is retried with exponential backoff instead of
//! exiting, so the bot comes back on its own once the database is up.

use std::future::Future;
use std::str::FromStr;
use std::time::{Duration, Instant};

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use tracing::{info, warn};

use crate::config::DatabaseConfig;

/// Retry schedule used while the database is unreachable at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectRetryPolicy {
    /// Delay before the first retry.
    pub initial_delay: Duration,
    /// Upper bound for the delay between two attempts.
    pub max_delay: Duration,
    /// Give up once this much time has passed since the first attempt.
    pub max_elapsed: Duration,
}

impl Default for ConnectRetryPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_elapsed: Duration::from_secs(600),
        }
    }
}

impl ConnectRetryPolicy {
    /// Delay before retry number `retry` (1-based): doubles each time, capped at `max_delay`.
    pub fn delay_before_retry(&self, retry: u32) -> Duration {
        let factor = 2u32.saturating_pow(retry.saturating_sub(1));
        self.initial_delay
            .saturating_mul(factor)
            .min(self.max_delay)
    }
}

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
/// exceed `policy.max_elapsed`.
pub async fn connect_with_retry(
    config: &DatabaseConfig,
    policy: ConnectRetryPolicy,
) -> Result<PgPool, sqlx::Error> {
    let connect_options = PgConnectOptions::from_str(&config.url)?;
    let pool_options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .max_lifetime(config.max_lifetime_secs.map(Duration::from_secs))
        .idle_timeout(config.idle_timeout_secs.map(Duration::from_secs));
    let target = describe_database_target(&config.url);

    retry_transient(policy, &target, || {
        pool_options.clone().connect_with(connect_options.clone())
    })
    .await
}

/// Runs `attempt` until it succeeds, retrying transient errors with the backoff of `policy`.
///
/// Stops at the first non-transient error, or when the next retry would end after
/// `policy.max_elapsed`.
async fn retry_transient<T, F, Fut>(
    policy: ConnectRetryPolicy,
    target: &str,
    mut attempt: F,
) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let started = Instant::now();
    let mut retry = 0;

    loop {
        match attempt().await {
            Ok(value) => {
                info!(database = %target, retries = retry, "Connected to database");
                return Ok(value);
            }
            Err(error) => {
                retry += 1;
                let delay = policy.delay_before_retry(retry);
                if !is_transient_connect_error(&error)
                    || started.elapsed() + delay > policy.max_elapsed
                {
                    return Err(error);
                }
                warn!(
                    database = %target,
                    retry,
                    retry_in_ms = delay.as_millis(),
                    error = %error,
                    "Database unavailable, retrying"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_and_is_capped() {
        let policy = ConnectRetryPolicy::default();
        let delays: Vec<u64> = (1..=7)
            .map(|retry| policy.delay_before_retry(retry).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30]);
        assert_eq!(policy.delay_before_retry(u32::MAX), policy.max_delay);
    }

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

    fn fast_policy() -> ConnectRetryPolicy {
        ConnectRetryPolicy {
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
