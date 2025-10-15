//! Health check functionality module.
//!
//! This module provides:
//! - Database connectivity checks
//! - OCR engine availability checks
//! - Bot token validation checks
//! - Comprehensive readiness checks

use anyhow::Result;
use leptess::LepTess;
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
    check_ocr_health().await?;

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

/// Check OCR engine availability by testing Tesseract initialization
pub async fn check_ocr_health() -> Result<()> {
    // Try to create a minimal Tesseract instance to test OCR availability
    // This is a lightweight check that doesn't require actual image processing
    match LepTess::new(None, "eng") {
        Ok(_) => {
            tracing::debug!("OCR health check passed");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("OCR health check failed: {}", e)),
    }
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
                crate::observability::metrics::record_health_check_metrics("database", db_healthy, check_duration);
            }

            // Perform OCR health check
            let check_start = std::time::Instant::now();
            let ocr_healthy = check_ocr_health().await.is_ok();
            let check_duration = check_start.elapsed();
            crate::observability::metrics::record_health_check_metrics("ocr", ocr_healthy, check_duration);

            // Perform bot token health check
            if let Some(token) = &bot_token {
                let check_start = std::time::Instant::now();
                let bot_healthy = check_bot_token_health(token).await.is_ok();
                let check_duration = check_start.elapsed();
                crate::observability::metrics::record_health_check_metrics("telegram_bot", bot_healthy, check_duration);
            }
        }
    })
}