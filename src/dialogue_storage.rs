//! Dialogue states kept in Postgres (`dialogues`, migration 8), so a deploy or a restart does
//! not lose a running review or edit, nor the paid Gemini read behind it (backlog IMP-022).
//!
//! One row per chat: the state as JSON and when it last changed. A state left unchanged for
//! [`DIALOGUE_TTL_HOURS`] is ignored and purged at startup. A state that no longer reads (its
//! shape changed in a release) is dropped, so its buttons answer `button-expired`, as they did
//! when states were lost at each restart.
//!
//! The states are only resumed when this version was running a moment ago: it notes it every
//! minute and when it stops ([`PgDialogueStorage::mark_alive`]). A start after a longer gap (an
//! older version, keeping states in memory, ran in between, e.g. after an automatic rollback; or
//! the bot was down) begins with none, since the user may have started over meanwhile.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use serde::{de::DeserializeOwned, Serialize};
use sqlx::PgPool;
use teloxide::dispatching::dialogue::Storage;
use teloxide::types::ChatId;
use tracing::warn;

/// A state unchanged for this long is gone: after 48 hours Telegram no longer lets the bot
/// delete the action's messages, so the end of the action could not clean them up.
pub const DIALOGUE_TTL_HOURS: i32 = 48;

/// A start within this time of the last note of [`PgDialogueStorage::mark_alive`] resumes the
/// states: a deploy stops the bot for about a minute.
pub const RESUME_WINDOW_MINUTES: i32 = 10;

/// `app_state` key: when this version last noted it was running (its `updated_at`).
const ALIVE: &str = "dialogues_alive";

/// The future type of teloxide's `Storage` methods (`futures::future::BoxFuture`).
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Why a dialogue state could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum DialogueStorageError {
    #[error("dialogue state query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("dialogue state could not be serialized: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// teloxide dialogue storage on the `dialogues` table.
#[derive(Debug)]
pub struct PgDialogueStorage {
    pool: PgPool,
}

impl PgDialogueStorage {
    pub fn new(pool: PgPool) -> Arc<Self> {
        Arc::new(Self { pool })
    }

    /// At startup, before any update: deletes the states that are not resumed (all of them after
    /// a gap over [`RESUME_WINDOW_MINUTES`], else those unchanged for [`DIALOGUE_TTL_HOURS`]),
    /// then notes that this version runs. Returns how many states went.
    pub async fn start(&self) -> Result<u64, sqlx::Error> {
        let start = Instant::now();
        let resumed: Option<bool> = sqlx::query_scalar(
            "SELECT updated_at >= now() - make_interval(mins => $2) FROM app_state WHERE key = $1",
        )
        .bind(ALIVE)
        .bind(RESUME_WINDOW_MINUTES)
        .fetch_optional(&self.pool)
        .await?;
        let result = if resumed == Some(true) {
            sqlx::query(
                "DELETE FROM dialogues WHERE updated_at < now() - make_interval(hours => $1)",
            )
            .bind(DIALOGUE_TTL_HOURS)
            .execute(&self.pool)
            .await?
        } else {
            sqlx::query("DELETE FROM dialogues")
                .execute(&self.pool)
                .await?
        };
        crate::observability::db_query("dialogue_purge", start.elapsed());
        self.mark_alive().await?;
        Ok(result.rows_affected())
    }

    /// Notes that this version, which keeps the states, is running: every minute and when it
    /// stops.
    pub async fn mark_alive(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO app_state (key, value, updated_at) VALUES ($1, '', now()) \
             ON CONFLICT (key) DO UPDATE SET updated_at = now()",
        )
        .bind(ALIVE)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, chat_id: ChatId) -> Result<(), sqlx::Error> {
        let start = Instant::now();
        sqlx::query("DELETE FROM dialogues WHERE chat_id = $1")
            .bind(chat_id.0)
            .execute(&self.pool)
            .await?;
        crate::observability::db_query("dialogue_remove", start.elapsed());
        Ok(())
    }
}

impl<D> Storage<D> for PgDialogueStorage
where
    D: Serialize + DeserializeOwned + Send + 'static,
{
    type Error = DialogueStorageError;

    /// Unlike teloxide's `InMemStorage`, removing a missing state is not an error: the action
    /// is over either way.
    fn remove_dialogue(self: Arc<Self>, chat_id: ChatId) -> BoxFuture<Result<(), Self::Error>>
    where
        D: Send + 'static,
    {
        Box::pin(async move { Ok(self.delete(chat_id).await?) })
    }

    fn update_dialogue(
        self: Arc<Self>,
        chat_id: ChatId,
        dialogue: D,
    ) -> BoxFuture<Result<(), Self::Error>>
    where
        D: Send + 'static,
    {
        Box::pin(async move {
            let state = serde_json::to_string(&dialogue)?;
            let start = Instant::now();
            sqlx::query(
                "INSERT INTO dialogues (chat_id, state, updated_at) VALUES ($1, $2::json, now()) \
                 ON CONFLICT (chat_id) DO UPDATE \
                 SET state = EXCLUDED.state, updated_at = EXCLUDED.updated_at",
            )
            .bind(chat_id.0)
            .bind(state)
            .execute(&self.pool)
            .await?;
            crate::observability::db_query("dialogue_update", start.elapsed());
            Ok(())
        })
    }

    fn get_dialogue(self: Arc<Self>, chat_id: ChatId) -> BoxFuture<Result<Option<D>, Self::Error>> {
        Box::pin(async move {
            let start = Instant::now();
            let state: Option<String> = sqlx::query_scalar(
                "SELECT state::text FROM dialogues \
                 WHERE chat_id = $1 AND updated_at >= now() - make_interval(hours => $2)",
            )
            .bind(chat_id.0)
            .bind(DIALOGUE_TTL_HOURS)
            .fetch_optional(&self.pool)
            .await?;
            crate::observability::db_query("dialogue_get", start.elapsed());
            let Some(state) = state else {
                return Ok(None);
            };
            match serde_json::from_str(&state) {
                Ok(dialogue) => Ok(Some(dialogue)),
                Err(error) => {
                    // The error's text can quote the state (the user's ingredients): only its
                    // kind and position are logged
                    warn!(
                        category = ?error.classify(),
                        column = error.column(),
                        "Dropped a dialogue state that no longer reads"
                    );
                    self.delete(chat_id).await?;
                    Ok(None)
                }
            }
        })
    }
}
