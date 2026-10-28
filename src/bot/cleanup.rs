//! Cleanup of the messages an action leaves behind (UX plan, Tasks U1.5 and U5.1).
//!
//! While an action runs (creating, renaming or editing a recipe), the bot prompts, the user
//! replies and the reviewed photo are recorded per chat. When the action ends they are deleted,
//! with the previous recipe list, and the list is sent again at the bottom (`home::send_home`), so
//! the chat keeps only the results (saved or cancelled notice, recipe card) and one list, last.
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
/// The home message of versions before U5.2: deleted (or stripped) when the next action ends.
const HOME: &str = "home";
/// The recipe list at the bottom: replaced when the next action ends.
const LIST: &str = "list";

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

async fn take(chat: ChatId, kinds: &[&str]) -> Vec<crate::db::ChatMessage> {
    let Some(pool) = pool() else {
        return Vec::new();
    };
    crate::db::take_chat_messages(pool, chat.0, kinds)
        .await
        .unwrap_or_else(|e| {
            debug!(error = %e, "Could not read the messages of the action");
            Vec::new()
        })
}

/// Deletes the messages Telegram still allows deleting. An older list or home message only
/// loses its buttons (a bot can edit its messages at any time); older prompts stay as they are.
async fn delete_or_strip(bot: &Bot, chat: ChatId, messages: Vec<crate::db::ChatMessage>) {
    let mut ids = Vec::new();
    for message in messages {
        let id = MessageId(message.message_id);
        if message.deletable {
            ids.push(id);
        } else if message.kind == LIST || message.kind == HOME {
            if let Err(e) = bot.edit_message_reply_markup(chat, id).await {
                debug!(error = %e, "Could not remove the buttons of an old list");
            }
        }
    }
    delete(bot, chat, ids).await;
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

/// Deletes the messages of the action that ends and the previous list, before the list is sent
/// again at the bottom; failures are only logged.
pub async fn delete_tracked(bot: &Bot, chat: ChatId) {
    let messages = take(chat, &[TRANSIENT, HOME, LIST]).await;
    delete_or_strip(bot, chat, messages).await;
}

/// Deletes the previous list only: the messages of the action stay (e.g. the welcome that
/// `/start` shows above the list).
pub async fn delete_lists(bot: &Bot, chat: ChatId) {
    let messages = take(chat, &[HOME, LIST]).await;
    delete_or_strip(bot, chat, messages).await;
}

/// Deletes the messages of an action that a command ends; the list stays.
pub async fn delete_action_messages(bot: &Bot, chat: ChatId) {
    let messages = take(chat, &[TRANSIENT]).await;
    delete_or_strip(bot, chat, messages).await;
}

async fn delete(bot: &Bot, chat: ChatId, ids: Vec<MessageId>) {
    for batch in ids.chunks(DELETE_BATCH) {
        if let Err(e) = bot.delete_messages(chat, batch.to_vec()).await {
            debug!(error = %e, count = batch.len(), "Could not delete the messages of the action");
        }
    }
}

/// Remembers the recipe list just sent at the bottom.
pub async fn set_list(chat: ChatId, id: MessageId) {
    record(chat, id, LIST).await;
}
