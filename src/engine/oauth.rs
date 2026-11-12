//! OAuth grants (`specs/tasks/oauth.md`, §2 and §7): what lets an assistant the user connected
//! (claude.ai, ChatGPT) act on their recipes, and the sign-in that makes one. The doors check the
//! HTTP side (clients, redirects, pages, Telegram); this module keeps the state and the rules:
//!
//! - a sign-in lives 10 minutes, bound to the browser that started it, consumed once by the
//!   user's decision;
//! - our codes live 60 seconds and are used once; a code presented again revokes its grant;
//! - a grant's access tokens live an hour, its refresh tokens 30 days, renewed at each use (so a
//!   grant used within 30 days never expires); a refresh token is exchanged once, but presented
//!   again within 60 seconds (a retry after a lost answer) it gives a fresh pair; later, it
//!   revokes the grant (it was copied);
//! - one grant per user and client: a new one replaces the previous one.
//!
//! Every secret is random (32 bytes), given once, stored by SHA-256.

use base64::Engine;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use super::tokens::{self, Authenticated, Checked, Credential, Realm};
use super::{EngineError, Owner};
use crate::secret::SecretString;

/// How long a sign-in waits for the user.
pub const REQUEST_LIFETIME_SECS: i64 = 600;
/// How long one of our codes lives.
pub const CODE_LIFETIME_SECS: i64 = 60;
/// How long an access token lives.
pub const ACCESS_LIFETIME_SECS: i64 = 3600;
/// How long a refresh token lives (renewed at each use).
pub const REFRESH_LIFETIME_SECS: i64 = 30 * 24 * 3600;
/// How long after its exchange a refresh token still gives a fresh pair.
pub const REFRESH_GRACE_SECS: i64 = 60;
/// A grant's use is noted at most this often.
const TOUCH_EVERY_SECS: i32 = 300;

/// What a client asked for, once the door checked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInRequest {
    pub client_id: String,
    pub redirect_uri: String,
    /// The client's `state`, given back with the answer.
    pub client_state: Option<String>,
    /// The client's PKCE S256 challenge.
    pub code_challenge: String,
    /// The scopes asked for that we know.
    pub scopes: Vec<String>,
    pub resource: String,
}

/// The secrets of a sign-in with the identity provider: our `state` with it, the browser's
/// binding (its cookie), our PKCE verifier and nonce with it.
#[derive(Debug, Clone)]
pub struct SignInSecrets<'a> {
    pub state: &'a str,
    pub binding: &'a str,
    pub verifier: &'a SecretString,
    pub nonce: &'a str,
}

/// A sign-in waiting for the identity provider's answer.
#[derive(Debug, Clone)]
pub struct PendingSignIn {
    pub id: i64,
    pub request: SignInRequest,
    pub verifier: SecretString,
    pub nonce: String,
}

/// What the provider's answer leads to.
#[derive(Debug, Clone)]
pub enum Callback {
    /// No sign-in waits with this state (expired, already answered, made up).
    Unknown,
    /// The sign-in was started in another browser: its owner may have been sent a crafted link.
    OtherBrowser,
    Pending(PendingSignIn),
}

/// A signed-in request, for the consent page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consent {
    pub request: SignInRequest,
    pub owner: Owner,
}

/// The user's answer on the consent page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Reading, and writing if ticked.
    Allow {
        write: bool,
    },
    Deny,
}

/// A decision taken: where to send the browser, with our code when the user allowed.
#[derive(Debug, Clone)]
pub struct Decided {
    pub request: SignInRequest,
    pub code: Option<SecretString>,
}

/// A code or refresh token exchanged: the new pair and what it allows.
#[derive(Debug, Clone)]
pub struct Issued {
    pub access_token: SecretString,
    pub refresh_token: SecretString,
    pub expires_in: i64,
    pub scopes: Vec<String>,
    pub grant_id: i64,
    pub owner: Owner,
    pub client_id: String,
    /// A new grant (from a code), and whether it replaced one of the same client.
    pub new_grant: Option<NewGrant>,
}

/// A grant just made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewGrant {
    pub replaced: bool,
}

