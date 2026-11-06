//! Personal tokens: how a door other than Telegram knows whose recipes a request reads
//! (`specs/tasks/engine-api.md`, §4). A user creates them in the bot (`/cle`).
//!
//! A token is a prefix (`ji_`, `ji_stg_` on staging) and 32 random bytes in base64url (43
//! characters). Only its SHA-256 is stored: a 256-bit random secret needs no slow hash, and the
//! lookup is by hash. Its first characters (the hint) let the user recognise it. A presented
//! token is checked for its shape first, without touching the database.

use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::{EngineError, Owner};

/// Active tokens a user may have at once.
pub const MAX_ACTIVE: i64 = 5;

/// Reading recipes: every token allows it.
pub const SCOPE_RECIPES_READ: &str = "recipes:read";

/// Changing recipes (rename, memo, create from text): a token allows it when it was created
/// "lecture et écriture".
pub const SCOPE_RECIPES_WRITE: &str = "recipes:write";

/// What a token allows, chosen when it is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
}

impl Access {
    fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::ReadOnly => &[SCOPE_RECIPES_READ],
            Self::ReadWrite => &[SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE],
        }
    }
}

/// A token's use is noted at most this often.
const TOUCH_EVERY_SECS: i32 = 300;

/// Characters of the random part shown in the hint.
const HINT_CHARS: usize = 6;

/// Length of the random part: 32 bytes in base64url without padding.
const SECRET_CHARS: usize = 43;

/// Where tokens are made: production or staging, told apart by their prefix so that one is never
/// tried on the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Realm {
    #[default]
    Production,
    Staging,
}

impl Realm {
    /// Staging when the Fly app's name says so (`just-ingredients-staging`).
    pub fn from_app_name(app_name: Option<&str>) -> Self {
        if app_name.is_some_and(|name| name.ends_with("-staging")) {
            Self::Staging
        } else {
            Self::Production
        }
    }

    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Production => "ji_",
            Self::Staging => "ji_stg_",
        }
    }
}

/// A token just created: its id, the secret, shown once and never again, and its hint. Its
/// `Debug` leaves the secret out, so that no log can hold it.
#[derive(Clone, PartialEq, Eq)]
pub struct NewToken {
    pub id: i64,
    pub secret: String,
    pub hint: String,
}

impl std::fmt::Debug for NewToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewToken")
            .field("id", &self.id)
            .field("hint", &self.hint)
            .finish_non_exhaustive()
    }
}

/// An active token as the bot lists it: never its secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub id: i64,
    pub hint: String,
    /// It may change recipes, not only read them.
    pub can_write: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Who a valid token speaks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub owner: Owner,
    pub token_id: i64,
    pub scopes: Vec<String>,
}

impl Authenticated {
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes.iter().any(|allowed| allowed == scope)
    }
}

/// Creates a token for `owner`, reading recipes only. [`EngineError::LimitReached`] when the
/// owner already has [`MAX_ACTIVE`] active tokens.
pub async fn create(pool: &PgPool, owner: Owner, realm: Realm) -> Result<NewToken, EngineError> {
    create_with(pool, owner, realm, Access::ReadOnly).await
}

/// [`create`], allowing `access`.
pub async fn create_with(
    pool: &PgPool,
    owner: Owner,
    realm: Realm,
    access: Access,
) -> Result<NewToken, EngineError> {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    let secret = format!(
        "{}{}",
        realm.prefix(),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
    );
    let hint = hint_of(&secret, realm);
    let created = crate::db::create_api_token(
        pool,
        owner.telegram_id(),
        &hash(&secret),
        &hint,
        access.scopes(),
        MAX_ACTIVE,
    )
    .await?;
    match created {
        Some(id) => Ok(NewToken { id, secret, hint }),
        None => Err(EngineError::LimitReached),
    }
}

