//! Cleanup of the messages an action leaves behind (UX plan, Task U1.5).
//!
//! While an action runs (creating, renaming or editing a recipe), the bot prompts and the user
//! replies are recorded per chat. When the action ends, the home message deletes them, as well
//! as the previous home message, so the chat keeps only the results (photo, ingredient list,
//! saved or cancelled notice, recipe card) and one home message at the bottom.
//!
//! Only recorded messages are ever deleted. Like the dialogue state, the records live in memory
//! and are lost on restart; the messages are then simply left in the chat.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use teloxide::prelude::*;
use teloxide::types::MessageId;
use tracing::debug;

/// Telegram refuses to delete messages older than 48 hours.
const MAX_AGE: Duration = Duration::from_secs(48 * 60 * 60);

/// Recorded messages kept per chat; the oldest are dropped beyond this.
const MAX_PER_CHAT: usize = 200;

/// `deleteMessages` accepts at most 100 ids per call.
const DELETE_BATCH: usize = 100;

#[derive(Debug, Default)]
struct ChatMessages {
    transient: Vec<(MessageId, Instant)>,
    home: Option<(MessageId, Instant)>,
}

/// Messages to delete, per chat. Pure bookkeeping: the clock is passed in, nothing is sent.
#[derive(Debug, Default)]
pub struct Tracker {
    chats: HashMap<ChatId, ChatMessages>,
}

impl Tracker {
    /// Records a prompt or a reply of the current action.
    pub fn track(&mut self, chat: ChatId, id: MessageId, now: Instant) {
        let transient = &mut self.chats.entry(chat).or_default().transient;
        if !transient.iter().any(|(tracked, _)| *tracked == id) {
            transient.push((id, now));
        }
        if transient.len() > MAX_PER_CHAT {
            let excess = transient.len() - MAX_PER_CHAT;
            transient.drain(..excess);
        }
    }

    /// Takes the messages to delete when an action ends: its prompts and replies and the
    /// previous home message, except those too old to be deleted.
    pub fn take_for_cleanup(&mut self, chat: ChatId, now: Instant) -> Vec<MessageId> {
        let Some(messages) = self.chats.get_mut(&chat) else {
            return Vec::new();
        };
        let recent =
            |(id, at): (MessageId, Instant)| (now.duration_since(at) < MAX_AGE).then_some(id);
        let mut ids: Vec<MessageId> = messages
            .home
            .take()
            .into_iter()
            .filter_map(recent)
            .collect();
        ids.extend(messages.transient.drain(..).filter_map(recent));
        self.chats.remove(&chat);
        ids
    }

    /// Remembers the home message just sent, deleted when the next action ends.
    pub fn set_home(&mut self, chat: ChatId, id: MessageId, now: Instant) {
        self.chats.entry(chat).or_default().home = Some((id, now));
    }
}

static TRACKER: LazyLock<Mutex<Tracker>> = LazyLock::new(Mutex::default);

fn with_tracker<T>(f: impl FnOnce(&mut Tracker) -> T) -> T {
    // The tracker holds plain data: it stays usable even if a holder panicked
    let mut tracker = TRACKER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut tracker)
}

/// Records a message of the current action, deleted when the action ends.
pub fn track(chat: ChatId, id: MessageId) {
    with_tracker(|tracker| tracker.track(chat, id, Instant::now()));
}

/// Sends a prompt or a "try again" message of the current action, deleted when it ends.
pub async fn send_transient(bot: &Bot, chat: ChatId, text: String) -> anyhow::Result<Message> {
    let sent = bot.send_message(chat, text).await?;
    track(chat, sent.id);
    Ok(sent)
}

/// Deletes the recorded messages of the chat; failures are only logged.
pub async fn delete_tracked(bot: &Bot, chat: ChatId) {
    let ids = with_tracker(|tracker| tracker.take_for_cleanup(chat, Instant::now()));
    for batch in ids.chunks(DELETE_BATCH) {
        if let Err(e) = bot.delete_messages(chat, batch.to_vec()).await {
            debug!(error = %e, count = batch.len(), "Could not delete the messages of the action");
        }
    }
}

/// Remembers the home message just sent.
pub fn set_home(chat: ChatId, id: MessageId) {
    with_tracker(|tracker| tracker.set_home(chat, id, Instant::now()));
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAT: ChatId = ChatId(42);

    #[test]
    fn cleanup_takes_the_previous_home_and_the_action_messages_once() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        tracker.set_home(CHAT, MessageId(1), now);
        tracker.track(CHAT, MessageId(5), now);
        tracker.track(CHAT, MessageId(6), now);
        tracker.track(CHAT, MessageId(6), now);
        tracker.track(ChatId(7), MessageId(9), now);

        assert_eq!(
            tracker.take_for_cleanup(CHAT, now),
            vec![MessageId(1), MessageId(5), MessageId(6)]
        );
        assert!(tracker.take_for_cleanup(CHAT, now).is_empty());
        // Other chats are untouched
        assert_eq!(tracker.take_for_cleanup(ChatId(7), now), vec![MessageId(9)]);
    }

    #[test]
    fn the_new_home_is_deleted_by_the_next_cleanup_only() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        tracker.track(CHAT, MessageId(5), now);
        assert_eq!(tracker.take_for_cleanup(CHAT, now), vec![MessageId(5)]);
        tracker.set_home(CHAT, MessageId(8), now);
        tracker.set_home(CHAT, MessageId(12), now);
        assert_eq!(tracker.take_for_cleanup(CHAT, now), vec![MessageId(12)]);
    }

    #[test]
    fn messages_older_than_48_hours_are_skipped() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        tracker.set_home(CHAT, MessageId(1), start);
        tracker.track(CHAT, MessageId(2), start);
        let later = start + MAX_AGE - Duration::from_secs(60);
        tracker.track(CHAT, MessageId(3), later);
        assert_eq!(
            tracker.take_for_cleanup(CHAT, start + MAX_AGE),
            vec![MessageId(3)]
        );
    }

    #[test]
    fn a_chat_keeps_at_most_the_latest_recorded_messages() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        let total = MAX_PER_CHAT as i32 + 10;
        for id in 0..total {
            tracker.track(CHAT, MessageId(id), now);
        }
        let ids = tracker.take_for_cleanup(CHAT, now);
        assert_eq!(ids.len(), MAX_PER_CHAT);
        assert_eq!(ids.first(), Some(&MessageId(10)));
        assert_eq!(ids.last(), Some(&MessageId(total - 1)));
    }
}
