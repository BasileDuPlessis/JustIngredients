//! The authorization server (`specs/tasks/oauth.md`, §2, §3 and §7): an allowed assistant sends
//! the user's browser to [`AUTHORIZE_PATH`]; the user signs in with Telegram, approves on our
//! consent page, and the browser goes back to the assistant with our code, which the assistant
//! exchanges at [`TOKEN_PATH`] for tokens. The state and its rules are `engine::oauth`; this
//! module checks the HTTP side:
//!
//! - before the client and its redirect URI are known to be allowed, problems are shown on a
//!   page, never redirected (no open redirect); after, they go back to the client as OAuth errors,
//!   every answer carrying the client's `state` and our `iss` (RFC 9207);
//! - the browser that starts is bound by a `__Host-` cookie (SameSite=Lax: it comes back on
//!   Telegram's redirect, never on another site's form); Telegram's answer and the consent page
//!   need it; Telegram's answer is claimed once (a reload waits for the first to finish);
//! - the consent page names the Telegram account, is never framed nor cached, has no script, and
//!   its form only posts to us, whose 303 goes to the client;
//! - the token endpoint answers JSON, never cached, `invalid_grant` for any bad code or refresh
//!   token. Clients are public (PKCE binds the code): a secret or an assertion sent anyway is
//!   ignored, never trusted;
//! - limits never refuse a request that turns out valid: the assistants' users share their
//!   addresses. Sign-ins started are limited per address (an IPv6 /64) and in all; at most
//!   [`MAX_AT_ONCE`] requests at once, the database's connections being few.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::{info, warn};

use super::OAuthSettings;
use super::clients::{self, Client};
use super::page::{escape, see_other};
use super::telegram::{Attempt, CALLBACK_PATH, TelegramLogin};
use crate::engine::oauth::{self, Callback, CodeExchange, Decision, GrantError, SignInRequest};
use crate::engine::tokens::{Realm, SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE};
use crate::engine::{Lang, Owner};
use crate::observability::server::RateLimiter;

pub use super::metadata::{AUTHORIZE_PATH, TOKEN_PATH};

/// The consent page.
pub const CONSENT_PATH: &str = "/oauth/consent";

/// The cookie binding a sign-in to the browser that started it.
pub const BINDING_COOKIE: &str = "__Host-ji_signin";

/// Largest form body the authorization server reads.
const MAX_BODY_BYTES: usize = 4 * 1024;

/// Largest query string it reads (the authorization request, Telegram's answer).
const MAX_QUERY_BYTES: usize = 4 * 1024;

/// Longest `state` a client may send, in bytes (given back as is).
const MAX_STATE_BYTES: usize = 1024;

/// Requests the authorization server handles at once: with the MCP door's 4, within the bot's
/// 10 database connections.
pub const MAX_AT_ONCE: usize = 3;

/// How long a request waits for a place.
const WAIT_FOR_PLACE: Duration = Duration::from_secs(2);

/// Exchanges with Telegram at once.
const MAX_TELEGRAM_EXCHANGES: usize = 2;

/// How long a reloaded Telegram answer waits for the first one to finish.
const WAIT_FOR_FIRST_ANSWER: Duration = Duration::from_secs(8);

/// What the authorization server needs, shared by its requests.
#[derive(Debug)]
pub struct AuthorizationServer {
    pool: PgPool,
    realm: Realm,
    settings: OAuthSettings,
    login: TelegramLogin,
    beta_users: Vec<i64>,
    at_once: Semaphore,
    telegram_exchanges: Semaphore,
    /// Sign-ins started per address (each writes a row).
    sign_ins: RateLimiter,
    /// Sign-ins started in all.
    all_sign_ins: RateLimiter,
    /// Codes and refresh tokens refused per address: past the limit, refusals answer 429.
    refused_grants: RateLimiter,
}

