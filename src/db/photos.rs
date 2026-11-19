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

/// An import to store: its owner, the hash of its link's secret, the key or connection that
/// asked, how long the link lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPhotoImport<'a> {
    pub telegram_id: i64,
    pub upload_hash: &'a [u8],
    pub token_id: Option<i64>,
    pub grant_id: Option<i64>,
    pub link_lifetime_secs: i64,
}

/// Stores an import waiting for its photo; its id.
pub async fn insert_photo_import(pool: &PgPool, import: &NewPhotoImport<'_>) -> Result<i64> {
    sqlx::query_scalar(
        "INSERT INTO photo_imports (telegram_id, upload_hash, token_id, grant_id, status, \
             link_expires_at, expires_at) \
         VALUES ($1, $2, $3, $4, 'waiting', now() + make_interval(secs => $5), \
                 now() + make_interval(secs => $5)) \
         RETURNING id",
    )
    .bind(import.telegram_id)
    .bind(import.upload_hash)
    .bind(import.token_id)
    .bind(import.grant_id)
    .bind(import.link_lifetime_secs as f64)
    .fetch_one(pool)
    .await
    .context("Failed to store the photo import")
}

/// An import as stored, with what its state means now.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PhotoImportRow {
    pub id: i64,
    pub telegram_id: i64,
    pub status: String,
    pub failure: Option<String>,
    pub ingredients: Option<serde_json::Value>,
    pub transcription: Option<String>,
    pub title: Option<String>,
    pub saved_recipe_ids: Option<Vec<i64>>,
    /// The link may still take a photo (not expired).
    pub link_open: bool,
    /// The import is not expired.
    pub alive: bool,
    /// A reading started long enough ago to have been cut by a restart.
    pub reading_stale: bool,
}

/// `reading_stale_secs`: after this long, a reading is taken as interrupted.
const IMPORT_COLUMNS: &str = "id, telegram_id, status, failure, ingredients, transcription, title, \
     saved_recipe_ids, link_expires_at > now() AS link_open, expires_at > now() AS alive, \
     COALESCE(reading_started_at < now() - make_interval(secs => $2), FALSE) AS reading_stale";

/// The import whose link's secret hashes to `upload_hash`.
pub async fn photo_import_by_link(
    pool: &PgPool,
    upload_hash: &[u8],
    reading_stale_secs: i64,
) -> Result<Option<PhotoImportRow>> {
    sqlx::query_as::<_, PhotoImportRow>(&format!(
        "SELECT {IMPORT_COLUMNS} FROM photo_imports WHERE upload_hash = $1"
    ))
    .bind(upload_hash)
    .bind(reading_stale_secs as f64)
    .fetch_optional(pool)
    .await
    .context("Failed to find the photo import")
}

/// The import `id` of `telegram_id`.
pub async fn photo_import_of(
    pool: &PgPool,
    telegram_id: i64,
    id: i64,
    reading_stale_secs: i64,
) -> Result<Option<PhotoImportRow>> {
    sqlx::query_as::<_, PhotoImportRow>(&format!(
        "SELECT {IMPORT_COLUMNS} FROM photo_imports WHERE id = $3 AND telegram_id = $1"
    ))
    .bind(telegram_id)
    .bind(reading_stale_secs as f64)
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to read the photo import")
}

/// Claims the link's import for a reading, once: waiting (or a reading cut by a restart), the
/// link open, and the key or connection that asked still active and allowed to write. Its id
/// and owner, else `None`.
pub async fn claim_photo_import(
    pool: &PgPool,
    upload_hash: &[u8],
    reading_stale_secs: i64,
) -> Result<Option<(i64, i64)>> {
    sqlx::query_as(
        "UPDATE photo_imports i SET status = 'reading', reading_started_at = now(), failure = NULL \
         WHERE i.upload_hash = $1 AND i.link_expires_at > now() \
           AND (i.status = 'waiting' \
                OR (i.status = 'reading' \
                    AND i.reading_started_at < now() - make_interval(secs => $2))) \
           AND (EXISTS (SELECT 1 FROM api_tokens t WHERE t.id = i.token_id \
                        AND t.revoked_at IS NULL AND 'recipes:write' = ANY(t.scopes)) \
                OR EXISTS (SELECT 1 FROM oauth_grants g WHERE g.id = i.grant_id \
                           AND g.revoked_at IS NULL AND 'recipes:write' = ANY(g.scopes))) \
         RETURNING i.id, i.telegram_id",
    )
    .bind(upload_hash)
    .bind(reading_stale_secs as f64)
    .fetch_optional(pool)
    .await
    .context("Failed to claim the photo import")
}

