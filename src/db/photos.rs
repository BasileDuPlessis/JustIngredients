//! Photo readings (migration 15): each user's paid readings per Paris day. What they mean and the
//! limit are `engine::photos`; this module only reads and writes the rows.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::postgres::PgPool;

/// Counts one reading of `telegram_id` today (Paris), unless they already made `limit`: the day,
/// or `None` at the limit. One statement: two readings at once cannot both pass the last place.
pub async fn take_photo_call(
    pool: &PgPool,
    telegram_id: i64,
    limit: u32,
) -> Result<Option<NaiveDate>> {
    sqlx::query_scalar(
        "INSERT INTO user_photo_days (telegram_id, day, calls) \
         VALUES ($1, (now() AT TIME ZONE 'Europe/Paris')::date, 1) \
         ON CONFLICT (telegram_id, day) DO UPDATE SET calls = user_photo_days.calls + 1 \
         WHERE user_photo_days.calls < $2 \
         RETURNING day",
    )
    .bind(telegram_id)
    .bind(i64::from(limit))
    .fetch_optional(pool)
    .await
    .context("Failed to count a photo reading")
}

/// Gives back a reading counted on `day`.
pub async fn give_back_photo_call(pool: &PgPool, telegram_id: i64, day: NaiveDate) -> Result<()> {
    sqlx::query(
        "UPDATE user_photo_days SET calls = calls - 1 \
         WHERE telegram_id = $1 AND day = $2 AND calls > 0",
    )
    .bind(telegram_id)
    .bind(day)
    .execute(pool)
    .await
    .context("Failed to give back a photo reading")?;
    Ok(())
}

/// The readings of `telegram_id` today (Paris).
pub async fn photo_calls_today(pool: &PgPool, telegram_id: i64) -> Result<u32> {
    let calls: Option<i32> = sqlx::query_scalar(
        "SELECT calls FROM user_photo_days \
         WHERE telegram_id = $1 AND day = (now() AT TIME ZONE 'Europe/Paris')::date",
    )
    .bind(telegram_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read the photo readings")?;
    Ok(calls
        .and_then(|calls| u32::try_from(calls).ok())
        .unwrap_or(0))
}

/// Deletes the days older than `keep_days` days; the rows deleted.
pub async fn purge_photo_days(pool: &PgPool, keep_days: i32) -> Result<u64> {
    Ok(sqlx::query(
        "DELETE FROM user_photo_days \
         WHERE day < (now() AT TIME ZONE 'Europe/Paris')::date - $1",
    )
    .bind(keep_days)
    .execute(pool)
    .await
    .context("Failed to purge the photo readings")?
    .rows_affected())
}
