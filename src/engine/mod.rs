//! The recipe engine: the use cases every door shares, Telegram today, an MCP server and an HTTP
//! API next (`specs/tasks/engine-api.md`). A call takes the [`Owner`] a door authenticated and
//! returns plain values or an [`EngineError`]. The engine knows nothing of Telegram, HTTP or MCP:
//! `tests/architecture_tests.rs` checks what it may use.

pub mod recipes;

/// Whose recipes a call reads or writes: the user a door authenticated (the sender of a Telegram
/// update, later the holder of a token). Opaque, so that owning recipes by `users.id` instead of
/// the Telegram id only changes this type and `db`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    telegram_id: i64,
}

impl Owner {
    /// The user of a Telegram id (in a private chat, the chat id).
    pub const fn telegram(telegram_id: i64) -> Self {
        Self { telegram_id }
    }

    pub(crate) const fn telegram_id(self) -> i64 {
        self.telegram_id
    }
}

/// The language of the texts the engine composes (unit names: "c. à s." or "tbsp"; decimal
/// comma or point): French or English, the two languages of the bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    Fr,
    #[default]
    En,
}

impl Lang {
    /// From a language code as Telegram gives it ("fr", "fr-FR", "en-US"); any other language
    /// is English, as for the bot's messages (`localization::detect_language`).
    pub fn from_code(code: Option<&str>) -> Self {
        match code.and_then(|code| code.split('-').next()) {
            Some("fr") => Self::Fr,
            _ => Self::En,
        }
    }

    pub const fn is_french(self) -> bool {
        matches!(self, Self::Fr)
    }
}

/// Why an engine call gave no result.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// No such recipe for this owner. Another user's recipe is not found either: its existence
    /// is not told.
    #[error("not found")]
    NotFound,
    /// The input is refused, with the reason a user is told.
    #[error("invalid input: {0}")]
    Invalid(crate::errors::ValidationError),
    /// The database or another service failed.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_read_from_telegram_codes() {
        for (code, lang) in [
            (Some("fr"), Lang::Fr),
            (Some("fr-FR"), Lang::Fr),
            (Some("fr-CA"), Lang::Fr),
            (Some("en"), Lang::En),
            (Some("en-US"), Lang::En),
            (Some("de"), Lang::En),
            (Some("FR"), Lang::En),
            (Some(""), Lang::En),
            (None, Lang::En),
        ] {
            assert_eq!(Lang::from_code(code), lang, "{code:?}");
        }
    }
}