impl AuthorizationServer {
    pub fn new(
        pool: PgPool,
        realm: Realm,
        settings: OAuthSettings,
        login: TelegramLogin,
        beta_users: Vec<i64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            realm,
            settings,
            login,
            beta_users,
            at_once: Semaphore::new(MAX_AT_ONCE),
            telegram_exchanges: Semaphore::new(MAX_TELEGRAM_EXCHANGES),
            sign_ins: RateLimiter::new(30, 60),
            all_sign_ins: RateLimiter::new(300, 600),
            refused_grants: RateLimiter::new(20, 60),
        })
    }

    fn allowed(&self, owner: Owner) -> bool {
        self.beta_users.contains(&owner.telegram_id())
    }

    /// A place among the requests at once, or `None` after [`WAIT_FOR_PLACE`].
    async fn place(&self) -> Option<SemaphorePermit<'_>> {
        tokio::time::timeout(WAIT_FOR_PLACE, self.at_once.acquire())
            .await
            .ok()?
            .ok()
    }
}

/// The authorization server's routes.
pub fn router(server: Arc<AuthorizationServer>) -> Router {
    Router::new()
        .route(AUTHORIZE_PATH, get(authorize))
        .route(CALLBACK_PATH, get(telegram_callback))
        .route(CONSENT_PATH, get(consent_page).post(consent_decision))
        .route(TOKEN_PATH, axum::routing::post(token))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(server)
}

/// The client's address for the limits: Fly's `Fly-Client-IP`, else the peer; an IPv6 address
/// by its /64, the block one subscriber usually holds.
pub fn address_key(headers: &HeaderMap, peer: IpAddr) -> String {
    let address = headers
        .get("fly-client-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .unwrap_or(peer);
    match address {
        IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| v6.to_string(), |v4| v4.to_string()),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Texts

/// The language of a page: French when the browser asks for it first, else English.
fn lang_of(headers: &HeaderMap) -> Lang {
    let first = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_ascii_lowercase());
    Lang::from_code(first.as_deref())
}

fn text(lang: Lang, fr: &'static str, en: &'static str) -> &'static str {
    if lang.is_french() { fr } else { en }
}

/// A page saying why the sign-in stops here, without going back to the assistant; it ends the
/// browser's binding.
fn stop_page(
    lang: Lang,
    status: StatusCode,
    why_fr: &'static str,
    why_en: &'static str,
) -> Response {
    let mut response = super::page::html_in(
        lang,
        status,
        text(lang, "Connexion impossible", "Cannot connect"),
        &format!(
            "<p>{}</p><p>{}</p>",
            text(lang, why_fr, why_en),
            text(
                lang,
                "Vous pouvez fermer cette page et recommencer depuis votre assistant.",
                "You can close this page and start again from your assistant."
            )
        ),
    );
    response
        .headers_mut()
        .insert(header::SET_COOKIE, binding_cookie(None));
    response
}

fn unknown_sign_in(lang: Lang) -> Response {
    stop_page(
        lang,
        StatusCode::BAD_REQUEST,
        "Cette connexion est inconnue ou a expiré (10 minutes).",
        "This sign-in is unknown or has expired (10 minutes).",
    )
}

fn busy(lang: Lang) -> Response {
    super::page::html_in(
        lang,
        StatusCode::SERVICE_UNAVAILABLE,
        text(lang, "Un instant", "One moment"),
        text(
            lang,
            "<p>Trop de connexions en cours : réessayez dans un instant.</p>",
            "<p>Too many sign-ins at once: try again in a moment.</p>",
        ),
    )
}

fn too_long(lang: Lang) -> Response {
    stop_page(
        lang,
        StatusCode::URI_TOO_LONG,
        "Cette adresse est trop longue.",
        "This address is too long.",
    )
}

// ---------------------------------------------------------------------------------------------
// Back to the client

/// The client's redirect URI with `params`, its `state` and our `iss` (RFC 9207), as a 303.
fn back_to_client(
    settings: &OAuthSettings,
    redirect_uri: &str,
    params: &[(&str, &str)],
    state: Option<&str>,
    clear_binding: bool,
) -> Response {
    let Ok(mut url) = reqwest::Url::parse(redirect_uri) else {
        return super::page::html(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erreur",
            "<p>Adresse de retour invalide.</p>",
        );
    };
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in params {
            query.append_pair(name, value);
        }
        if let Some(state) = state {
            query.append_pair("state", state);
        }
        query.append_pair("iss", &settings.issuer);
    }
    let mut response = see_other(url.as_str());
    if clear_binding {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, binding_cookie(None));
    }
    response
}

