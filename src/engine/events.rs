//! The engine's events (`specs/tasks/engine-api.md`, §8): what changed in a user's recipes, and
//! through which door. Written by [`super::writes`] in the transaction of each change (the
//! outbox, `db::events`), read in order by consumers that each keep a cursor. Thin: an event
//! names what changed; a consumer reads the current state (only a deleted recipe's name and links
//! travel with its event, since they cannot be read afterwards).

use serde_json::Value;
use sqlx::PgPool;

use super::{EngineError, Owner};

/// Through which door a change came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Telegram,
    Mcp,
}

impl Origin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Mcp => "mcp",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "telegram" => Some(Self::Telegram),
            "mcp" => Some(Self::Mcp),
            _ => None,
        }
    }
}

/// What changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    RecipeCreated,
    RecipeRenamed,
    MemoChanged,
    IngredientsChanged,
    RecipeDeleted,
}

impl EventKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecipeCreated => "recipe.created",
            Self::RecipeRenamed => "recipe.renamed",
            Self::MemoChanged => "recipe.memo_changed",
            Self::IngredientsChanged => "recipe.ingredients_changed",
            Self::RecipeDeleted => "recipe.deleted",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::RecipeCreated,
            Self::RecipeRenamed,
            Self::MemoChanged,
            Self::IngredientsChanged,
            Self::RecipeDeleted,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }
}

/// An event as a consumer reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub owner: Owner,
    pub kind: EventKind,
    pub recipe_id: Option<i64>,
    pub origin: Origin,
    pub token_id: Option<i64>,
    pub payload: Value,
}

/// How many events a consumer reads at a time.
pub const BATCH: i64 = 100;

/// The events after `cursor`, in order, at most [`BATCH`]. An event of a kind or origin this
/// version does not know (written by a newer one) is skipped, with its id still advancing the
/// cursor: returns the events and the last id read.
pub async fn after(pool: &PgPool, cursor: i64) -> Result<(Vec<Event>, i64), EngineError> {
    let rows = crate::db::events_after(pool, cursor, BATCH).await?;
    let last = rows.last().map_or(cursor, |row| row.id);
    let events = rows
        .into_iter()
        .filter_map(|row| {
            Some(Event {
                id: row.id,
                owner: Owner::telegram(row.telegram_id),
                kind: EventKind::parse(&row.kind)?,
                recipe_id: row.recipe_id,
                origin: Origin::parse(&row.origin)?,
                token_id: row.token_id,
                payload: row.payload,
            })
        })
        .collect();
    Ok((events, last))
}

/// Where `consumer` is. A new consumer starts at the last event written: it handles what
/// happens from now on.
pub async fn cursor(pool: &PgPool, consumer: &str) -> Result<i64, EngineError> {
    // A cursor at 0 is a consumer that started before any event: it reads from the first one
    if let Some(cursor) = crate::db::event_cursor(pool, consumer).await? {
        return Ok(cursor);
    }
    let last = crate::db::last_event_id(pool).await?;
    crate::db::set_event_cursor(pool, consumer, last).await?;
    Ok(last)
}

/// Notes that `consumer` handled the events up to `last_id`.
pub async fn advance(pool: &PgPool, consumer: &str, last_id: i64) -> Result<(), EngineError> {
    Ok(crate::db::set_event_cursor(pool, consumer, last_id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_origins_read_back() {
        for kind in [
            EventKind::RecipeCreated,
            EventKind::RecipeRenamed,
            EventKind::MemoChanged,
            EventKind::IngredientsChanged,
            EventKind::RecipeDeleted,
        ] {
            assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EventKind::parse("recipe.teleported"), None);
        assert_eq!(Origin::parse("mcp"), Some(Origin::Mcp));
        assert_eq!(Origin::parse("fax"), None);
    }
}
