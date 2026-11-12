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
//!   need it;
//! - the consent page is never framed nor cached, has no script, and its form only posts to us,
//!   whose 303 goes to the client;
//! - the token endpoint answers JSON, never cached, `invalid_grant` for any bad code or refresh
//!   token, with a limit per address.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::OAuthSettings;
use super::clients::{self, Client};
use super::page::{escape, see_other};
use super::spike::Spike;
use super::telegram::{Attempt, CALLBACK_PATH, TelegramLogin};
use crate::engine::oauth::{self, Callback, CodeExchange, Decision, GrantError, SignInRequest};
use crate::engine::tokens::{Realm, SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE};
use crate::engine::{Lang, Owner};
use crate::observability::server::{RateLimiter, client_key};

pub use super::metadata::{AUTHORIZE_PATH, TOKEN_PATH};

/// The consent page.
pub const CONSENT_PATH: &str = "/oauth/consent";

/// The cookie binding a sign-in to the browser that started it.
pub const BINDING_COOKIE: &str = "__Host-ji_signin";

/// Largest form or query the authorization server reads.
const MAX_BODY_BYTES: usize = 4 * 1024;

/// Longest `state` a client may send (given back as is).
const MAX_STATE_CHARS: usize = 1024;

/// Requests the authorization server handles at once (the rest wait).
const MAX_AT_ONCE: usize = 8;

/// Exchanges with Telegram at once.
const MAX_TELEGRAM_EXCHANGES: usize = 4;

/// What the authorization server needs, shared by its requests.
#[derive(Debug)]
pub struct AuthorizationServer {
    pool: PgPool,
    realm: Realm,
    settings: OAuthSettings,
    login: TelegramLogin,
    beta_users: Vec<i64>,
    spike: Option<Arc<Spike>>,
    at_once: Semaphore,
    telegram_exchanges: Semaphore,
    /// Sign-ins started per address (each writes a row).
    sign_ins: RateLimiter,
    /// Codes and refresh tokens refused per address.
    refused_grants: RateLimiter,
}

