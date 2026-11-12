//! OAuth (migration 13): sign-ins under way, our authorization codes, grants and their tokens.
//! What they mean and the rules (lifetimes, rotation, reuse) are `engine::oauth`; this module only
//! reads and writes the rows. Every secret arrives hashed. Times are the database's (`now()`),
//! so a check never mixes two clocks.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use sqlx::postgres::PgPool;

/// A sign-in to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOAuthRequest<'a> {
    pub state_hash: &'a [u8],
    pub binding_hash: &'a [u8],
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub client_state: Option<&'a str>,
    pub code_challenge: &'a str,
    pub scopes: &'a [String],
    pub resource: &'a str,
    pub telegram_verifier: &'a str,
    pub telegram_nonce: &'a str,
    pub lifetime_secs: i64,
}

/// A sign-in as stored (its secrets hashed but the verifier and nonce used with Telegram).
#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OAuthRequestRow {
    pub id: i64,
    pub binding_hash: Vec<u8>,
    pub client_id: String,
    pub redirect_uri: String,
    pub client_state: Option<String>,
    pub code_challenge: String,
    pub scopes: Vec<String>,
    pub resource: String,
    pub telegram_verifier: String,
    pub telegram_nonce: String,
    pub telegram_id: Option<i64>,
    /// The Telegram account's name, from the ID token, for the consent page.
    pub telegram_name: Option<String>,
}

impl std::fmt::Debug for OAuthRequestRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthRequestRow")
            .field("id", &self.id)
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

const REQUEST_COLUMNS: &str = "id, binding_hash, client_id, redirect_uri, client_state, \
     code_challenge, scopes, resource, telegram_verifier, telegram_nonce, telegram_id, \
     telegram_name";

/// Stores a sign-in, valid `lifetime_secs`.
pub async fn insert_oauth_request(pool: &PgPool, request: &NewOAuthRequest<'_>) -> Result<i64> {
    sqlx::query_scalar(
        "INSERT INTO oauth_requests (state_hash, binding_hash, client_id, redirect_uri, \
             client_state, code_challenge, scopes, resource, telegram_verifier, telegram_nonce, \
             expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now() + make_interval(secs => $11)) \
         RETURNING id",
    )
    .bind(request.state_hash)
    .bind(request.binding_hash)
    .bind(request.client_id)
    .bind(request.redirect_uri)
    .bind(request.client_state)
    .bind(request.code_challenge)
    .bind(request.scopes)
    .bind(request.resource)
    .bind(request.telegram_verifier)
    .bind(request.telegram_nonce)
    .bind(request.lifetime_secs as f64)
    .fetch_one(pool)
    .await
    .context("Failed to store the sign-in")
}

/// The sign-in Telegram answers for with this state: unexpired, not consumed (someone may have
/// signed in already: Telegram's answer reloaded).
pub async fn find_oauth_request_by_state(
    pool: &PgPool,
    state_hash: &[u8],
) -> Result<Option<OAuthRequestRow>> {
    sqlx::query_as::<_, OAuthRequestRow>(&format!(
        "SELECT {REQUEST_COLUMNS} FROM oauth_requests \
         WHERE state_hash = $1 AND consumed_at IS NULL AND expires_at > now()"
    ))
    .bind(state_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to find the sign-in")
}

/// Claims Telegram's answer to a sign-in, once: `false` when another request claimed it (a
/// reload, a double load) or the sign-in is over.
pub async fn claim_oauth_request(pool: &PgPool, id: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE oauth_requests SET claimed_at = now() \
         WHERE id = $1 AND claimed_at IS NULL AND telegram_id IS NULL AND consumed_at IS NULL \
           AND expires_at > now()",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to claim the sign-in")?
    .rows_affected()
        > 0)
}

/// Where a claimed sign-in is: signed in (`Some(true)`), still being answered (`Some(false)`), or
/// over (`None`).
pub async fn oauth_request_signed_in(pool: &PgPool, id: i64) -> Result<Option<bool>> {
    sqlx::query_scalar(
        "SELECT telegram_id IS NOT NULL FROM oauth_requests \
         WHERE id = $1 AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to read the sign-in")
}