/// An OAuth error sent back to the client.
fn error_to_client(
    settings: &OAuthSettings,
    request: &SignInRequest,
    error: &str,
    description: &str,
) -> Response {
    back_to_client(
        settings,
        &request.redirect_uri,
        &[("error", error), ("error_description", description)],
        request.client_state.as_deref(),
        true,
    )
}

/// The binding cookie with `value`, or its removal.
fn binding_cookie(value: Option<&str>) -> HeaderValue {
    let cookie = match value {
        Some(value) => format!(
            "{BINDING_COOKIE}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={}",
            oauth::REQUEST_LIFETIME_SECS
        ),
        None => format!("{BINDING_COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"),
    };
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// The binding cookie's value the browser sent, if any.
fn binding_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == BINDING_COOKIE && !value.is_empty()).then(|| value.to_string())
        })
}

fn query_too_long(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|query| query.len() > MAX_QUERY_BYTES)
}

// ---------------------------------------------------------------------------------------------
// The authorization endpoint

#[derive(Debug, Default, Deserialize)]
struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    resource: Option<String>,
}

/// The scopes we know among `asked` (space separated); reading when none is known.
fn known_scopes(asked: Option<&str>) -> Vec<String> {
    let mut scopes = vec![SCOPE_RECIPES_READ.to_string()];
    if asked.is_some_and(|asked| asked.split(' ').any(|scope| scope == SCOPE_RECIPES_WRITE)) {
        scopes.push(SCOPE_RECIPES_WRITE.to_string());
    }
    scopes
}

/// Whether `challenge` has the form of an S256 challenge: 43 base64url characters.
fn is_s256_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

