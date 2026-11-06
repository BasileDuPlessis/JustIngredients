//! The engine's events in Telegram (`specs/tasks/engine-api.md`, §15b): a change made through
//! another door (an assistant renaming a recipe, writing a memo) updates the recipe's cards in
//! the user's chat, as the bot does at once for its own changes.
//!
//! One consumer, `telegram_cards`, with its cursor in the database: it wakes on each event
//! (`LISTEN`, the notification sent when a change commits) and also every 30 seconds, since a
//! notification can be missed while the listening connection reconnects. Every event is read
//! (the cursor moves after a batch is handled; after a restart, a batch may be read again, which is
//! harmless: an unchanged card is left as it is). A refresh that Telegram refuses (unreachable,
//! too many requests) is not tried again: the card is corrected the next time it is shown or
//! changed. Events are kept 30 days.

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::{PgListener, PgPool};
use teloxide::prelude::*;
use tracing::{debug, warn};

use crate::engine::events::{self, Event, EventKind, Origin};
use crate::localization::LocalizationManager;

/// The consumer's name, its cursor's key.
const CONSUMER: &str = "telegram_cards";

/// Events are also read this often, without a notification.
const POLL_EVERY: Duration = Duration::from_secs(30);

/// Days an event is kept.
const KEEP_DAYS: i32 = 30;

/// Old events are purged this often.
const PURGE_EVERY: Duration = Duration::from_secs(24 * 3600);

/// What an event asks of the chat: the cards of a recipe refreshed, and those of its linked
/// recipes too (they name it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refresh {
    pub recipe_id: i64,
    pub linked_too: bool,
}

/// What `event` asks of the chat; nothing for a change made in Telegram (the bot refreshed it
/// already) or one that shows on no card yet (a recipe created elsewhere is in the list).
pub fn refresh_for(event: &Event) -> Option<Refresh> {
    if event.origin == Origin::Telegram {
        return None;
    }
    let recipe_id = event.recipe_id?;
    match event.kind {
        EventKind::RecipeRenamed => Some(Refresh {
            recipe_id,
            linked_too: true,
        }),
        EventKind::MemoChanged | EventKind::IngredientsChanged => Some(Refresh {
            recipe_id,
            linked_too: false,
        }),
        EventKind::RecipeCreated | EventKind::RecipeDeleted => None,
    }
}

/// Follows the events for as long as the bot runs.
pub async fn run(bot: Bot, pool: Arc<PgPool>, localization: Arc<LocalizationManager>) {
    let mut listener = listen(&pool).await;
    let mut last_purge: Option<tokio::time::Instant> = None;
    loop {
        if last_purge.is_none_or(|at| at.elapsed() >= PURGE_EVERY) {
            match crate::db::purge_events_older_than(&pool, KEEP_DAYS).await {
                Ok(0) => {}
                Ok(purged) => debug!(events = purged, "Old events purged"),
                Err(error) => warn!(error = %error, "Could not purge old events"),
            }
            last_purge = Some(tokio::time::Instant::now());
        }
        if let Err(error) = handle_pending(&bot, &pool, &localization).await {
            warn!(error = %error, "Could not handle the events");
        }
        wait(&mut listener).await;
    }
}

/// A listener on the events' channel; `None` when it cannot be opened (the poll still works).
async fn listen(pool: &PgPool) -> Option<PgListener> {
    let mut listener = match PgListener::connect_with(pool).await {
        Ok(listener) => listener,
        Err(error) => {
            warn!(error = %error, "Could not listen to the events: polling only");
            return None;
        }
    };
    match listener.listen(crate::db::EVENTS_CHANNEL).await {
        Ok(()) => Some(listener),
        Err(error) => {
            warn!(error = %error, "Could not listen to the events: polling only");
            None
        }
    }
}

/// Until a notification, or [`POLL_EVERY`].
async fn wait(listener: &mut Option<PgListener>) {
    let Some(active) = listener.as_mut() else {
        tokio::time::sleep(POLL_EVERY).await;
        return;
    };
    match tokio::time::timeout(POLL_EVERY, active.recv()).await {
        Ok(Ok(_)) | Err(_) => {}
        Ok(Err(error)) => {
            // It reconnects on the next call; meanwhile no busy loop
            debug!(error = %error, "The events listener lost its connection");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Handles every event after the cursor, then moves it.
async fn handle_pending(
    bot: &Bot,
    pool: &PgPool,
    localization: &Arc<LocalizationManager>,
) -> Result<(), crate::engine::EngineError> {
    let mut cursor = events::cursor(pool, CONSUMER).await?;
    loop {
        let (batch, last) = events::after(pool, cursor).await?;
        if last == cursor {
            return Ok(());
        }
        for event in &batch {
            if let Some(refresh) = refresh_for(event) {
                apply(bot, pool, localization, event, refresh).await;
            }
        }
        events::advance(pool, CONSUMER, last).await?;
        cursor = last;
    }
}

async fn apply(
    bot: &Bot,
    pool: &PgPool,
    localization: &Arc<LocalizationManager>,
    event: &Event,
    refresh: Refresh,
) {
    let telegram_id = event.owner.telegram_id();
    // A private chat's id is its user's
    let chat_id = ChatId(telegram_id);
    let lang = crate::engine::language_of(pool, event.owner)
        .await
        .unwrap_or_default();
    let language_code = Some(if lang.is_french() { "fr" } else { "en" });
    super::cards::refresh_cards(
        bot,
        pool,
        chat_id,
        telegram_id,
        refresh.recipe_id,
        language_code,
        localization,
    )
    .await;
    if refresh.linked_too {
        let linked = super::cards::linked_ids(pool, telegram_id, refresh.recipe_id).await;
        super::cards::refresh_linked_cards(
            bot,
            pool,
            chat_id,
            telegram_id,
            &linked,
            language_code,
            localization,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Owner;

    fn event(kind: EventKind, origin: Origin) -> Event {
        Event {
            id: 1,
            owner: Owner::telegram(42),
            kind,
            recipe_id: Some(7),
            origin,
            token_id: None,
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn only_changes_from_other_doors_refresh_the_cards() {
        let refresh = |kind, origin| refresh_for(&event(kind, origin));
        assert_eq!(
            refresh(EventKind::RecipeRenamed, Origin::Mcp),
            Some(Refresh {
                recipe_id: 7,
                linked_too: true
            })
        );
        assert_eq!(
            refresh(EventKind::MemoChanged, Origin::Mcp),
            Some(Refresh {
                recipe_id: 7,
                linked_too: false
            })
        );
        assert!(refresh(EventKind::IngredientsChanged, Origin::Mcp).is_some());
        for kind in [
            EventKind::RecipeRenamed,
            EventKind::MemoChanged,
            EventKind::IngredientsChanged,
        ] {
            assert_eq!(
                refresh(kind, Origin::Telegram),
                None,
                "the bot refreshed its own change"
            );
        }
        assert_eq!(refresh(EventKind::RecipeCreated, Origin::Mcp), None);
        assert_eq!(refresh(EventKind::RecipeDeleted, Origin::Mcp), None);
    }
}