/// The owner's active tokens, oldest first.
pub async fn list(pool: &PgPool, owner: Owner) -> Result<Vec<TokenInfo>, EngineError> {
    Ok(crate::db::list_api_tokens(pool, owner.telegram_id())
        .await?
        .into_iter()
        .map(|row| TokenInfo {
            id: row.id,
            hint: row.hint,
            can_write: row.scopes.iter().any(|scope| scope == SCOPE_RECIPES_WRITE),
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
        .collect())
}

/// Revokes a token of the owner; [`EngineError::NotFound`] when it is not one of the owner's
/// active tokens.
pub async fn revoke(pool: &PgPool, owner: Owner, token_id: i64) -> Result<(), EngineError> {
    if crate::db::revoke_api_token(pool, owner.telegram_id(), token_id).await? {
        Ok(())
    } else {
        Err(EngineError::NotFound)
    }
}

/// The owner a presented token speaks for: `None` for anything that is not an active token of
/// `realm` (wrong shape, unknown, revoked). Its use is noted.
pub async fn authenticate(
    pool: &PgPool,
    realm: Realm,
    presented: &str,
) -> Result<Option<Authenticated>, EngineError> {
    if !has_token_shape(presented, realm) {
        return Ok(None);
    }
    let Some(found) = crate::db::find_api_token(pool, &hash(presented)).await? else {
        return Ok(None);
    };
    // Bookkeeping: a failure to note the use does not refuse a valid token
    if let Err(error) = crate::db::touch_api_token(pool, found.id, TOUCH_EVERY_SECS).await {
        tracing::warn!(error = %error, token_id = found.id, "Could not note a token's use");
    }
    Ok(Some(Authenticated {
        owner: Owner::telegram(found.telegram_id),
        token_id: found.id,
        scopes: found.scopes,
    }))
}

/// Whether `text` has the shape of a token of `realm`: its prefix, then 43 base64url characters.
pub fn has_token_shape(text: &str, realm: Realm) -> bool {
    // "ji_" is also the start of "ji_stg_": a staging token stripped of "ji_" is 4 characters too
    // long, so the length alone tells them apart (a production token may start with "stg_")
    let Some(secret) = text.strip_prefix(realm.prefix()) else {
        return false;
    };
    secret.len() == SECRET_CHARS
        && secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// The beginning of a token of `realm` that the bot shows: its prefix and 6 characters.
fn hint_of(secret: &str, realm: Realm) -> String {
    let prefix = realm.prefix();
    let random: String = secret
        .strip_prefix(prefix)
        .unwrap_or(secret)
        .chars()
        .take(HINT_CHARS)
        .collect();
    format!("{prefix}{random}")
}

fn hash(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(realm: Realm) -> String {
        format!("{}{}", realm.prefix(), "A".repeat(SECRET_CHARS))
    }

    #[test]
    fn only_a_token_of_the_realm_has_the_shape() {
        assert!(has_token_shape(
            &token(Realm::Production),
            Realm::Production
        ));
        assert!(has_token_shape(&token(Realm::Staging), Realm::Staging));
        assert!(
            !has_token_shape(&token(Realm::Staging), Realm::Production),
            "a staging token is refused in production"
        );
        assert!(!has_token_shape(&token(Realm::Production), Realm::Staging));
        let production_with_stg = format!("ji_stg_{}", "A".repeat(SECRET_CHARS - 4));
        assert!(
            has_token_shape(&production_with_stg, Realm::Production),
            "a production token whose random part starts with stg_"
        );
        assert!(!has_token_shape(&production_with_stg, Realm::Staging));
        let refused = [
            String::new(),
            "ji_".to_string(),
            format!("ji_{}", "A".repeat(42)),
            format!("ji_{}", "A".repeat(44)),
            format!("ji_{}=", "A".repeat(42)),
            format!("ji_{}é", "A".repeat(42)),
            format!("JI_{}", "A".repeat(43)),
            format!("Bearer ji_{}", "A".repeat(43)),
            format!("ji_{} ", "A".repeat(42)),
        ];
        for text in refused {
            assert!(!has_token_shape(&text, Realm::Production), "{text:?}");
        }
    }

    #[test]
    fn the_hint_is_the_prefix_and_six_characters() {
        assert_eq!(hint_of("ji_AbCdEf123", Realm::Production), "ji_AbCdEf");
        assert_eq!(hint_of("ji_stg_AbCdEf123", Realm::Staging), "ji_stg_AbCdEf");
        assert_eq!(
            hint_of("ji_stg_Ab123", Realm::Production),
            "ji_stg_Ab",
            "a production token that starts with stg_ is still production's"
        );
    }

    #[test]
    fn a_new_token_never_shows_its_secret_in_debug() {
        let token = NewToken {
            id: 7,
            secret: "ji_TOPSECRET".to_string(),
            hint: "ji_TOPSEC".to_string(),
        };
        assert!(!format!("{token:?}").contains("TOPSECRET"));
    }

    #[test]
    fn the_realm_comes_from_the_app_name() {
        assert_eq!(
            Realm::from_app_name(Some("just-ingredients")),
            Realm::Production
        );
        assert_eq!(
            Realm::from_app_name(Some("just-ingredients-staging")),
            Realm::Staging
        );
        assert_eq!(Realm::from_app_name(None), Realm::Production);
    }

    #[test]
    fn a_hash_is_the_sha256_of_the_whole_token() {
        assert_eq!(hash("ji_x").len(), 32);
        assert_ne!(hash("ji_x"), hash("ji_y"));
        assert_eq!(hash("ji_x"), hash("ji_x"));
    }
}