impl AuthorizationServer {
    pub fn new(
        pool: PgPool,
        realm: Realm,
        settings: OAuthSettings,
        login: TelegramLogin,
        beta_users: Vec<i64>,
        spike: Option<Arc<Spike>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            realm,
            settings,
            login,
            beta_users,
            spike,
            at_once: Semaphore::new(MAX_AT_ONCE),
            telegram_exchanges: Semaphore::new(MAX_TELEGRAM_EXCHANGES),
            sign_ins: RateLimiter::new(30, 60),
            refused_grants: RateLimiter::new(20, 60),
        })
    }

    fn allowed(&self, owner: Owner) -> bool {
        self.beta_users.contains(&owner.telegram_id())
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

/// A page saying why the sign-in stops here, without going back to the assistant.
fn stop_page(
    lang: Lang,
    status: StatusCode,
    why_fr: &'static str,
    why_en: &'static str,
) -> Response {
    super::page::html_in(
        lang,
        status,
        text(lang, "Connexion impossible", "Cannot connect"),
        &format!(
            "<p>{}</p><p>{}</p>",
            text(lang, why_fr, why_en),
            text(
                lang,
                "Tu peux fermer cette page et recommencer depuis ton assistant.",
                "You can close this page and start again from your assistant."
            )
        ),
    )
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
    stop_page(
        lang,
        StatusCode::SERVICE_UNAVAILABLE,
        "Trop de connexions en cours : réessaie dans un instant.",
        "Too many sign-ins at once: try again in a moment.",
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
    query: Result<Query<AuthorizeQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let lang = lang_of(&headers);
    let Ok(_place) = server.at_once.acquire().await else {
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
    let state = query
        .state
        .filter(|state| state.chars().count() <= MAX_STATE_CHARS);
    let refuse = |error: &str, description: &str| {
        back_to_client(
            &server.settings,
            redirect_uri,
            &[("error", error), ("error_description", description)],
            state.as_deref(),
            false,
        )
    };
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
    let address = client_key(&headers, peer.ip());
    if !server.sign_ins.is_allowed(&address) {
        return refuse("temporarily_unavailable", "too many sign-ins");
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
    query: Result<Query<TelegramAnswer>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let lang = lang_of(&headers);
    let Query(answer) = query.unwrap_or_default();
    let Some(state) = answer.state.filter(|state| state.len() <= 128) else {
        return unknown_sign_in(lang);
    };
    // A trial of the Telegram sign-in (staging, task 0)
    if let Some(spike) = &server.spike
        && let Some(attempt) = spike.take(&state)
    {
        return spike.answer(attempt, answer.code, answer.error).await;
    }
    let Ok(_place) = server.at_once.acquire().await else {
        return busy(lang);
    };
    let binding = binding_of(&headers);
    let pending = match oauth::find_sign_in(&server.pool, &state, binding.as_deref()).await {
        Ok(Callback::Pending(pending)) => pending,
        Ok(Callback::Unknown) => return unknown_sign_in(lang),
        Ok(Callback::OtherBrowser) => {
            warn!("OAuth sign-in refused: finished in another browser");
            return stop_page(
                lang,
                StatusCode::BAD_REQUEST,
                "Cette connexion a été commencée dans un autre navigateur. Si tu n'as rien \
                 demandé, ignore ce lien.",
                "This sign-in was started in another browser. If you did not ask for it, ignore \
                 this link.",
            );
        }
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: could not read the request");
            return busy(lang);
        }
    };
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
    let Ok(_exchange) = server.telegram_exchanges.try_acquire() else {
        return busy(lang);
    };
    let identity = match server.login.exchange(&code, &pending.verifier).await {
        Ok(identity) => identity,
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: the Telegram code was not exchanged");
            return give_up("server_error", "the Telegram sign-in failed").await;
        }
    };
    match identity.nonce.as_deref() {
        Some(nonce) if nonce != pending.nonce => {
            warn!("OAuth sign-in refused: Telegram's nonce is not ours");
            return give_up("access_denied", "the Telegram sign-in failed").await;
        }
        returned => info!(
            nonce_returned = returned.is_some(),
            claims = ?identity.claim_names,
            "OAuth sign-in: Telegram answered"
        ),
    }
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
    match oauth::signed_in(&server.pool, pending.id, owner).await {
        Ok(true) => see_other(CONSENT_PATH),
        Ok(false) => unknown_sign_in(lang),
        Err(error) => {
            warn!(error = %error, "OAuth sign-in: could not record who signed in");
            busy(lang)
        }
    }
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
    consent_html(
        lang,
        client,
        &redirect_host,
        &origin,
        &form_token,
        asks_write,
    )
}

fn consent_html(
    lang: Lang,
    client: &Client,
    redirect_host: &str,
    redirect_origin: &str,
    form_token: &str,
    asks_write: bool,
) -> Response {
    let name = escape(client.name);
    let title = if lang.is_french() {
        format!("{name} demande l'accès à tes recettes")
    } else {
        format!("{name} asks to access your recipes")
    };
    let write_hint = if asks_write {
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
        intro = if lang.is_french() {
            format!(
                "Tu es connecté avec Telegram. <strong>{name}</strong> veut utiliser \
                 JustIngredients en ton nom."
            )
        } else {
            format!(
                "You are signed in with Telegram. <strong>{name}</strong> wants to use \
                 JustIngredients on your behalf."
            )
        },
        token = escape(form_token),
        read = text(
            lang,
            "Lire tes recettes et tes mémos",
            "Read your recipes and memos"
        ),
        write = text(
            lang,
            "Les modifier aussi : renommer, écrire des mémos, créer des recettes (jamais supprimer)",
            "Change them too: rename, write memos, create recipes (never delete)"
        ),
        back = text(
            lang,
            "Ta réponse sera renvoyée à",
            "Your answer will be sent back to"
        ),
        host = escape(redirect_host),
        allow = text(lang, "Autoriser", "Allow"),
        deny = text(lang, "Refuser", "Deny"),
        revoke = text(
            lang,
            "Tu pourras retirer cet accès à tout moment avec /cle dans le bot.",
            "You can remove this access at any time with /key in the bot."
        ),
    );
    let mut response = super::page::html_in(lang, StatusCode::OK, &title, &body);
    // The form posts to us; our 303 then goes to the client, which Chromium also checks
    if let Ok(policy) = HeaderValue::from_str(&format!(
        "default-src 'none'; style-src 'unsafe-inline'; form-action 'self' {redirect_origin}; \
         frame-ancestors 'none'; base-uri 'none'"
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
    let mut response = (
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
        .into_response();
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"oauth\""),
        );
    }
    response
}

fn token_error(status: StatusCode, error: &str, description: &str) -> Response {
    token_answer(
        status,
        &json!({ "error": error, "error_description": description }),
    )
}

async fn token(
    State(server): State<Arc<AuthorizationServer>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    form: Result<Form<TokenForm>, axum::extract::rejection::FormRejection>,
) -> Response {
    let address = client_key(&headers, peer.ip());
    if !server.refused_grants.has_room(&address) {
        return token_error(
            StatusCode::TOO_MANY_REQUESTS,
            "invalid_request",
            "too many refused requests",
        );
    }
    let Ok(Form(form)) = form else {
        return token_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "the body must be a form",
        );
    };
    let Ok(_place) = server.at_once.acquire().await else {
        return token_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
            "busy",
        );
    };
    // Public clients: PKCE alone, no secret nor assertion
    if headers.contains_key(header::AUTHORIZATION)
        || form.client_secret.is_some()
        || form.client_assertion.is_some()
    {
        return token_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "public clients only (token_endpoint_auth_method none)",
        );
    }
    let Some(client) = form.client_id.as_deref().and_then(clients::find) else {
        return token_error(StatusCode::UNAUTHORIZED, "invalid_client", "unknown client");
    };
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
        Err(refused) => {
            server.refused_grants.is_allowed(&address);
            match &refused {
                GrantError::Reused { grant_id } => {
                    warn!(
                        grant_id,
                        client_id = client.id,
                        "OAuth grant revoked: a reused code or token"
                    );
                }
                other => info!(reason = %other, client_id = client.id, "OAuth grant refused"),
            }
            token_error(
                StatusCode::BAD_REQUEST,
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
}
