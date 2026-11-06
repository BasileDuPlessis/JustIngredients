//! The outbox (migration 12): events written in the transaction of their change, read in order
//! by consumers that each keep a cursor. What an event means is `engine::events`; this module
//! only reads and writes the rows.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::Transaction;
use sqlx::postgres::{PgPool, Postgres};

/// The channel `pg_notify` announces new events on, after their transaction commits.
pub const EVENTS_CHANNEL: &str = "ji_events";

/// Taken first by every transaction that will write an event ([`lock_events`]), before any row
/// lock: such transactions run one after the other, so ids are committed in increasing order (a
/// consumer reading "after its cursor" can never miss one committed late), and since it is always
/// the first lock taken, two of them cannot wait for each other (no deadlock).
const EVENTS_LOCK: i64 = 0x6A69_6576; // "jiev"

/// Takes the events lock for the rest of `tx`: the first statement of a transaction that writes
/// an event.
pub async fn lock_events(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(EVENTS_LOCK)
        .execute(&mut **tx)
        .await
        .context("Failed to take the events lock")?;
    Ok(())
}

/// An event to write.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEventRow<'a> {
    pub kind: &'a str,
    pub recipe_id: Option<i64>,
    pub origin: &'a str,
    pub token_id: Option<i64>,
    pub payload: serde_json::Value,
}

/// An event as stored, with its owner's Telegram id.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EventRow {
    pub id: i64,
    pub telegram_id: i64,
    pub kind: String,
    pub recipe_id: Option<i64>,
    pub origin: String,
    pub token_id: Option<i64>,
    pub version: i16,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Writes an event of the user of `telegram_id` in `tx`, the transaction of the change, which
/// began with [`lock_events`]; announces it (delivered when the transaction commits). Returns
/// its id. The user's row is only read, not locked: every change is made by an existing user.
pub async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    telegram_id: i64,
    event: &NewEventRow<'_>,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO events (user_id, kind, recipe_id, origin, token_id, payload) \
         SELECT id, $2, $3, $4, $5, $6 FROM users WHERE telegram_id = $1 RETURNING id",
    )
    .bind(telegram_id)
    .bind(event.kind)
    .bind(event.recipe_id)
    .bind(event.origin)
    .bind(event.token_id)
    .bind(&event.payload)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to write the event")?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(EVENTS_CHANNEL)
        .bind(id.to_string())
        .execute(&mut **tx)
        .await
        .context("Failed to announce the event")?;
    Ok(id)
}

/// The events after `last_id`, in order, at most `limit`.
pub async fn events_after(pool: &PgPool, last_id: i64, limit: i64) -> Result<Vec<EventRow>> {
    sqlx::query_as::<_, EventRow>(
        "SELECT e.id, u.telegram_id, e.kind, e.recipe_id, e.origin, e.token_id, e.version, \
                e.payload, e.created_at \
         FROM events e JOIN users u ON u.id = e.user_id \
         WHERE e.id > $1 ORDER BY e.id LIMIT $2",
    )
    .bind(last_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to read the events")
}

/// The last event `consumer` handled; `None` for a consumer that never ran.
pub async fn event_cursor(pool: &PgPool, consumer: &str) -> Result<Option<i64>> {
    sqlx::query_scalar("SELECT last_id FROM event_cursors WHERE consumer = $1")
        .bind(consumer)
        .fetch_optional(pool)
        .await
        .context("Failed to read an event cursor")
}

/// Notes that `consumer` handled the events up to `last_id`.
pub async fn set_event_cursor(pool: &PgPool, consumer: &str, last_id: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO event_cursors (consumer, last_id, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (consumer) DO UPDATE SET last_id = EXCLUDED.last_id, updated_at = now()",
    )
    .bind(consumer)
    .bind(last_id)
    .execute(pool)
    .await
    .context("Failed to write an event cursor")?;
    Ok(())
}

/// The id of the last event written; 0 without any. A new consumer starts there: it handles
/// what happens from now on, not the past.
pub async fn last_event_id(pool: &PgPool) -> Result<i64> {
    let last: Option<i64> = sqlx::query_scalar("SELECT max(id) FROM events")
        .fetch_one(pool)
        .await
        .context("Failed to read the last event")?;
    Ok(last.unwrap_or(0))
}

/// Deletes the events older than `days` days; returns how many.
pub async fn purge_events_older_than(pool: &PgPool, days: i32) -> Result<u64> {
    Ok(
        sqlx::query("DELETE FROM events WHERE created_at < now() - make_interval(days => $1)")
            .bind(days)
            .execute(pool)
            .await
            .context("Failed to purge the events")?
            .rows_affected(),
    )
}
