//! Usage history (`usage_days`) and values kept across restarts (`app_state`).

use anyhow::{Context, Result};
use sqlx::postgres::PgPool;
use sqlx::Row;

/// What happened during part of a day, added to its line of `usage_days`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageCounts {
    pub photos_read: i64,
    pub photos_failed: i64,
    pub recipes_created: i64,
    pub gemini_calls: i64,
    pub gemini_failures: i64,
    pub gemini_cost_micro_usd: i64,
}

impl std::ops::AddAssign for UsageCounts {
    fn add_assign(&mut self, other: Self) {
        self.photos_read += other.photos_read;
        self.photos_failed += other.photos_failed;
        self.recipes_created += other.recipes_created;
        self.gemini_calls += other.gemini_calls;
        self.gemini_failures += other.gemini_failures;
        self.gemini_cost_micro_usd += other.gemini_cost_micro_usd;
    }
}

/// Adds `counts` to the line of `day` (Paris time), created when needed.
pub async fn add_usage(pool: &PgPool, day: chrono::NaiveDate, counts: &UsageCounts) -> Result<()> {
    sqlx::query(
        "INSERT INTO usage_days (day, photos_read, photos_failed, recipes_created, gemini_calls, \
         gemini_failures, gemini_cost_micro_usd) VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (day) DO UPDATE SET \
         photos_read = usage_days.photos_read + EXCLUDED.photos_read, \
         photos_failed = usage_days.photos_failed + EXCLUDED.photos_failed, \
         recipes_created = usage_days.recipes_created + EXCLUDED.recipes_created, \
         gemini_calls = usage_days.gemini_calls + EXCLUDED.gemini_calls, \
         gemini_failures = usage_days.gemini_failures + EXCLUDED.gemini_failures, \
         gemini_cost_micro_usd = usage_days.gemini_cost_micro_usd + EXCLUDED.gemini_cost_micro_usd",
    )
    .bind(day)
    .bind(counts.photos_read)
    .bind(counts.photos_failed)
    .bind(counts.recipes_created)
    .bind(counts.gemini_calls)
    .bind(counts.gemini_failures)
    .bind(counts.gemini_cost_micro_usd)
    .execute(pool)
    .await
    .context("Failed to save the day's usage")?;
    Ok(())
}

/// Users seen in the last 24 hours and 7 days, all users, and all recipes: the `ji_users` and
/// `ji_recipes` gauges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub users_last_day: i64,
    pub users_last_week: i64,
    pub users: i64,
    pub recipes: i64,
}

pub async fn usage_totals(pool: &PgPool) -> Result<UsageTotals> {
    let row = sqlx::query(
        "SELECT count(*) FILTER (WHERE last_seen_at > now() - interval '1 day') AS last_day, \
         count(*) FILTER (WHERE last_seen_at > now() - interval '7 days') AS last_week, \
         count(*) AS users, (SELECT count(*) FROM recipes) AS recipes FROM users",
    )
    .fetch_one(pool)
    .await
    .context("Failed to count users and recipes")?;
    Ok(UsageTotals {
        users_last_day: row.try_get("last_day")?,
        users_last_week: row.try_get("last_week")?,
        users: row.try_get("users")?,
        recipes: row.try_get("recipes")?,
    })
}

/// The usage of the days `from..=to` (Paris time; from the first day without `from`), for the
/// weekly report and `/stats` (plan O5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageSummary {
    /// Users seen since `from` (since `from`'s midnight, so also after `to`: a user keeps only
    /// the last visit); every user seen once without `from`.
    pub active_users: i64,
    /// Users who first came during the period.
    pub new_users: i64,
    /// All users today.
    pub users: i64,
    pub counts: UsageCounts,
}

pub async fn usage_summary(
    pool: &PgPool,
    from: Option<chrono::NaiveDate>,
    to: chrono::NaiveDate,
) -> Result<UsageSummary> {
    let row = sqlx::query(
        "WITH bounds AS ( \
             SELECT $1::date::timestamp AT TIME ZONE 'Europe/Paris' AS since, \
                    ($2::date + 1)::timestamp AT TIME ZONE 'Europe/Paris' AS until \
         ) \
         SELECT \
           (SELECT count(*) FROM users, bounds WHERE last_seen_at >= coalesce(bounds.since, '-infinity')) \
             AS active_users, \
           (SELECT count(*) FROM users, bounds WHERE created_at >= coalesce(bounds.since, '-infinity') \
             AND created_at < bounds.until) AS new_users, \
           (SELECT count(*) FROM users) AS users, \
           coalesce(sum(photos_read), 0)::bigint AS photos_read, \
           coalesce(sum(photos_failed), 0)::bigint AS photos_failed, \
           coalesce(sum(recipes_created), 0)::bigint AS recipes_created, \
           coalesce(sum(gemini_calls), 0)::bigint AS gemini_calls, \
           coalesce(sum(gemini_failures), 0)::bigint AS gemini_failures, \
           coalesce(sum(gemini_cost_micro_usd), 0)::bigint AS gemini_cost_micro_usd \
         FROM usage_days WHERE ($1::date IS NULL OR day >= $1) AND day <= $2",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .context("Failed to read the usage history")?;
    Ok(UsageSummary {
        active_users: row.try_get("active_users")?,
        new_users: row.try_get("new_users")?,
        users: row.try_get("users")?,
        counts: UsageCounts {
            photos_read: row.try_get("photos_read")?,
            photos_failed: row.try_get("photos_failed")?,
            recipes_created: row.try_get("recipes_created")?,
            gemini_calls: row.try_get("gemini_calls")?,
            gemini_failures: row.try_get("gemini_failures")?,
            gemini_cost_micro_usd: row.try_get("gemini_cost_micro_usd")?,
        },
    })
}

/// A value kept across restarts (`app_state`): the release running, the last weekly report.
pub async fn app_state(pool: &PgPool, key: &str) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT value FROM app_state WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .context("Failed to read the app state")
}

pub async fn set_app_state(pool: &PgPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_state (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .context("Failed to save the app state")?;
    Ok(())
}
