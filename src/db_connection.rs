//! # Database Connection
//!
//! Creates the PostgreSQL pool at startup. While the database is unreachable (for example
//! during a Postgres restart) the connection is retried with exponential backoff instead of
//! exiting, so the bot comes back on its own once the database is up.

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
    let started = Instant::now();
    let mut retry = 0;

    loop {
        match pool_options
            .clone()
            .connect_with(connect_options.clone())
            .await
        {
            Ok(pool) => {
                info!(database = %target, retries = retry, "Connected to database");
                return Ok(pool);
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
                    retry_in_secs = delay.as_secs(),
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

    #[tokio::test]
    async fn non_transient_error_fails_without_retrying() {
        let config = DatabaseConfig {
            url: "not a url".to_string(),
            ..DatabaseConfig::default()
        };
        let started = Instant::now();
        let result = connect_with_retry(&config, ConnectRetryPolicy::default()).await;
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn unreachable_server_is_retried_until_max_elapsed() {
        // Port 1 on localhost refuses connections immediately.
        let config = DatabaseConfig {
            url: "postgres://user:pass@127.0.0.1:1/db".to_string(),
            connect_timeout_secs: 1,
            ..DatabaseConfig::default()
        };
        let policy = ConnectRetryPolicy {
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(100),
            max_elapsed: Duration::from_millis(400),
        };
        let started = Instant::now();
        let result = connect_with_retry(&config, policy).await;
        assert!(result.is_err());
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(150),
            "retried: {elapsed:?}"
        );
        assert!(elapsed < Duration::from_secs(5), "bounded: {elapsed:?}");
    }
}
