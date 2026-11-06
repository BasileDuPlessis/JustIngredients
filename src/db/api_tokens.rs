//! Personal API tokens (migration 11): stored by SHA-256, scoped by owner. What a token is and
//! how it is checked is `engine::tokens`; this module only reads and writes the rows.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;

/// An active token of a user, as the bot lists it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ApiTokenRow {
    pub id: i64,
    pub hint: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// The active token a hash belongs to: its id, its owner's Telegram id, its scopes.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ApiTokenOwner {
    pub id: i64,
    pub telegram_id: i64,
    pub scopes: Vec<String>,
}

/// Stores a new token of `telegram_id` (its user row created when needed), unless the user
/// already has `max_active` active tokens: `None` then, and nothing is written. The user's row is
/// locked for the transaction, so two creations at once cannot both pass the count.
pub async fn create_api_token(
    pool: &PgPool,
    telegram_id: i64,
    token_hash: &[u8],
    hint: &str,
    scopes: &[&str],
    max_active: i64,
) -> Result<Option<i64>> {
    let start = std::time::Instant::now();
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start the token transaction")?;
    let user_id = super::users::user_id_in(&mut tx, telegram_id, None).await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM api_tokens WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to count the active tokens")?;
    if active >= max_active {
        return Ok(None);
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO api_tokens (user_id, token_hash, hint, scopes) VALUES ($1, $2, $3, $4) \
         RETURNING id",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(hint)
    .bind(scopes)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to store the token")?;
    tx.commit().await.context("Failed to commit the token")?;
    crate::observability::db_query("create_api_token", start.elapsed());
    Ok(Some(id))
}

/// The active token whose SHA-256 is `token_hash`, with its owner; `None` when there is none or
/// it was revoked.
pub async fn find_api_token(pool: &PgPool, token_hash: &[u8]) -> Result<Option<ApiTokenOwner>> {
    let start = std::time::Instant::now();
    let found = sqlx::query_as::<_, ApiTokenOwner>(
        "SELECT t.id, u.telegram_id, t.scopes FROM api_tokens t \
         JOIN users u ON u.id = t.user_id \
         WHERE t.token_hash = $1 AND t.revoked_at IS NULL",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to look the token up")?;
    crate::observability::db_query("find_api_token", start.elapsed());
    Ok(found)
}

/// Notes that a token was used now, at most once every `every_secs` seconds (one write per
/// request would be one write per tool call).
pub async fn touch_api_token(pool: &PgPool, token_id: i64, every_secs: i32) -> Result<()> {
    sqlx::query(
        "UPDATE api_tokens SET last_used_at = now() WHERE id = $1 \
         AND (last_used_at IS NULL OR last_used_at < now() - make_interval(secs => $2))",
    )
    .bind(token_id)
    .bind(f64::from(every_secs))
    .execute(pool)
    .await
    .context("Failed to note the token's use")?;
    Ok(())
}

/// The active tokens of `telegram_id`, oldest first.
pub async fn list_api_tokens(pool: &PgPool, telegram_id: i64) -> Result<Vec<ApiTokenRow>> {
    sqlx::query_as::<_, ApiTokenRow>(
        "SELECT t.id, t.hint, t.scopes, t.created_at, t.last_used_at FROM api_tokens t \
         JOIN users u ON u.id = t.user_id \
         WHERE u.telegram_id = $1 AND t.revoked_at IS NULL ORDER BY t.created_at, t.id",
    )
    .bind(telegram_id)
    .fetch_all(pool)
    .await
    .context("Failed to list the tokens")
}

/// Revokes a token of `telegram_id`; `false` when it is another user's, gone or already revoked.
pub async fn revoke_api_token(pool: &PgPool, telegram_id: i64, token_id: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE api_tokens SET revoked_at = now() \
         WHERE id = $1 AND revoked_at IS NULL \
           AND user_id = (SELECT id FROM users WHERE telegram_id = $2)",
    )
    .bind(token_id)
    .bind(telegram_id)
    .execute(pool)
    .await
    .context("Failed to revoke the token")?
    .rows_affected()
        > 0)
}