async fn authorize(
    State(server): State<Arc<AuthorizationServer>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    query: Result<Query<AuthorizeQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let lang = lang_of(&headers);
    if query_too_long(&uri) {
        return too_long(lang);
    }
    let Some(_place) = server.place().await else {
        return busy(lang);
    };
    let Query(query) = query.unwrap_or_default();
    // Who asks and where to answer: shown on a page if wrong, never redirected
    let Some(client) = query.client_id.as_deref().and_then(clients::find) else {
        let refused: String = query
            .client_id
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        warn!(client_id = %refused, "OAuth sign-in refused: unknown client");
        return stop_page(
            lang,
            StatusCode::BAD_REQUEST,
            "Cet assistant n'est pas (encore) accepté par JustIngredients.",
            "This assistant is not accepted by JustIngredients (yet).",
        );
    };
    let Some(redirect_uri) = query
        .redirect_uri
        .as_deref()
        .filter(|redirect| client.allows_redirect(redirect))
    else {
        warn!(
            client_id = client.id,
            "OAuth sign-in refused: unknown redirect URI"
        );
        return stop_page(
            lang,
            StatusCode::BAD_REQUEST,
            "L'adresse de retour de cet assistant n'est pas la sienne.",
            "This assistant's return address is not its own.",
        );
    };
    // From here, problems go back to the client
    let state_too_long = query
        .state
        .as_ref()
        .is_some_and(|state| state.len() > MAX_STATE_BYTES);
    let state = query.state.filter(|_| !state_too_long);
    let refuse = |error: &str, description: &str| {
        back_to_client(
            &server.settings,
            redirect_uri,
            &[("error", error), ("error_description", description)],
            state.as_deref(),
            false,
        )
    };
    if state_too_long {
        return refuse("invalid_request", "state is too long");
    }
    if query.response_type.as_deref() != Some("code") {
        return refuse("unsupported_response_type", "response_type must be code");
    }
    if query.code_challenge_method.as_deref() != Some("S256") {
        return refuse("invalid_request", "code_challenge_method must be S256");
    }
    let Some(code_challenge) = query
        .code_challenge
        .filter(|challenge| is_s256_challenge(challenge))
    else {
        return refuse(
            "invalid_request",
            "code_challenge must be an S256 challenge",
        );
    };
    if query
        .resource
        .as_deref()
        .is_some_and(|resource| resource != server.settings.resource)
    {
        return refuse("invalid_target", "unknown resource");
    }
    let address = address_key(&headers, peer.ip());
    if !server.sign_ins.is_allowed(&address) || !server.all_sign_ins.is_allowed("all") {
        warn!("OAuth sign-in refused: too many started");
        return refuse(
            "temporarily_unavailable",
            "too many sign-ins, try again later",
        );
    }

    let request = SignInRequest {
        client_id: client.id.to_string(),
        redirect_uri: redirect_uri.to_string(),
        client_state: state.clone(),
        code_challenge,
        scopes: known_scopes(query.scope.as_deref()),
        resource: server.settings.resource.clone(),
    };
    let attempt = Attempt::new();
    let binding = crate::engine::tokens::new_secret("");
    let started = oauth::start_sign_in(
        &server.pool,
        &request,
        &oauth::SignInSecrets {
            state: &attempt.state,
            binding: &binding,
            verifier: &attempt.verifier,
            nonce: &attempt.nonce,
        },
    )
    .await;
    let telegram = started
        .map_err(anyhow::Error::from)
        .and_then(|_| server.login.authorize_url(&attempt));
    match telegram {
        Ok(url) => {
            info!(client_id = client.id, "OAuth sign-in started");
            let mut response = see_other(&url);
            response
                .headers_mut()
                .insert(header::SET_COOKIE, binding_cookie(Some(&binding)));
            response
        }
        Err(error) => {
            warn!(error = %error, "OAuth sign-in could not start");
            refuse("server_error", "the sign-in could not start")
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Telegram's answer

#[derive(Debug, Default, Deserialize)]
struct TelegramAnswer {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn telegram_callback(
    State(server): State<Arc<AuthorizationServer>>,
    headers: HeaderMap,
    uri: Uri,
    query: Result<Query<TelegramAnswer>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let lang = lang_of(&headers);
    if query_too_long(&uri) {
        return too_long(lang);
    }
    let Query(answer) = query.unwrap_or_default();
    let Some(state) = answer.state.filter(|state| state.len() <= 128) else {
        return unknown_sign_in(lang);
    };
    let Some(_place) = server.place().await else {
        return busy(lang);
    };
    let binding = binding_of(&headers);
    let pending = match oauth::find_sign_in(&server.pool, &state, binding.as_deref()).await {
        Ok(Callback::Pending(pending)) => pending,
        Ok(Callback::SignedIn) => return see_other(CONSENT_PATH),
        Ok(Callback::Unknown) => return unknown_sign_in(lang),
        Ok(Callback::OtherBrowser) => {
            warn!("OAuth sign-in refused: finished in another browser");
            return stop_page(
                lang,
                StatusCode::BAD_REQUEST,
                "Cette connexion a été commencée dans un autre navigateur, ou remplacée par une \
                 connexion plus récente. Si vous n'avez rien demandé, ignorez ce lien.",
                "This sign-in was started in another browser, or replaced by a newer one. If you \
                 did not ask for it, ignore this link.",
            );
        }
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: could not read the request");
            return busy(lang);
        }
    };
    // Telegram's answer counts once: a reload waits for the first to finish
    match oauth::claim_sign_in(&server.pool, pending.id).await {
        Ok(true) => {}
        Ok(false) => return wait_for_first_answer(&server, pending.id, lang).await,
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: could not claim the request");
            return busy(lang);
        }
    }
    // Every way out from here ends the sign-in and goes back to the client
    let give_up = |error: &'static str, description: &'static str| {
        let server = Arc::clone(&server);
        let id = pending.id;
        let request = pending.request.clone();
        async move {
            if let Err(failure) = oauth::abandon_sign_in(&server.pool, id).await {
                warn!(error = %failure, "OAuth sign-in: could not end the request");
            }
            error_to_client(&server.settings, &request, error, description)
        }
    };
    if let Some(error) = &answer.error {
        let error: String = error
            .chars()
            .filter(char::is_ascii_graphic)
            .take(64)
            .collect();
        info!(error = %error, "OAuth sign-in: Telegram answered an error");
        return give_up("access_denied", "the Telegram sign-in was not completed").await;
    }
    let Some(code) = answer
        .code
        .filter(|code| !code.is_empty() && code.len() <= 512)
    else {
        return give_up("access_denied", "the Telegram sign-in was not completed").await;
    };
    let exchange = tokio::time::timeout(WAIT_FOR_PLACE, server.telegram_exchanges.acquire()).await;
    let Ok(Ok(_exchange)) = exchange else {
        return give_up("temporarily_unavailable", "too many sign-ins at once").await;
    };
    let identity = match server.login.exchange(&code, &pending.verifier).await {
        Ok(identity) => identity,
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: the Telegram code was not exchanged");
            return give_up("server_error", "the Telegram sign-in failed").await;
        }
    };
    // Telegram returns the nonce it was sent (the staging trial, 2026-11-12): required
    if identity.nonce.as_deref() != Some(pending.nonce.as_str()) {
        warn!(
            nonce_returned = identity.nonce.is_some(),
            "OAuth sign-in refused: Telegram's nonce is missing or not ours"
        );
        return give_up("access_denied", "the Telegram sign-in failed").await;
    }
    info!(claims = ?identity.claim_names, "OAuth sign-in: Telegram answered");
    let owner = Owner::telegram(identity.telegram_id);
    if !server.allowed(owner) {
        info!("OAuth sign-in refused: not in the beta");
        if let Err(failure) = oauth::abandon_sign_in(&server.pool, pending.id).await {
            warn!(error = %failure, "OAuth sign-in: could not end the request");
        }
        return stop_page(
            lang,
            StatusCode::FORBIDDEN,
            "Les assistants ne sont pas encore ouverts à ce compte Telegram.",
            "Assistants are not open to this Telegram account yet.",
        );
    }
    match oauth::signed_in(
        &server.pool,
        pending.id,
        owner,
        identity.display_name.as_deref(),
    )
    .await
    {
        Ok(true) => see_other(CONSENT_PATH),
        Ok(false) => unknown_sign_in(lang),
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: could not record who signed in");
            busy(lang)
        }
    }
}

