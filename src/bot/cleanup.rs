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
/// A recipe card in the history, without buttons, kept current (U5.4, U6.1).
const CARD: &str = "card";
/// The recipe card at the bottom, with its buttons; at most one per chat (U6.1).
const ACTIVE_CARD: &str = "active_card";

/// What a list older than 48 hours becomes, Telegram no longer letting the bot delete it.
const OLD_LIST_LINE: &str = "📚 ⬇️";

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
    take_except(chat, kinds, None).await
}

async fn take_except(
    chat: ChatId,
    kinds: &[&str],
    keep: Option<MessageId>,
) -> Vec<crate::db::ChatMessage> {
    let Some(pool) = pool() else {
        return Vec::new();
    };
    crate::db::take_chat_messages_except(pool, chat.0, kinds, keep.map(|id| id.0))
        .await
        .unwrap_or_else(|e| {
            debug!(error = %e, "Could not read the messages of the action");
            Vec::new()
        })
}

/// Deletes the messages Telegram still allows deleting. An older list or home message shrinks to
/// one line without buttons, and any other older message loses its buttons (a bot can edit its
/// messages at any time), so no forgotten button stays usable. The active card is never
/// deleted: it loses its buttons and goes into the history.
async fn delete_or_strip(bot: &Bot, chat: ChatId, messages: Vec<crate::db::ChatMessage>) {
    let mut ids = Vec::new();
    for message in messages {
        let id = MessageId(message.message_id);
        if message.kind == ACTIVE_CARD {
            if let Err(e) = bot.edit_message_reply_markup(chat, id).await {
                debug!(error = %e, "Could not remove the buttons of the card");
            }
            if let Some(recipe_id) = message.recipe_id {
                record_card_as(chat, id, recipe_id, CARD).await;
            }
        } else if message.deletable {
            ids.push(id);
        } else if message.kind == LIST || message.kind == HOME {
            // An old list shrinks to one line without buttons: the new one is below
            if let Err(e) = bot.edit_message_text(chat, id, OLD_LIST_LINE).await {
                debug!(error = %e, "Could not shrink an old list");
            }
        } else if let Err(e) = bot.edit_message_reply_markup(chat, id).await {
            // An old prompt, answer or confirmation (e.g. "🗑️ Oui, supprimer") or card
            debug!(error = %e, "Could not remove the buttons of an old message");
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
    let messages = take(chat, &[TRANSIENT, HOME, LIST, ACTIVE_CARD]).await;
    delete_or_strip(bot, chat, messages).await;
}

/// Clears the bottom of the chat for a new message with buttons: the list goes (shrunk when
/// too old) and the active card goes into the history without its buttons. The messages of the
/// action stay (e.g. the welcome that `/start` shows).
pub async fn delete_lists(bot: &Bot, chat: ChatId) {
    let messages = take(chat, &[HOME, LIST, ACTIVE_CARD]).await;
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

async fn record_card_as(chat: ChatId, id: MessageId, recipe_id: i64, kind: &str) {
    let Some(pool) = pool() else { return };
    if let Err(e) = crate::db::record_chat_message(pool, chat.0, id.0, kind, Some(recipe_id)).await
    {
        debug!(error = %e, "Could not record a recipe card");
    }
}

/// Whether `id` is the recorded list (or home message) at the bottom: a recipe opened from it
/// can take its place.
pub async fn is_list(chat: ChatId, id: MessageId) -> bool {
    let Some(pool) = pool() else { return false };
    matches!(
        crate::db::chat_message_kind(pool, chat.0, id.0).await,
        Ok(Some(kind)) if kind == LIST || kind == HOME
    )
}

/// The list `id` was just edited into the card of `recipe_id`: recorded as the active card
/// (keeping its send time), and anything else with buttons at the bottom goes as with
/// [`delete_lists`].
pub async fn list_became_card(bot: &Bot, chat: ChatId, id: MessageId, recipe_id: i64) {
    record_active_card(chat, id, recipe_id).await;
    let messages = take_except(chat, &[HOME, LIST, ACTIVE_CARD], Some(id)).await;
    delete_or_strip(bot, chat, messages).await;
}

/// The recipe to show again after the unit chooser: the card at the bottom with its buttons,
/// or the one an earlier chooser still at the bottom remembered (`/unites` typed twice).
pub async fn recipe_at_bottom(chat: ChatId) -> Option<i64> {
    let pool = pool()?;
    for kind in [ACTIVE_CARD, TRANSIENT] {
        match crate::db::latest_recipe_message(pool, chat.0, kind).await {
            Ok(Some(recipe)) => return Some(recipe),
            Ok(None) => {}
            Err(e) => debug!(error = %e, "Could not read the recipe at the bottom"),
        }
    }
    None
}

/// Records the unit chooser of the current action, with the recipe it brings back.
pub async fn track_chooser(chat: ChatId, id: MessageId, recipe_id: Option<i64>) {
    let Some(pool) = pool() else { return };
    if let Err(e) = crate::db::record_chat_message(pool, chat.0, id.0, TRANSIENT, recipe_id).await {
        debug!(error = %e, "Could not record the unit chooser");
    }
}

/// Remembers the card just sent at the bottom with its buttons.
pub async fn record_active_card(chat: ChatId, id: MessageId, recipe_id: i64) {
    record_card_as(chat, id, recipe_id, ACTIVE_CARD).await;
}

/// Gives a history card of a recipe its buttons back and makes it the active card: `card`, or
/// the newest recorded card of the recipe. Returns whether it worked.
pub async fn reactivate_card(
    bot: &Bot,
    chat: ChatId,
    recipe_id: i64,
    card: Option<MessageId>,
    keyboard: teloxide::types::InlineKeyboardMarkup,
) -> bool {
    let card = match card {
        Some(card) => Some(card),
        None => recipe_cards(chat, recipe_id)
            .await
            .last()
            .map(|card| card.id),
    };
    let Some(card) = card else {
        return false;
    };
    match bot
        .edit_message_reply_markup(chat, card)
        .reply_markup(keyboard)
        .await
    {
        Ok(_) => {
            record_active_card(chat, card, recipe_id).await;
            true
        }
        Err(e) => {
            debug!(error = %e, "Could not make the card active again");
            false
        }
    }
}

/// A recorded card of a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeCard {
    pub id: MessageId,
    /// Telegram still lets the bot delete it.
    pub deletable: bool,
    /// The card at the bottom, with its buttons.
    pub active: bool,
}

/// The recorded cards of a recipe, in the history and at the bottom, oldest first.
pub async fn recipe_cards(chat: ChatId, recipe_id: i64) -> Vec<RecipeCard> {
    let Some(pool) = pool() else {
        return Vec::new();
    };
    match crate::db::find_recipe_messages(pool, chat.0, &[CARD, ACTIVE_CARD], recipe_id).await {
        Ok(cards) => cards
            .into_iter()
            .map(|card| RecipeCard {
                id: MessageId(card.message_id),
                deletable: card.deletable,
                active: card.kind == ACTIVE_CARD,
            })
            .collect(),
        Err(e) => {
            debug!(error = %e, "Could not read the cards of a recipe");
            Vec::new()
        }
    }
}

/// Removes the recorded cards of a recipe (except `keep`): deleted, or stripped of their
/// buttons when Telegram no longer allows deleting them, and forgotten.
pub async fn remove_recipe_cards(bot: &Bot, chat: ChatId, recipe_id: i64, keep: Option<MessageId>) {
    for card in recipe_cards(chat, recipe_id).await {
        if Some(card.id) == keep {
            continue;
        }
        untrack(chat, card.id).await;
        let message = crate::db::ChatMessage {
            message_id: card.id.0,
            kind: CARD.to_string(),
            recipe_id: Some(recipe_id),
            deletable: card.deletable,
        };
        delete_or_strip(bot, chat, vec![message]).await;
    }
}

/// A list or home message of a version before U5, whose button was just pressed: it was never
/// recorded, so it would stay above the new list. Deleted, or emptied when too old. A recorded
/// message is left to the cleanup.
pub async fn remove_legacy_message(bot: &Bot, chat: ChatId, id: MessageId) {
    let Some(pool) = pool() else { return };
    if !matches!(
        crate::db::is_chat_message_recorded(pool, chat.0, id.0).await,
        Ok(false)
    ) {
        return;
    }
    if bot.delete_message(chat, id).await.is_err() {
        if let Err(e) = bot.edit_message_text(chat, id, OLD_LIST_LINE).await {
            debug!(error = %e, "Could not remove an old list");
        }
    }
}

/// Whether a message is recorded; `None` when the database cannot tell.
pub async fn is_recorded(chat: ChatId, id: MessageId) -> Option<bool> {
    let pool = pool()?;
    crate::db::is_chat_message_recorded(pool, chat.0, id.0)
        .await
        .ok()
}

/// A card of a version before U5.4 whose action button was just pressed: recorded as the active
/// card, so the action puts it into the history and "❌ Annuler" brings it back.
pub async fn adopt_card(chat: ChatId, id: MessageId, recipe_id: i64) {
    if is_recorded(chat, id).await == Some(false) {
        record_active_card(chat, id, recipe_id).await;
    }
}

/// A list of a version before U5 being paged: recorded as a list from now on, so the next
/// action's cleanup takes it with the current one.
pub async fn adopt_legacy_list(chat: ChatId, id: MessageId) {
    let Some(pool) = pool() else { return };
    if matches!(
        crate::db::is_chat_message_recorded(pool, chat.0, id.0).await,
        Ok(false)
    ) {
        record(chat, id, LIST).await;
    }
}

/// Remembers the recipe list just sent at the bottom.
pub async fn set_list(chat: ChatId, id: MessageId) {
    record(chat, id, LIST).await;
}
