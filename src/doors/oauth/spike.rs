//! Task 0 of `specs/tasks/oauth.md` (§7), temporary and on staging only: a real sign-in with
//! Telegram, to learn what its ID token holds before the authorization server is built on it.
//! [`TRY_PATH`] sends the browser to Telegram; the callback exchanges the code and shows what
//! was checked and received (claim names, whether the nonce came back, the Telegram id to the
//! person who just signed in). Nothing is stored but the attempt, in memory, 10 minutes.
//!
//! Telegram's callback is the authorization server's ([`super::server`]): it hands back the
//! attempts started here ([`Spike::take`], [`Spike::answer`]). Removed once the sign-in works on
//! staging.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::page::{escape, html, see_other};
use super::telegram::{Attempt, TelegramLogin};

/// The page that starts a trial sign-in.
pub const TRY_PATH: &str = "/oauth/telegram/try";

/// How long an attempt waits for Telegram's answer.
const ATTEMPT_LIFETIME: Duration = Duration::from_secs(600);

/// Most attempts waiting at once.
const MAX_PENDING: usize = 50;

/// Most exchanges with Telegram at once.
const MAX_EXCHANGES: usize = 4;

/// The trial's state: the login, who is in the beta, the attempts waiting for Telegram.
#[derive(Debug)]
pub struct Spike {
    login: TelegramLogin,
    beta_users: Vec<i64>,
    bot_id: Option<String>,
    pending: Mutex<HashMap<String, (Attempt, Instant)>>,
    exchanges: Semaphore,
}

impl Spike {
    /// `bot_id` is the part of the bot's token before `:`, to tell whether BotFather's client id
    /// is the bot's id.
    pub fn new(login: TelegramLogin, beta_users: Vec<i64>, bot_id: Option<String>) -> Self {
        Self {
            login,
            beta_users,
            bot_id,
            pending: Mutex::new(HashMap::new()),
            exchanges: Semaphore::new(MAX_EXCHANGES),
        }
    }
}

/// The trial's start page (Telegram's callback is the authorization server's).
pub fn router(spike: Arc<Spike>) -> Router {
    Router::new().route(TRY_PATH, get(start)).with_state(spike)
}

async fn start(State(spike): State<Arc<Spike>>) -> Response {
    let attempt = Attempt::new();
    let url = match spike.login.authorize_url(&attempt) {
        Ok(url) => url,
        Err(error) => {
            warn!(error = %error, "Telegram sign-in address could not be built");
            return html(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erreur",
                "<p>L'adresse de connexion Telegram n'a pas pu être construite.</p>",
            );
        }
    };
    {
        let mut pending = spike.pending.lock().unwrap_or_else(PoisonError::into_inner);
        pending.retain(|_, (_, created)| created.elapsed() < ATTEMPT_LIFETIME);
        if pending.len() >= MAX_PENDING {
            return html(
                StatusCode::SERVICE_UNAVAILABLE,
                "Trop d'essais",
                "<p>Trop d'essais en cours : réessaie dans 10 minutes.</p>",
            );
        }
        pending.insert(attempt.state.clone(), (attempt, Instant::now()));
    }
    see_other(&url)
}

impl Spike {
    /// Takes the trial attempt of `state`, if it is one (used once, 10 minutes).
    pub fn take(&self, state: &str) -> Option<Attempt> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(state)
            .filter(|(_, created)| created.elapsed() < ATTEMPT_LIFETIME)
            .map(|(attempt, _)| attempt)
    }

    /// Telegram's answer to a trial attempt: the report page.
    pub async fn answer(
        &self,
        attempt: Attempt,
        code: Option<String>,
        error: Option<String>,
    ) -> Response {
        let spike = self;
        if let Some(error) = &error {
            let error: String = error.chars().take(100).collect();
            warn!(error = %error, "Telegram sign-in trial: Telegram answered an error");
            return html(
                StatusCode::BAD_REQUEST,
                "Telegram a refusé",
                &format!(
                    "<p>Telegram a répondu : <code>{}</code>.</p><p><a href=\"{TRY_PATH}\">Recommencer</a>.</p>",
                    escape(&error)
                ),
            );
        }
        let Some(code) = code.filter(|code| !code.is_empty() && code.len() <= 512) else {
            return html(
                StatusCode::BAD_REQUEST,
                "Pas de code",
                "<p>Telegram n'a pas renvoyé de code.</p>",
            );
        };
        let Ok(_permit) = spike.exchanges.try_acquire() else {
            return html(
                StatusCode::SERVICE_UNAVAILABLE,
                "Occupé",
                "<p>Trop de connexions en cours : réessaie dans un instant.</p>",
            );
        };
        match spike.login.exchange(&code, &attempt.verifier).await {
            Ok(identity) => {
                let nonce_returned = identity.nonce.as_deref() == Some(attempt.nonce.as_str());
                let nonce_mismatch = identity.nonce.is_some() && !nonce_returned;
                let beta = spike.beta_users.contains(&identity.telegram_id);
                let client_id_is_bot_id = spike.bot_id.as_deref() == Some(spike.login.client_id());
                info!(
                    claims = ?identity.claim_names,
                    algorithm = identity.algorithm.as_deref().unwrap_or("none"),
                    nonce_returned,
                    nonce_mismatch,
                    beta,
                    client_id_is_bot_id,
                    "Telegram sign-in trial succeeded"
                );
                let yes_no = |yes: bool| if yes { "oui" } else { "non" };
                let nonce = if nonce_returned {
                    "oui, identique"
                } else if nonce_mismatch {
                    "<strong>différent</strong> (à signaler)"
                } else {
                    "non (absent du jeton)"
                };
                html(
                    StatusCode::OK,
                    "Connexion Telegram réussie",
                    &format!(
                        "<p>Telegram a répondu et son jeton d'identité est valable : émetteur, \
                     destinataire (le Client ID) et date d'expiration vérifiés.</p><ul>\
                     <li>Compte Telegram : <code>{id}</code> (c'est bien le tien ?)</li>\
                     <li>Dans la liste bêta : {beta}</li>\
                     <li>Client ID = identifiant du bot : {bot}</li>\
                     <li>Nonce renvoyé : {nonce}</li>\
                     <li>Signature annoncée : <code>{algorithm}</code></li>\
                     <li>Champs reçus : <code>{claims}</code></li></ul>\
                     <p>Tu peux fermer cette page. <a href=\"{TRY_PATH}\">Refaire l'essai</a>.</p>",
                        id = identity.telegram_id,
                        beta = yes_no(beta),
                        bot = yes_no(client_id_is_bot_id),
                        algorithm = escape(identity.algorithm.as_deref().unwrap_or("aucune")),
                        claims = escape(&identity.claim_names.join(", ")),
                    ),
                )
            }
            Err(error) => {
                warn!(error = %error, "Telegram sign-in trial failed");
                html(
                    StatusCode::BAD_GATEWAY,
                    "Échec de la connexion",
                    &format!(
                        "<p>{}</p><p><a href=\"{TRY_PATH}\">Recommencer</a>.</p>",
                        escape(&error.to_string())
                    ),
                )
            }
        }
    }
}