/// The reading of import `id` gave no draft: back to waiting for another photo with `failure`
/// (`retry`), or failed for good.
pub async fn photo_reading_failed(
    pool: &PgPool,
    id: i64,
    failure: &str,
    retry: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE photo_imports SET status = $3, failure = $2, reading_started_at = NULL \
         WHERE id = $1 AND status = 'reading'",
    )
    .bind(id)
    .bind(failure)
    .bind(if retry { "waiting" } else { "failed" })
    .execute(pool)
    .await
    .context("Failed to record the photo's reading")?;
    Ok(())
}

/// The reading of import `id` gave a draft, kept `draft_lifetime_secs`.
pub async fn photo_reading_done(
    pool: &PgPool,
    id: i64,
    ingredients: &serde_json::Value,
    transcription: &str,
    title: Option<&str>,
    draft_lifetime_secs: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE photo_imports SET status = 'ready', ingredients = $2, transcription = $3, \
             title = $4, failure = NULL, expires_at = now() + make_interval(secs => $5) \
         WHERE id = $1 AND status = 'reading'",
    )
    .bind(id)
    .bind(ingredients)
    .bind(transcription)
    .bind(title)
    .bind(draft_lifetime_secs as f64)
    .execute(pool)
    .await
    .context("Failed to store the photo's draft")?;
    Ok(())
}

/// Claims the ready draft `id` of `telegram_id` for its save, in `tx` (the save's transaction):
/// `false` when it is not ready, expired, or saved already.
pub async fn claim_photo_draft(
    conn: &mut sqlx::PgConnection,
    telegram_id: i64,
    id: i64,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE photo_imports SET status = 'saved' \
         WHERE id = $1 AND telegram_id = $2 AND status = 'ready' AND expires_at > now()",
    )
    .bind(id)
    .bind(telegram_id)
    .execute(conn)
    .await
    .context("Failed to claim the photo's draft")?
    .rows_affected()
        > 0)
}

/// Records the recipes a draft was saved as.
pub async fn photo_draft_saved(
    conn: &mut sqlx::PgConnection,
    id: i64,
    recipe_ids: &[i64],
) -> Result<()> {
    sqlx::query("UPDATE photo_imports SET saved_recipe_ids = $2 WHERE id = $1")
        .bind(id)
        .bind(recipe_ids)
        .execute(conn)
        .await
        .context("Failed to record the saved recipes")?;
    Ok(())
}

/// Drops the import `id` of `telegram_id` unless it was saved; `false` when there is none.
pub async fn discard_photo_import(pool: &PgPool, telegram_id: i64, id: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE photo_imports SET status = 'discarded', ingredients = NULL, transcription = NULL, \
             title = NULL \
         WHERE id = $1 AND telegram_id = $2 AND status NOT IN ('saved', 'discarded')",
    )
    .bind(id)
    .bind(telegram_id)
    .execute(pool)
    .await
    .context("Failed to discard the photo import")?
    .rows_affected()
        > 0)
}

/// Deletes the imports expired a day ago (kept that long to tell "expired" from "unknown").
pub async fn purge_photo_imports(pool: &PgPool) -> Result<u64> {
    Ok(
        sqlx::query("DELETE FROM photo_imports WHERE expires_at < now() - interval '1 day'")
            .execute(pool)
            .await
            .context("Failed to purge the photo imports")?
            .rows_affected(),
    )
}