/// Why a code or refresh token gave nothing. The client is told `invalid_grant` in every case;
/// the reason is for logs.
#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    #[error("refused: {0}")]
    Refused(&'static str),
    /// A used code or a rotated refresh token presented again: its grant was revoked.
    #[error("reused, grant {grant_id} revoked")]
    Reused { grant_id: i64 },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// A grant as the bot lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantInfo {
    pub id: i64,
    pub client_id: String,
    pub can_write: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

fn hash(secret: &str) -> Vec<u8> {
    tokens::hash(secret)
}

/// Stores a sign-in for 10 minutes.
pub async fn start_sign_in(
    pool: &PgPool,
    request: &SignInRequest,
    secrets: &SignInSecrets<'_>,
) -> Result<i64, EngineError> {
    Ok(crate::db::insert_oauth_request(
        pool,
        &crate::db::NewOAuthRequest {
            state_hash: &hash(secrets.state),
            binding_hash: &hash(secrets.binding),
            client_id: &request.client_id,
            redirect_uri: &request.redirect_uri,
            client_state: request.client_state.as_deref(),
            code_challenge: &request.code_challenge,
            scopes: &request.scopes,
            resource: &request.resource,
            telegram_verifier: secrets.verifier.expose(),
            telegram_nonce: secrets.nonce,
            lifetime_secs: REQUEST_LIFETIME_SECS,
        },
    )
    .await?)
}

fn request_of(row: &crate::db::OAuthRequestRow) -> SignInRequest {
    SignInRequest {
        client_id: row.client_id.clone(),
        redirect_uri: row.redirect_uri.clone(),
        client_state: row.client_state.clone(),
        code_challenge: row.code_challenge.clone(),
        scopes: row.scopes.clone(),
        resource: row.resource.clone(),
    }
}

/// The sign-in the provider answers for, with `state`, in the browser holding `binding` (none:
/// the cookie did not come back).
pub async fn find_sign_in(
    pool: &PgPool,
    state: &str,
    binding: Option<&str>,
) -> Result<Callback, EngineError> {
    let Some(row) = crate::db::find_oauth_request_by_state(pool, &hash(state)).await? else {
        return Ok(Callback::Unknown);
    };
    let same_browser = binding.is_some_and(|binding| {
        let presented = hash(binding);
        constant_time_eq(&presented, &row.binding_hash)
    });
    if !same_browser {
        return Ok(Callback::OtherBrowser);
    }
    Ok(Callback::Pending(PendingSignIn {
        id: row.id,
        request: request_of(&row),
        verifier: SecretString::new(row.telegram_verifier.clone()),
        nonce: row.telegram_nonce.clone(),
    }))
}

/// Records who signed in; `false` when the sign-in is over or someone already did.
pub async fn signed_in(pool: &PgPool, sign_in: i64, owner: Owner) -> Result<bool, EngineError> {
    Ok(crate::db::set_oauth_request_user(pool, sign_in, owner.telegram_id()).await?)
}

/// Ends a sign-in without a code; the request, for the error sent back to the client.
pub async fn abandon_sign_in(
    pool: &PgPool,
    sign_in: i64,
) -> Result<Option<SignInRequest>, EngineError> {
    Ok(crate::db::abandon_oauth_request(pool, sign_in)
        .await?
        .as_ref()
        .map(request_of))
}

/// The signed-in request of the browser holding `binding`, for the consent page, whose form will
/// carry `consent_token` (a new one at each display).
pub async fn open_consent(
    pool: &PgPool,
    binding: &str,
    consent_token: &str,
) -> Result<Option<Consent>, EngineError> {
    let row = crate::db::prepare_oauth_consent(pool, &hash(binding), &hash(consent_token)).await?;
    Ok(row.and_then(|row| {
        Some(Consent {
            owner: Owner::telegram(row.telegram_id?),
            request: request_of(&row),
        })
    }))
}

/// Takes the user's decision on the consent page shown with `consent_token`, once: a code when
/// allowed. `None` when the page is not (or no longer) the current one of this browser.
pub async fn decide(
    pool: &PgPool,
    binding: &str,
    consent_token: &str,
    decision: Decision,
) -> Result<Option<Decided>, EngineError> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let Some(row) =
        crate::db::consume_oauth_consent(&mut tx, &hash(binding), &hash(consent_token)).await?
    else {
        return Ok(None);
    };
    let request = request_of(&row);
    let code = match (decision, row.telegram_id) {
        (Decision::Allow { write }, Some(telegram_id)) => {
            let mut scopes = vec![tokens::SCOPE_RECIPES_READ.to_string()];
            if write {
                scopes.push(tokens::SCOPE_RECIPES_WRITE.to_string());
            }
            let code = tokens::new_secret("");
            crate::db::insert_oauth_code(
                &mut tx,
                &crate::db::NewOAuthCode {
                    code_hash: &hash(&code),
                    telegram_id,
                    client_id: &request.client_id,
                    redirect_uri: &request.redirect_uri,
                    code_challenge: &request.code_challenge,
                    scopes: &scopes,
                    resource: &request.resource,
                    lifetime_secs: CODE_LIFETIME_SECS,
                },
            )
            .await?;
            Some(SecretString::new(code))
        }
        _ => None,
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Some(Decided { request, code }))
}

