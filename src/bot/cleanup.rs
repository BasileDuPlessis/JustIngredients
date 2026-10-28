//! Cleanup of the messages an action leaves behind (UX plan, Tasks U1.5 and U5.1).
//!
//! While an action runs (creating, renaming or editing a recipe), the bot prompts, the user
//! replies and the reviewed photo are recorded per chat. When the action ends, the home message
//! deletes them, as well as the previous home message, so the chat keeps only the results
//! (saved or cancelled notice, recipe card) and one home message at the bottom.
//!
//! Only recorded messages are ever deleted. The records are message ids in the `chat_messages`
//! table (never their content), so a deploy or a restart does not forget them. Telegram refuses
//! to delete messages older than 48 hours: those are just forgotten. A failing database never
//! fails the action: the messages are then left in the chat.

use std::sync::{Arc, OnceLock};

use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::MessageId;
use tracing::debug;

/// A prompt, a reply, the reviewed photo or list: deleted when the action ends.
const TRANSIENT: &str = "transient";
/// The home message: deleted when the next action ends.
const HOME: &str = "home";

/// `deleteMessages` accepts at most 100 ids per call.
const DELETE_BATCH: usize = 100;

static POOL: OnceLock<Arc<PgPool>> = OnceLock::new();

/// Gives the cleanup its database, once at startup. Until then (tests) nothing is recorded.
pub fn init(pool: Arc<PgPool>) {
    let _ = POOL.set(pool);
}

fn pool() -> Option<&'static PgPool> {
    POOL.get().map(|pool| pool.as_ref())
}

async fn record(chat: ChatId, id: MessageId, kind: &str) {
    let Some(pool) = pool() else { return };
    if let Err(e) = crate::db::record_chat_message(pool, chat.0, id.0, kind, None).await {
        debug!(error = %e, "Could not record a message of the action");
    }
}

async fn take(chat: ChatId, kinds: &[&str]) -> Vec<MessageId> {
    let Some(pool) = pool() else {
        return Vec::new();
    };
    match crate::db::take_chat_messages(pool, chat.0, kinds).await {
        Ok(messages) => messages
            .into_iter()
            .filter(|message| message.deletable)
            .map(|message| MessageId(message.message_id))
            .collect(),
        Err(e) => {
            debug!(error = %e, "Could not read the messages of the action");
            Vec::new()
        }
    }
}

/// Records a message of the current action, deleted when the action ends.
pub async fn track(chat: ChatId, id: MessageId) {
    record(chat, id, TRANSIENT).await;
}

/// Keeps a recorded message: it is no longer deleted when the action ends.
pub async fn untrack(chat: ChatId, id: MessageId) {
    let Some(pool) = pool() else { return };
    if let Err(e) = crate::db::forget_chat_message(pool, chat.0, id.0).await {
        debug!(error = %e, "Could not forget a message of the action");
    }
}

/// Sends a prompt or a "try again" message of the current action, deleted when it ends.
pub async fn send_transient(bot: &Bot, chat: ChatId, text: String) -> anyhow::Result<Message> {
    let sent = bot.send_message(chat, text).await?;
    track(chat, sent.id).await;
    Ok(sent)
}

/// Deletes the messages of the action that ends and the previous home message, before a new
/// home message is sent; failures are only logged.
pub async fn delete_tracked(bot: &Bot, chat: ChatId) {
    let ids = take(chat, &[TRANSIENT, HOME]).await;
    delete(bot, chat, ids).await;
}

/// Deletes the messages of an action that a command ends; the previous home message stays.
pub async fn delete_action_messages(bot: &Bot, chat: ChatId) {
    let ids = take(chat, &[TRANSIENT]).await;
    delete(bot, chat, ids).await;
}

async fn delete(bot: &Bot, chat: ChatId, ids: Vec<MessageId>) {
    for batch in ids.chunks(DELETE_BATCH) {
        if let Err(e) = bot.delete_messages(chat, batch.to_vec()).await {
            debug!(error = %e, count = batch.len(), "Could not delete the messages of the action");
        }
    }
}

/// Remembers the home message just sent.
pub async fn set_home(chat: ChatId, id: MessageId) {
    record(chat, id, HOME).await;
}