/// A Telegram answer received again (a reload, a double load): once the first one has signed the
/// user in, on to the consent page; if it failed or takes too long, the sign-in is over.
async fn wait_for_first_answer(server: &AuthorizationServer, sign_in: i64, lang: Lang) -> Response {
    let started = tokio::time::Instant::now();
    while started.elapsed() < WAIT_FOR_FIRST_ANSWER {
        match oauth::sign_in_state(&server.pool, sign_in).await {
            Ok(Some(true)) => return see_other(CONSENT_PATH),
            Ok(Some(false)) => tokio::time::sleep(Duration::from_millis(250)).await,
            Ok(None) | Err(_) => break,
        }
    }
    unknown_sign_in(lang)
}

// ---------------------------------------------------------------------------------------------
// Consent

async fn consent_page(
    State(server): State<Arc<AuthorizationServer>>,
    headers: HeaderMap,
) -> Response {
    let browser_lang = lang_of(&headers);
    let Some(binding) = binding_of(&headers) else {
        return unknown_sign_in(browser_lang);
    };
    let Some(_place) = server.place().await else {
        return busy(browser_lang);
    };
    let form_token = crate::engine::tokens::new_secret("");
    let consent = match oauth::open_consent(&server.pool, &binding, &form_token).await {
        Ok(Some(consent)) => consent,
        Ok(None) => return unknown_sign_in(browser_lang),
        Err(error) => {
            warn!(error = %error, "OAuth consent: could not read the request");
            return busy(browser_lang);
        }
    };
    // The user's language in the bot, else the browser's
    let lang = crate::engine::known_language_of(&server.pool, consent.owner)
        .await
        .ok()
        .flatten()
        .unwrap_or(browser_lang);
    let Some(client) = clients::find(&consent.request.client_id) else {
        return unknown_sign_in(lang);
    };
    let Some(origin) = Client::redirect_origin(&consent.request.redirect_uri) else {
        return unknown_sign_in(lang);
    };
    let redirect_host = reqwest::Url::parse(&consent.request.redirect_uri)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default();
    let asks_write = consent
        .request
        .scopes
        .iter()
        .any(|scope| scope == SCOPE_RECIPES_WRITE);
    consent_html(&ConsentView {
        lang,
        client,
        account: consent.telegram_name.as_deref(),
        redirect_host: &redirect_host,
        redirect_origin: &origin,
        form_token: &form_token,
        asks_write,
    })
}