/// What a client presents to exchange one of our codes.
#[derive(Debug, Clone, Copy)]
pub struct CodeExchange<'a> {
    pub code: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub verifier: &'a str,
    /// The `resource` parameter, when sent: it must be the code's.
    pub resource: Option<&'a str>,
}

/// Whether a PKCE verifier has the form RFC 7636 asks: 43 to 128 unreserved characters.
pub fn is_pkce_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

/// The S256 challenge of a PKCE verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Exchanges a code for the grant it allows and its first pair of tokens.
pub async fn redeem_code(
    pool: &PgPool,
    realm: Realm,
    exchange: CodeExchange<'_>,
) -> Result<Issued, GrantError> {
    let code_hash = hash(exchange.code);
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let Some(code) = crate::db::use_oauth_code(&mut tx, &code_hash).await? else {
        let revoked = crate::db::revoke_grant_of_used_code(&mut tx, &code_hash).await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        return Err(match revoked {
            Some(grant_id) => GrantError::Reused { grant_id },
            None => GrantError::Refused("unknown or expired code"),
        });
    };
    // The code is used whatever comes next: a wrong verifier burns it
    let refusal = if code.client_id != exchange.client_id {
        Some("another client")
    } else if code.redirect_uri != exchange.redirect_uri {
        Some("another redirect URI")
    } else if !is_pkce_verifier(exchange.verifier)
        || !constant_time_eq(
            pkce_challenge(exchange.verifier).as_bytes(),
            code.code_challenge.as_bytes(),
        )
    {
        Some("wrong PKCE verifier")
    } else if exchange
        .resource
        .is_some_and(|resource| resource != code.resource)
    {
        Some("another resource")
    } else {
        None
    };
    if let Some(reason) = refusal {
        tx.commit().await.map_err(anyhow::Error::from)?;
        return Err(GrantError::Refused(reason));
    }
    let (grant_id, replaced) = crate::db::insert_oauth_grant(&mut tx, &code_hash, &code).await?;
    let telegram_id = crate::db::telegram_id_of_user(&mut tx, code.user_id).await?;
    let (access_token, refresh_token) = issue_pair(&mut tx, realm, grant_id, None).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Issued {
        access_token,
        refresh_token,
        expires_in: ACCESS_LIFETIME_SECS,
        scopes: code.scopes,
        grant_id,
        owner: Owner::telegram(telegram_id),
        client_id: code.client_id,
        new_grant: Some(NewGrant { replaced }),
    })
}

async fn issue_pair(
    conn: &mut sqlx::PgConnection,
    realm: Realm,
    grant_id: i64,
    parent_id: Option<i64>,
) -> Result<(SecretString, SecretString), GrantError> {
    let access = tokens::new_secret(realm.access_prefix());
    let refresh = tokens::new_secret(realm.refresh_prefix());
    crate::db::insert_oauth_token(
        &mut *conn,
        grant_id,
        "access",
        &hash(&access),
        None,
        ACCESS_LIFETIME_SECS,
    )
    .await?;
    crate::db::insert_oauth_token(
        &mut *conn,
        grant_id,
        "refresh",
        &hash(&refresh),
        parent_id,
        REFRESH_LIFETIME_SECS,
    )
    .await?;
    Ok((SecretString::new(access), SecretString::new(refresh)))
}

