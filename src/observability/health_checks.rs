//! Health check functionality module.
//!
//! This module provides:
//! - Database connectivity checks
//! - OCR engine availability checks
//! - Bot token validation checks
//! - Comprehensive readiness checks

use anyhow::Result;
use sqlx::PgPool;

/// Perform comprehensive readiness checks
pub async fn perform_readiness_checks(
    db_pool: Option<std::sync::Arc<PgPool>>,
    bot_token: Option<String>,
) -> Result<()> {
    // Check database connectivity
    if let Some(pool) = &db_pool {
        check_database_health(pool.as_ref()).await?;
    }

    // Check OCR engine availability
    check_telegram_health()?;

    // Check bot token validity
    if let Some(token) = &bot_token {
        check_bot_token_health(token).await?;
    }

    Ok(())
}

/// Check database connectivity and basic query capability
pub async fn check_database_health(pool: &PgPool) -> Result<()> {
    // Simple query to test database connectivity
    sqlx::query("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("Database health check failed: {}", e))?;

    tracing::debug!("Database health check passed");
    Ok(())
}

/// Check Telegram bot token validity by testing API access
pub async fn check_bot_token_health(token: &str) -> Result<()> {
    // Create a minimal bot instance to test token validity
    // This doesn't make actual API calls, just validates the token format
    if token.is_empty() {
        return Err(anyhow::anyhow!("Bot token is empty"));
    }

    // Basic token format validation (Telegram bot tokens have a specific format)
    if !token.contains(':') {
        return Err(anyhow::anyhow!("Bot token format is invalid"));
    }

    tracing::debug!("Bot token health check passed");
    Ok(())
}

/// Start a background task to periodically record health check metrics
pub async fn start_health_metrics_recorder(
    db_pool: Option<std::sync::Arc<PgPool>>,
    bot_token: Option<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60)); // Every minute

        loop {
            interval.tick().await;

            // Perform database health check
            if let Some(pool) = &db_pool {
                let check_start = std::time::Instant::now();
                let db_healthy = check_database_health(pool.as_ref()).await.is_ok();
                let check_duration = check_start.elapsed();
                crate::observability::metrics::record_health_check_metrics(
                    "database",
                    db_healthy,
                    check_duration,
                );
            }

            // Perform OCR health check

            // Perform bot token health check
            if let Some(token) = &bot_token {
                let check_start = std::time::Instant::now();
                let bot_healthy =
                    check_bot_token_health(token).await.is_ok() && check_telegram_health().is_ok();
                let check_duration = check_start.elapsed();
                crate::observability::metrics::record_health_check_metrics(
                    "telegram_bot",
                    bot_healthy,
                    check_duration,
                );
            }
        }
    })
}

/// Telegram is considered unreachable when no API call succeeded for this long.
pub const TELEGRAM_STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How often the heartbeat calls `getMe`.
const TELEGRAM_HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Unix seconds of the last successful Telegram call (0: none yet).
static LAST_TELEGRAM_OK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// When the heartbeat started, for the startup grace period.
static HEARTBEAT_STARTED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Records a successful Telegram API call.
pub fn record_telegram_success() {
    LAST_TELEGRAM_OK.store(unix_now(), std::sync::atomic::Ordering::Relaxed);
}

/// Whether Telegram counts as reachable at `now`: a success within `stale_after`, or no success
/// yet but still within `stale_after` of the start (unix seconds). No heartbeat started at all
/// (tests, tools) counts as reachable.
fn telegram_reachable(last_ok: u64, started: Option<u64>, now: u64, stale_after: u64) -> bool {
    match started {
        None => true,
        Some(_) if last_ok > 0 => now.saturating_sub(last_ok) <= stale_after,
        Some(started) => now.saturating_sub(started) <= stale_after,
    }
}

/// Fails when the Telegram heartbeat has not succeeded for [`TELEGRAM_STALE_AFTER`].
pub fn check_telegram_health() -> Result<()> {
    let last_ok = LAST_TELEGRAM_OK.load(std::sync::atomic::Ordering::Relaxed);
    if telegram_reachable(
        last_ok,
        HEARTBEAT_STARTED.get().copied(),
        unix_now(),
        TELEGRAM_STALE_AFTER.as_secs(),
    ) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Telegram API unreachable: last successful call {}s ago",
            unix_now().saturating_sub(last_ok)
        ))
    }
}

/// Calls `getMe` every minute so health checks reflect whether the bot can reach Telegram
/// (network, revoked token), not only whether the process runs.
pub fn start_telegram_heartbeat(bot: teloxide::Bot) -> tokio::task::JoinHandle<()> {
    use teloxide::requests::Requester;
    let _ = HEARTBEAT_STARTED.set(unix_now());
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TELEGRAM_HEARTBEAT_INTERVAL);
        loop {
            interval.tick().await;
            match bot.get_me().await {
                Ok(_) => record_telegram_success(),
                Err(error) => tracing::warn!(error = %error, "Telegram heartbeat failed"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STALE: u64 = 300;

    #[test]
    fn telegram_is_reachable_after_a_recent_success() {
        assert!(telegram_reachable(1_000, Some(500), 1_200, STALE));
        assert!(!telegram_reachable(1_000, Some(500), 1_400, STALE), "stale");
    }

    #[test]
    fn startup_has_a_grace_period() {
        assert!(
            telegram_reachable(0, Some(1_000), 1_200, STALE),
            "just started"
        );
        assert!(
            !telegram_reachable(0, Some(1_000), 1_400, STALE),
            "never succeeded"
        );
    }

    #[test]
    fn without_a_heartbeat_telegram_is_not_judged() {
        assert!(telegram_reachable(0, None, 99_999, STALE));
    }
}
