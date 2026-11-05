//! Ids of messages a later action deletes or edits (`chat_messages`, UX plan U5).

use anyhow::{Context, Result};
use sqlx::postgres::PgPool;

/// The recipe of a recorded chat message (a card), if any.
pub async fn chat_message_recipe(
    pool: &PgPool,
    chat_id: i64,
    message_id: i32,
) -> Result<Option<i64>> {
    let recipe: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT recipe_id FROM chat_messages WHERE chat_id = $1 AND message_id = $2",
    )
    .bind(chat_id)
    .bind(i64::from(message_id))
    .fetch_optional(pool)
    .await
    .context("Failed to look up a chat message")?;
    Ok(recipe.flatten())
}

/// A message recorded in `chat_messages`, taken back to be deleted or edited.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChatMessage {
    /// `BIGINT` in the table; Telegram's message ids fit an `i32`
    #[sqlx(try_from = "i64")]
    pub message_id: i32,
    /// What it is: `transient`, `home`, `list`, `card` or `active_card`.
    pub kind: String,
    /// The recipe of a card.
    pub recipe_id: Option<i64>,
    /// Sent less than 48 hours ago: Telegram still lets the bot delete it.
    pub deletable: bool,
}

/// Records a message of `chat` with its `kind` (and recipe, for a card). Recording it again
/// replaces its kind and recipe.
pub async fn record_chat_message(
    pool: &PgPool,
    chat_id: i64,
    message_id: i32,
    kind: &str,
    recipe_id: Option<i64>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO chat_messages (chat_id, message_id, kind, recipe_id) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (chat_id, message_id) DO UPDATE SET kind = EXCLUDED.kind, recipe_id = EXCLUDED.recipe_id",
    )
    .bind(chat_id)
    .bind(i64::from(message_id))
    .bind(kind)
    .bind(recipe_id)
    .execute(pool)
    .await
    .context("Failed to record a chat message")?;
    Ok(())
}

/// Forgets a recorded message: it stays in the chat.
pub async fn forget_chat_message(pool: &PgPool, chat_id: i64, message_id: i32) -> Result<()> {
    sqlx::query("DELETE FROM chat_messages WHERE chat_id = $1 AND message_id = $2")
        .bind(chat_id)
        .bind(i64::from(message_id))
        .execute(pool)
        .await
        .context("Failed to forget a chat message")?;
    Ok(())
}

/// Takes the recorded messages of `chat` of the given kinds, leaving `keep` recorded (a list
/// just turned into a card): they are no longer recorded, and come back oldest first, each
/// telling whether Telegram still allows deleting it.
pub async fn take_chat_messages_except(
    pool: &PgPool,
    chat_id: i64,
    kinds: &[&str],
    keep: Option<i32>,
) -> Result<Vec<ChatMessage>> {
    let kinds: Vec<String> = kinds.iter().map(|kind| (*kind).to_string()).collect();
    let rows = sqlx::query_as::<_, ChatMessage>(
        "DELETE FROM chat_messages WHERE chat_id = $1 AND kind = ANY($2) \
         AND message_id IS DISTINCT FROM $3 \
         RETURNING message_id, kind, recipe_id, sent_at > CURRENT_TIMESTAMP - INTERVAL '48 hours' AS deletable",
    )
    .bind(chat_id)
    .bind(&kinds)
    .bind(keep.map(i64::from))
    .fetch_all(pool)
    .await
    .context("Failed to take chat messages")?;
    let mut messages: Vec<ChatMessage> = rows
        .into_iter()
        .filter(|message| message.message_id > 0)
        .collect();
    messages.sort_by_key(|message| message.message_id);
    Ok(messages)
}

/// The kind a message of `chat` is recorded as, `None` when it is not recorded.
pub async fn chat_message_kind(
    pool: &PgPool,
    chat_id: i64,
    message_id: i32,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT kind FROM chat_messages WHERE chat_id = $1 AND message_id = $2")
        .bind(chat_id)
        .bind(i64::from(message_id))
        .fetch_optional(pool)
        .await
        .context("Failed to look up a chat message")
}