/// Exchanges a refresh token of `client_id` for a new pair (see the module for the grace
/// period). `allowed` is asked whether the grant's owner may still use the doors (the beta
/// list): if not, the grant is revoked.
pub async fn refresh(
    pool: &PgPool,
    realm: Realm,
    presented: &str,
    client_id: &str,
    allowed: impl Fn(Owner) -> bool,
) -> Result<Issued, GrantError> {
    if !tokens::has_shape(presented, realm.refresh_prefix()) {
        return Err(GrantError::Refused("not a refresh token"));
    }
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let Some(token) =
        crate::db::lock_oauth_refresh(&mut tx, &hash(presented), REFRESH_GRACE_SECS).await?
    else {
        return Err(GrantError::Refused("unknown refresh token"));
    };
    if !token.usable {
        return Err(GrantError::Refused("expired or revoked"));
    }
    if token.client_id != client_id {
        return Err(GrantError::Refused("another client"));
    }
    let owner = Owner::telegram(token.telegram_id);
    if token.rotated && !token.in_grace {
        crate::db::revoke_oauth_grant_by_id(&mut tx, token.grant_id).await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        return Err(GrantError::Reused {
            grant_id: token.grant_id,
        });
    }
    if !allowed(owner) {
        crate::db::revoke_oauth_grant_by_id(&mut tx, token.grant_id).await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        return Err(GrantError::Refused("the user may no longer use the doors"));
    }
    crate::db::rotate_oauth_refresh(&mut tx, token.id).await?;
    let (access_token, refresh_token) =
        issue_pair(&mut tx, realm, token.grant_id, Some(token.id)).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Issued {
        access_token,
        refresh_token,
        expires_in: ACCESS_LIFETIME_SECS,
        scopes: token.scopes,
        grant_id: token.grant_id,
        owner,
        client_id: token.client_id,
        new_grant: None,
    })
}

/// Checks an access token (of the shape of the realm's) for `resource`.
pub(super) async fn authenticate_access(
    pool: &PgPool,
    presented: &str,
    resource: &str,
) -> Result<Checked, EngineError> {
    let Some(found) = crate::db::find_oauth_access(pool, &hash(presented)).await? else {
        return Ok(Checked::Unknown);
    };
    if !found.usable || found.resource != resource {
        return Ok(Checked::Known);
    }
    // Bookkeeping: a failure to note the use does not refuse a valid token
    if let Err(error) = crate::db::touch_oauth_grant(pool, found.grant_id, TOUCH_EVERY_SECS).await {
        tracing::warn!(error = %error, grant_id = found.grant_id, "Could not note a grant's use");
    }
    Ok(Checked::Valid(Authenticated {
        owner: Owner::telegram(found.telegram_id),
        credential: Credential::Grant(found.grant_id),
        client: Some(found.client_id),
        scopes: found.scopes,
    }))
}

/// The owner's grants that still work, oldest first.
pub async fn list_grants(pool: &PgPool, owner: Owner) -> Result<Vec<GrantInfo>, EngineError> {
    Ok(crate::db::list_oauth_grants(pool, owner.telegram_id())
        .await?
        .into_iter()
        .map(|row| GrantInfo {
            id: row.id,
            can_write: row
                .scopes
                .iter()
                .any(|scope| scope == tokens::SCOPE_RECIPES_WRITE),
            client_id: row.client_id,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
        .collect())
}

/// Revokes a grant of the owner; [`EngineError::NotFound`] when it is not one of the owner's
/// active grants.
pub async fn revoke_grant(pool: &PgPool, owner: Owner, grant_id: i64) -> Result<(), EngineError> {
    if crate::db::revoke_oauth_grant(pool, owner.telegram_id(), grant_id).await? {
        Ok(())
    } else {
        Err(EngineError::NotFound)
    }
}

/// Deletes the sign-ins, codes and tokens that can no longer be used; the rows deleted.
pub async fn purge(pool: &PgPool) -> Result<u64, EngineError> {
    Ok(crate::db::purge_oauth(pool).await?)
}

/// Compares two secrets' hashes in a time that does not depend on where they differ.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verifier_has_rfc_7636s_form() {
        assert!(is_pkce_verifier(&"a".repeat(43)));
        assert!(is_pkce_verifier(&format!("{}-._~", "Z9".repeat(64))[..128]));
        assert!(!is_pkce_verifier(&"a".repeat(42)), "too short");
        assert!(!is_pkce_verifier(&"a".repeat(129)), "too long");
        assert!(
            !is_pkce_verifier(&format!("{}+", "a".repeat(43))),
            "not unreserved"
        );
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn equal_secrets_compare_equal_only() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn the_tokens_of_each_realm_have_their_prefix() {
        let access = tokens::new_secret(Realm::Staging.access_prefix());
        assert!(tokens::has_shape(&access, "jio_stg_"));
        assert!(
            !tokens::has_token_shape(&access, Realm::Staging),
            "not a key"
        );
        assert!(
            !tokens::has_shape(&access, Realm::Production.access_prefix()),
            "a staging token is not a production one"
        );
        let refresh = tokens::new_secret(Realm::Production.refresh_prefix());
        assert!(tokens::has_shape(&refresh, "jir_"));
        assert!(!tokens::has_shape(&refresh, "jio_"));
    }
}