/// What the consent page shows.
struct ConsentView<'a> {
    lang: Lang,
    client: &'a Client,
    /// The Telegram account's name.
    account: Option<&'a str>,
    redirect_host: &'a str,
    redirect_origin: &'a str,
    form_token: &'a str,
    asks_write: bool,
}

fn consent_html(view: &ConsentView<'_>) -> Response {
    let lang = view.lang;
    let name = escape(view.client.name);
    // Escaped by the page
    let title = if lang.is_french() {
        format!("{} demande l'accès à vos recettes", view.client.name)
    } else {
        format!("{} asks to access your recipes", view.client.name)
    };
    let account = view.account.map(escape);
    let intro = match (lang.is_french(), &account) {
        (true, Some(account)) => format!(
            "Vous êtes connecté avec le compte Telegram <strong>{account}</strong>. \
             <strong>{name}</strong> veut utiliser JustIngredients en votre nom. Ce n'est pas \
             votre compte ? Refusez."
        ),
        (true, None) => format!(
            "Vous êtes connecté avec Telegram. <strong>{name}</strong> veut utiliser \
             JustIngredients en votre nom."
        ),
        (false, Some(account)) => format!(
            "You are signed in with the Telegram account <strong>{account}</strong>. \
             <strong>{name}</strong> wants to use JustIngredients on your behalf. Not your \
             account? Deny."
        ),
        (false, None) => format!(
            "You are signed in with Telegram. <strong>{name}</strong> wants to use \
             JustIngredients on your behalf."
        ),
    };
    let write_hint = if view.asks_write {
        text(
            lang,
            " (demandé par l'assistant)",
            " (asked by the assistant)",
        )
    } else {
        ""
    };
    let body = format!(
        "<p>{intro}</p>\
         <form method=\"post\" action=\"{CONSENT_PATH}\">\
         <input type=\"hidden\" name=\"consent\" value=\"{token}\">\
         <p>✅ {read}</p>\
         <p><label><input type=\"checkbox\" name=\"write\" value=\"yes\"> {write}{write_hint}\
         </label></p>\
         <p>{back} <strong>{host}</strong>.</p>\
         <p><button type=\"submit\" name=\"decision\" value=\"allow\">{allow}</button> \
         <button type=\"submit\" name=\"decision\" value=\"deny\">{deny}</button></p>\
         </form><p><small>{revoke}</small></p>",
        token = escape(view.form_token),
        read = text(
            lang,
            "Lire vos recettes et vos mémos",
            "Read your recipes and memos"
        ),
        write = text(
            lang,
            "Les modifier aussi : renommer, écrire des mémos, créer des recettes (jamais supprimer)",
            "Change them too: rename, write memos, create recipes (never delete)"
        ),
        back = text(
            lang,
            "Votre réponse sera renvoyée à",
            "Your answer will be sent back to"
        ),
        host = escape(view.redirect_host),
        allow = text(lang, "Autoriser", "Allow"),
        deny = text(lang, "Refuser", "Deny"),
        revoke = text(
            lang,
            "Vous pourrez retirer cet accès à tout moment avec /cle dans le bot.",
            "You can remove this access at any time with /key in the bot."
        ),
    );
    let mut response = super::page::html_in(lang, StatusCode::OK, &title, &body);
    // The form posts to us; our 303 then goes to the client, which Chromium also checks
    if let Ok(policy) = HeaderValue::from_str(&format!(
        "default-src 'none'; style-src 'unsafe-inline'; form-action 'self' {}; \
         frame-ancestors 'none'; base-uri 'none'",
        view.redirect_origin
    )) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

#[derive(Debug, Default, Deserialize)]
struct ConsentForm {
    consent: Option<String>,
    decision: Option<String>,
    write: Option<String>,
}

async fn consent_decision(
    State(server): State<Arc<AuthorizationServer>>,
    headers: HeaderMap,
    form: Result<Form<ConsentForm>, axum::extract::rejection::FormRejection>,
) -> Response {
    let lang = lang_of(&headers);
    let Form(form) = form.unwrap_or_default();
    let (Some(binding), Some(token)) = (binding_of(&headers), form.consent) else {
        return unknown_sign_in(lang);
    };
    let Some(_place) = server.place().await else {
        return busy(lang);
    };
    let decision = match form.decision.as_deref() {
        Some("allow") => Decision::Allow {
            write: form.write.as_deref() == Some("yes"),
        },
        _ => Decision::Deny,
    };
    let decided = match oauth::decide(&server.pool, &binding, &token, decision).await {
        Ok(Some(decided)) => decided,
        Ok(None) => return unknown_sign_in(lang),
        Err(error) => {
            warn!(error = %error, "OAuth consent: could not record the decision");
            return busy(lang);
        }
    };
    match &decided.code {
        Some(code) => {
            info!(
                client_id = %decided.request.client_id,
                write = matches!(decision, Decision::Allow { write: true }),
                "OAuth sign-in approved"
            );
            back_to_client(
                &server.settings,
                &decided.request.redirect_uri,
                &[("code", code.expose())],
                decided.request.client_state.as_deref(),
                true,
            )
        }
        None => {
            info!(client_id = %decided.request.client_id, "OAuth sign-in denied");
            error_to_client(
                &server.settings,
                &decided.request,
                "access_denied",
                "the user denied access",
            )
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The token endpoint

#[derive(Debug, Default, Deserialize)]
struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
    client_secret: Option<String>,
    client_assertion: Option<String>,
}

/// A JSON answer of the token endpoint, never cached.
fn token_answer(status: StatusCode, body: &serde_json::Value) -> Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::PRAGMA, HeaderValue::from_static("no-cache")),
        ],
        body.to_string(),
    )
        .into_response()
}