/// Records who signed in with Telegram, and their name; `false` when someone already did, or the
/// sign-in is over.
pub async fn set_oauth_request_user(
    pool: &PgPool,
    id: i64,
    telegram_id: i64,
    telegram_name: Option<&str>,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE oauth_requests SET telegram_id = $2, telegram_name = $3 \
         WHERE id = $1 AND telegram_id IS NULL AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(id)
    .bind(telegram_id)
    .bind(telegram_name)
    .execute(pool)
    .await
    .context("Failed to record who signed in")?
    .rows_affected()
        > 0)
}

/// The signed-in request of the browser holding `binding_hash`, ready for consent: its consent
/// form's token becomes `consent_hash` (an older form no longer counts).
pub async fn prepare_oauth_consent(
    pool: &PgPool,
    binding_hash: &[u8],
    consent_hash: &[u8],
) -> Result<Option<OAuthRequestRow>> {
    sqlx::query_as::<_, OAuthRequestRow>(&format!(
        "UPDATE oauth_requests SET consent_hash = $2 \
         WHERE binding_hash = $1 AND telegram_id IS NOT NULL AND consumed_at IS NULL \
           AND expires_at > now() \
         RETURNING {REQUEST_COLUMNS}"
    ))
    .bind(binding_hash)
    .bind(consent_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to prepare the consent")
}

/// Consumes, once, the signed-in request of the browser holding `binding_hash` whose consent
/// form carried `consent_hash`.
pub async fn consume_oauth_consent(
    conn: &mut PgConnection,
    binding_hash: &[u8],
    consent_hash: &[u8],
) -> Result<Option<OAuthRequestRow>> {
    sqlx::query_as::<_, OAuthRequestRow>(&format!(
        "UPDATE oauth_requests SET consumed_at = now() \
         WHERE binding_hash = $1 AND consent_hash = $2 AND telegram_id IS NOT NULL \
           AND consumed_at IS NULL AND expires_at > now() \
         RETURNING {REQUEST_COLUMNS}"
    ))
    .bind(binding_hash)
    .bind(consent_hash)
    .fetch_optional(conn)
    .await
    .context("Failed to consume the consent")
}

/// Consumes a sign-in that ends without a code (Telegram refused, the user is not allowed),
/// unless someone already signed in to it.
pub async fn abandon_oauth_request(pool: &PgPool, id: i64) -> Result<Option<OAuthRequestRow>> {
    sqlx::query_as::<_, OAuthRequestRow>(&format!(
        "UPDATE oauth_requests SET consumed_at = now() \
         WHERE id = $1 AND consumed_at IS NULL AND telegram_id IS NULL \
         RETURNING {REQUEST_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to end the sign-in")
}

/// One of our authorization codes to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOAuthCode<'a> {
    pub code_hash: &'a [u8],
    pub telegram_id: i64,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub scopes: &'a [String],
    pub resource: &'a str,
    pub lifetime_secs: i64,
}

/// Stores a code for the user of `telegram_id` (their row created when needed).
pub async fn insert_oauth_code(conn: &mut PgConnection, code: &NewOAuthCode<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (telegram_id, language_code) VALUES ($1, 'en') \
         ON CONFLICT (telegram_id) DO NOTHING",
    )
    .bind(code.telegram_id)
    .execute(&mut *conn)
    .await
    .context("Failed to make sure the user exists")?;
    sqlx::query(
        "INSERT INTO oauth_codes (code_hash, user_id, client_id, redirect_uri, code_challenge, \
             scopes, resource, expires_at) \
         SELECT $1, id, $3, $4, $5, $6, $7, now() + make_interval(secs => $8) \
         FROM users WHERE telegram_id = $2",
    )
    .bind(code.code_hash)
    .bind(code.telegram_id)
    .bind(code.client_id)
    .bind(code.redirect_uri)
    .bind(code.code_challenge)
    .bind(code.scopes)
    .bind(code.resource)
    .bind(code.lifetime_secs as f64)
    .execute(&mut *conn)
    .await
    .context("Failed to store the code")?;
    Ok(())
}

