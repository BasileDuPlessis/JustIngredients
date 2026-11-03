//! Users: lookup and creation, unit view preference, last visit.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use tracing::{debug, error};

/// Represents a user in the database
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How the recipe cards of `telegram_id` show measures; the default (French units) for a user
/// who never chose, or not saved yet.
pub async fn unit_view(pool: &PgPool, telegram_id: i64) -> Result<crate::unit_view::UnitView> {
    let saved: Option<String> =
        sqlx::query_scalar("SELECT unit_view FROM users WHERE telegram_id = $1")
            .bind(telegram_id)
            .fetch_optional(pool)
            .await
            .context("Failed to read the unit view")?;
    Ok(saved
        .as_deref()
        .and_then(crate::unit_view::UnitView::parse)
        .unwrap_or_default())
}

/// Saves the unit view `telegram_id` chose, creating the user when needed.
pub async fn set_unit_view(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
    view: crate::unit_view::UnitView,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (telegram_id, language_code, unit_view) VALUES ($1, $2, $3) \
         ON CONFLICT (telegram_id) DO UPDATE SET unit_view = EXCLUDED.unit_view, \
         updated_at = CURRENT_TIMESTAMP",
    )
    .bind(telegram_id)
    .bind(language_code.unwrap_or("en"))
    .bind(view.as_str())
    .execute(pool)
    .await
    .context("Failed to save the unit view")?;
    Ok(())
}

// --- Usage history (plan O3, `specs/tasks/observability.md`) ---

/// Records a visit of `telegram_id` now: `users.last_seen_at` (the user is created when needed),
/// and one more active user for the day, Paris time, at the user's first visit of the day.
pub async fn record_user_seen(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<()> {
    // One statement: `previous` sees the row as it was before `seen` updates it
    sqlx::query(
        "WITH previous AS (SELECT last_seen_at FROM users WHERE telegram_id = $1), \
         seen AS ( \
             INSERT INTO users (telegram_id, language_code, last_seen_at) VALUES ($1, $2, now()) \
             ON CONFLICT (telegram_id) DO UPDATE SET last_seen_at = now() \
             RETURNING 1 \
         ) \
         INSERT INTO usage_days (day, active_users) \
         SELECT (now() AT TIME ZONE 'Europe/Paris')::date, 1 FROM seen LEFT JOIN previous ON true \
         WHERE previous.last_seen_at IS NULL \
            OR (previous.last_seen_at AT TIME ZONE 'Europe/Paris')::date \
               < (now() AT TIME ZONE 'Europe/Paris')::date \
         ON CONFLICT (day) DO UPDATE SET active_users = usage_days.active_users + 1",
    )
    .bind(telegram_id)
    .bind(language_code.unwrap_or("en"))
    .execute(pool)
    .await
    .context("Failed to record the user's visit")?;
    Ok(())
}

/// Get or create a user by Telegram ID
pub async fn get_or_create_user(
    pool: &PgPool,
    telegram_id: i64,
    language_code: Option<&str>,
) -> Result<User> {
    debug!(telegram_id = %telegram_id, "Starting get_or_create_user");

    // Try to get existing user
    debug!(telegram_id = %telegram_id, "Attempting to find existing user");
    match get_user_by_telegram_id(pool, telegram_id).await {
        Ok(Some(user)) => {
            debug!(telegram_id = %telegram_id, user_id = %user.id, "Found existing user, returning");
            return Ok(user);
        }
        Ok(None) => {
            debug!(telegram_id = %telegram_id, "No existing user found, will create new user");
        }
        Err(e) => {
            error!(telegram_id = %telegram_id, error = %e, "Error looking up existing user");
            return Err(e);
        }
    }

    // Create new user
    let language_code = language_code.unwrap_or("en");
    debug!(telegram_id = %telegram_id, language_code = %language_code, "Attempting to create new user");

    let insert_result = sqlx::query_as::<_, User>(
        "INSERT INTO users (telegram_id, language_code) VALUES ($1, $2) RETURNING id, telegram_id, COALESCE(language_code, 'en') AS language_code, created_at, updated_at"
    )
    .bind(telegram_id)
    .bind(language_code)
    .fetch_one(pool)
    .await;

    match insert_result {
        Ok(row) => {
            let user = row;

            debug!(telegram_id = %telegram_id, user_id = %user.id, returned_telegram_id = %user.telegram_id, "User insert succeeded, verifying data");

            // Verify that the created user has the correct telegram_id
            if user.telegram_id != telegram_id {
                error!(
                    expected_telegram_id = %telegram_id,
                    actual_telegram_id = %user.telegram_id,
                    user_id = %user.id,
                    "CRITICAL: User creation returned wrong telegram_id!"
                );
                return Err(anyhow::anyhow!(
                    "User creation returned wrong telegram_id: expected {}, got {}",
                    telegram_id,
                    user.telegram_id
                ));
            }

            debug!(user_id = %user.id, telegram_id = %user.telegram_id, "User created and verified successfully");
            Ok(user)
        }
        Err(e) => {
            error!(telegram_id = %telegram_id, language_code = %language_code, error = %e, "User insert failed");
            Err(e.into())
        }
    }
}

/// Get a user by Telegram ID
pub async fn get_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> Result<Option<User>> {
    debug!(telegram_id = %telegram_id, "Getting user by telegram_id");

    let row = sqlx::query_as::<_, User>("SELECT id, telegram_id, COALESCE(language_code, 'en') AS language_code, created_at, updated_at FROM users WHERE telegram_id = $1")
        .bind(telegram_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get user by telegram_id")?;

    match row {
        Some(row) => {
            let user = row;
            debug!("User found with ID: {}", user.id);
            Ok(Some(user))
        }
        None => {
            debug!("No user found with telegram_id: {telegram_id}");
            Ok(None)
        }
    }
}

/// Get a user by internal database ID
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> {
    debug!(user_id = %user_id, "Getting user by internal ID");

    let row = sqlx::query_as::<_, User>(
        "SELECT id, telegram_id, COALESCE(language_code, 'en') AS language_code, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("Failed to get user by internal ID")?;

    match row {
        Some(row) => {
            let user = row;
            debug!(user_id = %user.id, "User found by internal ID");
            Ok(Some(user))
        }
        None => {
            debug!(user_id = %user_id, "No user found with internal ID");
            Ok(None)
        }
    }
}