fn token_error(status: StatusCode, error: &str, description: &str) -> Response {
    let mut response = token_answer(
        status,
        &json!({ "error": error, "error_description": description }),
    );
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"oauth\""),
        );
    }
    response
}

async fn token(
    State(server): State<Arc<AuthorizationServer>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    form: Result<Form<TokenForm>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(form)) = form else {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "the body must be a form",
        );
    };
    let Some(_place) = server.place().await else {
        return token_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
            "busy, try again",
        );
    };
    let Some(client) = form.client_id.as_deref().and_then(clients::find) else {
        return token_error(StatusCode::UNAUTHORIZED, "invalid_client", "unknown client");
    };
    // Public clients: PKCE binds the code; a secret or an assertion sent anyway proves nothing
    if headers.contains_key(header::AUTHORIZATION)
        || form.client_secret.is_some()
        || form.client_assertion.is_some()
    {
        info!(
            client_id = client.id,
            "OAuth token request with client authentication: ignored"
        );
    }
    if form
        .resource
        .as_deref()
        .is_some_and(|resource| resource != server.settings.resource)
    {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "unknown resource",
        );
    }
    let issued = match form.grant_type.as_deref() {
        Some("authorization_code") => {
            let (Some(code), Some(redirect_uri), Some(verifier)) =
                (&form.code, &form.redirect_uri, &form.code_verifier)
            else {
                return token_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "code, redirect_uri and code_verifier are needed",
                );
            };
            oauth::redeem_code(
                &server.pool,
                server.realm,
                CodeExchange {
                    code,
                    client_id: client.id,
                    redirect_uri,
                    verifier,
                    resource: form.resource.as_deref(),
                },
            )
            .await
        }
        Some("refresh_token") => {
            let Some(refresh_token) = &form.refresh_token else {
                return token_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "refresh_token is needed",
                );
            };
            let server_for_check = Arc::clone(&server);
            oauth::refresh(
                &server.pool,
                server.realm,
                refresh_token,
                client.id,
                move |owner| server_for_check.allowed(owner),
            )
            .await
        }
        _ => {
            return token_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "authorization_code or refresh_token",
            );
        }
    };
    match issued {
        Ok(issued) => {
            if let Some(grant) = issued.new_grant {
                info!(
                    client_id = client.id,
                    grant_id = issued.grant_id,
                    replaced = grant.replaced,
                    "OAuth grant made"
                );
            }
            token_answer(
                StatusCode::OK,
                &json!({
                    "access_token": issued.access_token.expose(),
                    "token_type": "Bearer",
                    "expires_in": issued.expires_in,
                    "refresh_token": issued.refresh_token.expose(),
                    "scope": issued.scopes.join(" "),
                }),
            )
        }
        Err(GrantError::Internal(error)) => {
            warn!(error = %error, "OAuth token endpoint failed");
            token_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "temporarily_unavailable",
                "try again",
            )
        }
        Err(GrantError::TooOften) => {
            warn!(client_id = client.id, "OAuth grant renewed too often");
            token_error(
                StatusCode::TOO_MANY_REQUESTS,
                "invalid_request",
                "renewed too often, try again in a few minutes",
            )
        }
        Err(refused) => {
            match &refused {
                GrantError::Reused { grant_id } => warn!(
                    grant_id,
                    client_id = client.id,
                    "OAuth grant revoked: a reused code or token"
                ),
                other => info!(reason = %other, client_id = client.id, "OAuth grant refused"),
            }
            // Only refusals are counted, and past the limit they answer 429: a valid code or
            // token from the same (shared) address is never refused for them
            let address = address_key(&headers, peer.ip());
            let status = if server.refused_grants.is_allowed(&address) {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::TOO_MANY_REQUESTS
            };
            token_error(
                status,
                "invalid_grant",
                "the code or refresh token is invalid, expired or revoked",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_scopes_and_reading_by_default() {
        assert_eq!(known_scopes(None), [SCOPE_RECIPES_READ]);
        assert_eq!(
            known_scopes(Some("openid offline_access recipes:write")),
            [SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE]
        );
        assert_eq!(known_scopes(Some("admin")), [SCOPE_RECIPES_READ]);
    }

    #[test]
    fn an_s256_challenge_is_43_base64url_characters() {
        assert!(is_s256_challenge(
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        ));
        assert!(!is_s256_challenge("plain"));
        assert!(!is_s256_challenge(&"=".repeat(43)));
    }

    #[test]
    fn the_binding_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; __Host-ji_signin=abc; b=2"),
        );
        assert_eq!(binding_of(&headers).as_deref(), Some("abc"));
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-ji_signin="),
        );
        assert_eq!(binding_of(&headers), None);
        let cookie = binding_cookie(Some("abc"));
        let cookie = cookie.to_str().unwrap_or_default();
        for part in [
            "Secure",
            "HttpOnly",
            "SameSite=Lax",
            "Path=/",
            "Max-Age=600",
        ] {
            assert!(cookie.contains(part), "{part}");
        }
    }

    #[test]
    fn french_pages_for_french_browsers() {
        let mut headers = HeaderMap::new();
        assert_eq!(lang_of(&headers), Lang::En);
        headers.insert(
            header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("fr-FR,fr;q=0.9,en;q=0.8"),
        );
        assert_eq!(lang_of(&headers), Lang::Fr);
    }

    #[test]
    fn an_ipv6_client_is_limited_by_its_64() {
        let headers = HeaderMap::new();
        let one: IpAddr = "2a09:8280:1::1"
            .parse()
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        let other: IpAddr = "2a09:8280:1:0:ffff::9"
            .parse()
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        assert_eq!(address_key(&headers, one), address_key(&headers, other));
        assert_eq!(address_key(&headers, one), "2a09:8280:1:0::/64");
        let v4: IpAddr = "160.79.104.10"
            .parse()
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        assert_eq!(address_key(&headers, v4), "160.79.104.10");
        let mapped: IpAddr = "::ffff:160.79.104.10"
            .parse()
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        assert_eq!(address_key(&headers, mapped), "160.79.104.10");
    }
}