/// A code once redeemed.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OAuthCodeRow {
    pub user_id: i64,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scopes: Vec<String>,
    pub resource: String,
}

/// Marks a code used, once, if it is unexpired: the code, else `None`.
pub async fn use_oauth_code(
    conn: &mut PgConnection,
    code_hash: &[u8],
) -> Result<Option<OAuthCodeRow>> {
    sqlx::query_as::<_, OAuthCodeRow>(
        "UPDATE oauth_codes SET used_at = now() \
         WHERE code_hash = $1 AND used_at IS NULL AND expires_at > now() \
         RETURNING user_id, client_id, redirect_uri, code_challenge, scopes, resource",
    )
    .bind(code_hash)
    .fetch_optional(conn)
    .await
    .context("Failed to redeem the code")
}

/// Revokes the grant made with a code already used (it is presented again: copied); its id.
pub async fn revoke_grant_of_used_code(
    conn: &mut PgConnection,
    code_hash: &[u8],
) -> Result<Option<i64>> {
    sqlx::query_scalar(
        "UPDATE oauth_grants SET revoked_at = now() WHERE revoked_at IS NULL AND id = \
             (SELECT grant_id FROM oauth_codes WHERE code_hash = $1 AND used_at IS NOT NULL)
         RETURNING id",
    )
    .bind(code_hash)
    .fetch_optional(conn)
    .await
    .context("Failed to revoke the grant of a reused code")
}

/// Makes the grant of a redeemed code, replacing the user's active grant to the same client
/// (the user's row is locked meanwhile: two codes at once cannot both make one). Returns the new
/// grant's id and whether one was replaced.
pub async fn insert_oauth_grant(
    conn: &mut PgConnection,
    code_hash: &[u8],
    code: &OAuthCodeRow,
) -> Result<(i64, bool)> {
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(code.user_id)
        .execute(&mut *conn)
        .await
        .context("Failed to lock the user")?;
    let replaced = sqlx::query(
        "UPDATE oauth_grants SET revoked_at = now() \
         WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL",
    )
    .bind(code.user_id)
    .bind(&code.client_id)
    .execute(&mut *conn)
    .await
    .context("Failed to replace the previous grant")?
    .rows_affected()
        > 0;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO oauth_grants (user_id, client_id, scopes, resource, last_used_at) \
         VALUES ($1, $2, $3, $4, now()) RETURNING id",
    )
    .bind(code.user_id)
    .bind(&code.client_id)
    .bind(&code.scopes)
    .bind(&code.resource)
    .fetch_one(&mut *conn)
    .await
    .context("Failed to store the grant")?;
    sqlx::query("UPDATE oauth_codes SET grant_id = $1 WHERE code_hash = $2")
        .bind(id)
        .bind(code_hash)
        .execute(&mut *conn)
        .await
        .context("Failed to link the code to its grant")?;
    Ok((id, replaced))
}

/// The Telegram id of a user row.
pub async fn telegram_id_of_user(conn: &mut PgConnection, user_id: i64) -> Result<i64> {
    sqlx::query_scalar("SELECT telegram_id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(conn)
        .await
        .context("Failed to read the user")
}

/// Stores a token of a grant, valid `lifetime_secs`; its id.
pub async fn insert_oauth_token(
    conn: &mut PgConnection,
    grant_id: i64,
    kind: &str,
    token_hash: &[u8],
    parent_id: Option<i64>,
    lifetime_secs: i64,
) -> Result<i64> {
    sqlx::query_scalar(
        "INSERT INTO oauth_tokens (grant_id, kind, token_hash, parent_id, expires_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) RETURNING id",
    )
    .bind(grant_id)
    .bind(kind)
    .bind(token_hash)
    .bind(parent_id)
    .bind(lifetime_secs as f64)
    .fetch_one(conn)
    .await
    .context("Failed to store the token")
}

/// A refresh token presented, locked for the transaction, with what its state means now.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OAuthRefreshRow {
    pub id: i64,
    pub grant_id: i64,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub telegram_id: i64,
    /// Neither it nor its grant is revoked, and it has not expired.
    pub usable: bool,
    /// It was already exchanged.
    pub rotated: bool,
    /// It was exchanged less than the grace period ago.
    pub in_grace: bool,
}

