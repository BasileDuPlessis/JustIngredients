//! Observability (plan O, `specs/tasks/observability.md`): logs, the metrics Fly collects from
//! an internal port, the public health checks.
//!
//! Logs are JSON lines in production, readable text with `LOG_FORMAT=pretty` (local runs);
//! `RUST_LOG` filters them (`info,sqlx=warn` on Fly).

pub mod health_checks;
pub mod metrics;
pub mod server;

pub use health_checks::*;
pub use metrics::*;

use tracing_subscriber::prelude::*;

/// Installs the log subscriber. Call it first at startup, so that everything logged afterwards
/// (database connection retries included) is visible; only once per process.
pub fn init_logging() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sqlx=warn,teloxide=warn"));
    let pretty = std::env::var("LOG_FORMAT").is_ok_and(|format| format == "pretty");
    if pretty {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().pretty().with_target(true))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_current_span(false)
                    .flatten_event(true),
            )
            .init();
    }
    tracing::info!(
        format = if pretty { "pretty" } else { "json" },
        "Logging started"
    );
    Ok(())
}

/// Starts the metrics recorder and the two HTTP servers (public health checks, internal
/// metrics), and records the time of this start.
pub async fn start(
    db_pool: Option<std::sync::Arc<sqlx::PgPool>>,
    bot_token: Option<String>,
    config: &crate::config::AppConfig,
) -> anyhow::Result<server::Addresses> {
    let handle = metrics::install_recorder()?;
    // Without the exporter's own HTTP listener nothing drains the histograms between two reads:
    // the upkeep bounds their memory
    let upkeep = handle.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            every.tick().await;
            upkeep.run_upkeep();
        }
    });
    let addresses = server::start(
        handle,
        config.server.health_port,
        config.server.metrics_port,
        config.server.bind_all_interfaces,
        db_pool,
        bot_token,
    )
    .await?;
    metrics::started();
    Ok(addresses)
}

/// A span for database operations.
pub fn db_span(operation: &str, table: &str) -> tracing::Span {
    tracing::info_span!(
        "db_operation",
        operation = operation,
        table = table,
        component = "database"
    )
}

/// A span for Telegram updates.
pub fn telegram_span(operation: &str, user_id: Option<i64>) -> tracing::Span {
    tracing::info_span!(
        "telegram_operation",
        operation = operation,
        user_id = user_id,
        component = "telegram"
    )
}
