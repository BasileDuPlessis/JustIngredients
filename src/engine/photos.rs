//! Recipe photos (`specs/tasks/mcp-photos.md`): every door's photo readings go through the bot's
//! Gemini pipeline (`extraction::GuardedExtractor`). This module keeps each user's daily count of
//! paid readings in the database: shared by Telegram and the assistants, kept across deploys,
//! Paris days.

use sqlx::PgPool;

use super::EngineError;
use crate::extraction::guard::{DailyCalls, DailyFuture};

/// Days of counts kept (only today's is read).
const KEEP_DAYS: i32 = 7;

/// The daily count of paid readings, in the database.
#[derive(Debug, Clone)]
pub struct StoredDailyCalls {
    pool: PgPool,
}

impl StoredDailyCalls {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DailyCalls for StoredDailyCalls {
    fn take(
        &self,
        user: i64,
        limit: u32,
    ) -> DailyFuture<'_, anyhow::Result<Option<chrono::NaiveDate>>> {
        Box::pin(async move { crate::db::take_photo_call(&self.pool, user, limit).await })
    }

    fn give_back(&self, user: i64, day: chrono::NaiveDate) -> DailyFuture<'_, ()> {
        Box::pin(async move {
            if let Err(error) = crate::db::give_back_photo_call(&self.pool, user, day).await {
                tracing::warn!(error = %error, "Could not give back a photo reading");
            }
        })
    }
}

/// Deletes the counts of past days.
pub async fn purge(pool: &PgPool) -> Result<u64, EngineError> {
    Ok(crate::db::purge_photo_days(pool, KEEP_DAYS).await?)
}