/// The refresh token whose hash is `token_hash`, locked, its state judged with a grace period of
/// `grace_secs` after its rotation.
pub async fn lock_oauth_refresh(
    conn: &mut PgConnection,
    token_hash: &[u8],
    grace_secs: i64,
) -> Result<Option<OAuthRefreshRow>> {
    sqlx::query_as::<_, OAuthRefreshRow>(
        "SELECT t.id, t.grant_id, g.client_id, g.scopes, u.telegram_id, \
                (t.revoked_at IS NULL AND g.revoked_at IS NULL AND t.expires_at > now()) \
                    AS usable, \
                t.rotated_at IS NOT NULL AS rotated, \
                COALESCE(t.rotated_at > now() - make_interval(secs => $2), FALSE) AS in_grace \
         FROM oauth_tokens t \
         JOIN oauth_grants g ON g.id = t.grant_id \
         JOIN users u ON u.id = g.user_id \
         WHERE t.token_hash = $1 AND t.kind = 'refresh' \
         FOR UPDATE OF t",
    )
    .bind(token_hash)
    .bind(grace_secs as f64)
    .fetch_optional(conn)
    .await
    .context("Failed to find the refresh token")
}

/// How many refresh tokens were made from the refresh token `token_id` (its rotation, and a
/// retry within the grace period).
pub async fn oauth_refresh_children(conn: &mut PgConnection, token_id: i64) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM oauth_tokens WHERE parent_id = $1 AND kind = 'refresh'",
    )
    .bind(token_id)
    .fetch_one(conn)
    .await
    .context("Failed to count the renewals of a token")
}

/// How many refresh tokens a grant was given in the last `window_secs` seconds.
pub async fn oauth_recent_refreshes(
    conn: &mut PgConnection,
    grant_id: i64,
    window_secs: i64,
) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM oauth_tokens WHERE grant_id = $1 AND kind = 'refresh' \
         AND created_at > now() - make_interval(secs => $2)",
    )
    .bind(grant_id)
    .bind(window_secs as f64)
    .fetch_one(conn)
    .await
    .context("Failed to count a grant's renewals")
}

/// Revokes a grant's live refresh tokens (not yet exchanged) but the `keep` newest: a grant
/// never has more chains than that.
pub async fn trim_oauth_refreshes(conn: &mut PgConnection, grant_id: i64, keep: i64) -> Result<()> {
    sqlx::query(
        "UPDATE oauth_tokens SET revoked_at = now() \
         WHERE grant_id = $1 AND kind = 'refresh' AND rotated_at IS NULL AND revoked_at IS NULL \
           AND id NOT IN (SELECT id FROM oauth_tokens WHERE grant_id = $1 AND kind = 'refresh' \
                          AND rotated_at IS NULL AND revoked_at IS NULL \
                          ORDER BY id DESC LIMIT $2)",
    )
    .bind(grant_id)
    .bind(keep)
    .execute(conn)
    .await
    .context("Failed to trim a grant's refresh tokens")?;
    Ok(())
}

/// Notes that a refresh token was exchanged, and the grant used.
pub async fn rotate_oauth_refresh(conn: &mut PgConnection, token_id: i64) -> Result<()> {
    sqlx::query("UPDATE oauth_tokens SET rotated_at = now() WHERE id = $1 AND rotated_at IS NULL")
        .bind(token_id)
        .execute(&mut *conn)
        .await
        .context("Failed to rotate the refresh token")?;
    sqlx::query(
        "UPDATE oauth_grants SET last_used_at = now() \
         WHERE id = (SELECT grant_id FROM oauth_tokens WHERE id = $1)",
    )
    .bind(token_id)
    .execute(&mut *conn)
    .await
    .context("Failed to note the grant's use")?;
    Ok(())
}

/// Revokes a grant (its tokens stop working with it); `false` when it already was.
pub async fn revoke_oauth_grant_by_id(conn: &mut PgConnection, grant_id: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE oauth_grants SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(grant_id)
    .execute(conn)
    .await
    .context("Failed to revoke the grant")?
    .rows_affected()
        > 0)
}