/// The recipe of the newest message of `chat` recorded as `kind` with a recipe (the active
/// card's recipe, the one a unit chooser brings back).
pub async fn latest_recipe_message(pool: &PgPool, chat_id: i64, kind: &str) -> Result<Option<i64>> {
    let recipe: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT recipe_id FROM chat_messages \
         WHERE chat_id = $1 AND kind = $2 AND recipe_id IS NOT NULL \
         ORDER BY message_id DESC LIMIT 1",
    )
    .bind(chat_id)
    .bind(kind)
    .fetch_optional(pool)
    .await
    .context("Failed to look up a chat message")?;
    Ok(recipe.flatten())
}

/// Whether a message of `chat` is recorded.
pub async fn is_chat_message_recorded(
    pool: &PgPool,
    chat_id: i64,
    message_id: i32,
) -> Result<bool> {
    let found: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM chat_messages WHERE chat_id = $1 AND message_id = $2")
            .bind(chat_id)
            .bind(i64::from(message_id))
            .fetch_optional(pool)
            .await
            .context("Failed to look up a chat message")?;
    Ok(found.is_some())
}

/// The recorded messages of `chat` of the given kinds about `recipe_id` (the cards of a recipe),
/// oldest first, still recorded.
pub async fn find_recipe_messages(
    pool: &PgPool,
    chat_id: i64,
    kinds: &[&str],
    recipe_id: i64,
) -> Result<Vec<ChatMessage>> {
    let kinds: Vec<String> = kinds.iter().map(|kind| (*kind).to_string()).collect();
    let rows = sqlx::query_as::<_, ChatMessage>(
        "SELECT message_id, kind, recipe_id, sent_at > CURRENT_TIMESTAMP - INTERVAL '48 hours' AS deletable \
         FROM chat_messages WHERE chat_id = $1 AND kind = ANY($2) AND recipe_id = $3 ORDER BY message_id",
    )
    .bind(chat_id)
    .bind(&kinds)
    .bind(recipe_id)
    .fetch_all(pool)
    .await
    .context("Failed to read the messages of a recipe")?;
    Ok(rows
        .into_iter()
        .filter(|message| message.message_id > 0)
        .collect())
}

/// The messages of `kind` sent more than `older_than_secs` seconds ago, in every chat, with the
/// language of the chat's user (a private chat's id is the user's Telegram id): the token messages
/// that `bot::api_keys` hides after 10 minutes.
pub async fn messages_older_than(
    pool: &PgPool,
    kind: &str,
    older_than_secs: i32,
) -> Result<Vec<(i64, i32, Option<String>)>> {
    let rows: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT m.chat_id, m.message_id, u.language_code FROM chat_messages m \
         LEFT JOIN users u ON u.telegram_id = m.chat_id \
         WHERE m.kind = $1 AND m.sent_at < now() - make_interval(secs => $2) \
         ORDER BY m.sent_at, m.chat_id, m.message_id",
    )
    .bind(kind)
    .bind(f64::from(older_than_secs))
    .fetch_all(pool)
    .await
    .context("Failed to read the messages to hide")?;
    Ok(rows
        .into_iter()
        .filter_map(|(chat, message, language)| {
            i32::try_from(message)
                .ok()
                .map(|message| (chat, message, language))
        })
        .collect())
}

/// Forgets the messages of `kind` sent more than `older_than_secs` seconds ago; returns how many.
pub async fn forget_messages_older_than(
    pool: &PgPool,
    kind: &str,
    older_than_secs: i32,
) -> Result<u64> {
    Ok(sqlx::query(
        "DELETE FROM chat_messages WHERE kind = $1 \
         AND sent_at < now() - make_interval(secs => $2)",
    )
    .bind(kind)
    .bind(f64::from(older_than_secs))
    .execute(pool)
    .await
    .context("Failed to forget old messages")?
    .rows_affected())
}