/// An access token presented, with its grant.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OAuthAccessRow {
    pub grant_id: i64,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub resource: String,
    pub telegram_id: i64,
    /// Neither it nor its grant is revoked, and it has not expired.
    pub usable: bool,
}

/// The access token whose hash is `token_hash`, whatever its state.
pub async fn find_oauth_access(pool: &PgPool, token_hash: &[u8]) -> Result<Option<OAuthAccessRow>> {
    let start = std::time::Instant::now();
    let found = sqlx::query_as::<_, OAuthAccessRow>(
        "SELECT g.id AS grant_id, g.client_id, g.scopes, g.resource, u.telegram_id, \
                (t.revoked_at IS NULL AND g.revoked_at IS NULL AND t.expires_at > now()) \
                    AS usable \
         FROM oauth_tokens t \
         JOIN oauth_grants g ON g.id = t.grant_id \
         JOIN users u ON u.id = g.user_id \
         WHERE t.token_hash = $1 AND t.kind = 'access'",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to look the access token up")?;
    crate::observability::db_query("find_oauth_access", start.elapsed());
    Ok(found)
}

/// Notes that a grant was used now, at most once every `every_secs` seconds.
pub async fn touch_oauth_grant(pool: &PgPool, grant_id: i64, every_secs: i32) -> Result<()> {
    sqlx::query(
        "UPDATE oauth_grants SET last_used_at = now() WHERE id = $1 \
         AND (last_used_at IS NULL OR last_used_at < now() - make_interval(secs => $2))",
    )
    .bind(grant_id)
    .bind(f64::from(every_secs))
    .execute(pool)
    .await
    .context("Failed to note the grant's use")?;
    Ok(())
}

/// A grant as the bot lists it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OAuthGrantRow {
    pub id: i64,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// The grants of `telegram_id` that still work (not revoked, a refresh token alive), oldest
/// first.
pub async fn list_oauth_grants(pool: &PgPool, telegram_id: i64) -> Result<Vec<OAuthGrantRow>> {
    sqlx::query_as::<_, OAuthGrantRow>(
        "SELECT g.id, g.client_id, g.scopes, g.created_at, g.last_used_at \
         FROM oauth_grants g JOIN users u ON u.id = g.user_id \
         WHERE u.telegram_id = $1 AND g.revoked_at IS NULL \
           AND EXISTS (SELECT 1 FROM oauth_tokens t WHERE t.grant_id = g.id \
                       AND t.kind = 'refresh' AND t.revoked_at IS NULL \
                       AND t.expires_at > now()) \
         ORDER BY g.created_at, g.id",
    )
    .bind(telegram_id)
    .fetch_all(pool)
    .await
    .context("Failed to list the grants")
}

/// Revokes a grant of `telegram_id`; `false` when it is another user's, gone or already revoked.
pub async fn revoke_oauth_grant(pool: &PgPool, telegram_id: i64, grant_id: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE oauth_grants SET revoked_at = now() \
         WHERE id = $1 AND revoked_at IS NULL \
           AND user_id = (SELECT id FROM users WHERE telegram_id = $2)",
    )
    .bind(grant_id)
    .bind(telegram_id)
    .execute(pool)
    .await
    .context("Failed to revoke the grant")?
    .rows_affected()
        > 0)
}

/// Deletes what can no longer be used: sign-ins once expired, codes and tokens expired a day
/// ago (kept that long so a late reuse is still recognised), grants revoked 30 days ago (with
/// what is left of them). Returns the rows deleted.
pub async fn purge_oauth(pool: &PgPool) -> Result<u64> {
    let mut deleted = 0;
    for statement in [
        "DELETE FROM oauth_requests WHERE expires_at < now()",
        "DELETE FROM oauth_codes WHERE expires_at < now() - interval '1 day'",
        "DELETE FROM oauth_tokens WHERE expires_at < now() - interval '1 day'",
        "DELETE FROM oauth_grants WHERE revoked_at < now() - interval '30 days'",
    ] {
        deleted += sqlx::query(statement)
            .execute(pool)
            .await
            .context("Failed to purge OAuth rows")?
            .rows_affected();
    }
    Ok(deleted)
}
