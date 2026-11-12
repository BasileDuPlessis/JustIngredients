//! The sign-in with Telegram (`specs/tasks/oauth.md`, task 0) against a fake Telegram over real
//! sockets: the trial page sends the browser to Telegram with PKCE, the callback exchanges the
//! code with the login secret and checks the ID token. No database.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Form, Router};
use base64::Engine;
use just_ingredients::config::TelegramLoginConfig;
use just_ingredients::doors::oauth::spike::{self, Spike};
use just_ingredients::doors::oauth::telegram::{self, Endpoints, TelegramLogin};
use just_ingredients::secret::SecretString;
use serde_json::json;

/// A JSON answer (axum's `json` feature is not enabled).
fn json_answer(status: StatusCode, value: &serde_json::Value) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

const CLIENT_ID: &str = "123456";
const SECRET: &str = "login-secret";
const USER: i64 = 8_194_026_187;
const ORIGIN: &str = "https://127.0.0.1";

/// What the fake Telegram was told by the test: the challenge and nonce of the sign-in page.
#[derive(Debug, Default)]
struct Fake {
    challenge: Option<String>,
    nonce: Option<String>,
}

type Shared = Arc<Mutex<Fake>>;

fn encode(value: &serde_json::Value) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
}

/// Telegram's token endpoint: the bot's credentials, the code, our callback, the PKCE verifier.
async fn token(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{CLIENT_ID}:{SECRET}"))
    );
    let refuse = |error: &str| json_answer(StatusCode::BAD_REQUEST, &json!({ "error": error }));
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(&expected)
    {
        return refuse("invalid_client");
    }
    let field = |name: &str| form.get(name).map(String::as_str);
    let (challenge, nonce) = {
        let fake = fake.lock().unwrap_or_else(PoisonError::into_inner);
        (fake.challenge.clone(), fake.nonce.clone())
    };
    let verified = field("code_verifier").map(telegram::challenge) == challenge;
    if field("grant_type") != Some("authorization_code")
        || field("code") != Some("the-code")
        || field("client_id") != Some(CLIENT_ID)
        || field("redirect_uri") != Some(&format!("{ORIGIN}{}", telegram::CALLBACK_PATH))
        || !verified
    {
        return refuse("invalid_grant");
    }
    let now = chrono::Utc::now().timestamp();
    let id_token = format!(
        "{}.{}.c2lnbmF0dXJl",
        encode(&json!({ "alg": "RS256", "kid": "k1" })),
        encode(&json!({
            "iss": telegram::ISSUER, "aud": CLIENT_ID, "sub": "424242", "iat": now,
            "exp": now + 3600, "id": USER, "name": "<b>Basile</b>", "nonce": nonce
        }))
    );
    json_answer(
        StatusCode::OK,
        &json!({ "access_token": "at", "token_type": "Bearer", "expires_in": 3600, "id_token": id_token }),
    )
}

async fn listen(app: Router) -> Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(address)
}

/// The fake Telegram and the trial pages, served; their shared state.
async fn serve() -> Result<(SocketAddr, Shared)> {
    let fake: Shared = Arc::default();
    let telegram = listen(
        Router::new()
            .route("/token", post(token))
            .with_state(Arc::clone(&fake)),
    )
    .await?;
    let login = TelegramLogin::new(
        TelegramLoginConfig {
            client_id: CLIENT_ID.to_string(),
            secret: SecretString::new(SECRET),
        },
        ORIGIN,
        Endpoints {
            issuer: telegram::ISSUER.to_string(),
            authorize: format!("{}/auth", telegram::ISSUER),
            token: format!("http://{telegram}/token"),
        },
    )?;
    let pages = listen(spike::router(Arc::new(Spike::new(
        login,
        vec![USER],
        Some(CLIENT_ID.to_string()),
    ))))
    .await?;
    Ok((pages, fake))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// Starts a trial: Telegram's sign-in address and its parameters.
async fn start(pages: SocketAddr, fake: &Shared) -> Result<HashMap<String, String>> {
    let response = client()?
        .get(format!("http://{pages}{}", spike::TRY_PATH))
        .send()
        .await?;
    assert_eq!(response.status().as_u16(), 303);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .context("a redirect")?;
    let url = reqwest::Url::parse(location)?;
    assert_eq!(url.origin().ascii_serialization(), telegram::ISSUER);
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let mut shared = fake.lock().unwrap_or_else(PoisonError::into_inner);
    shared.challenge = query.get("code_challenge").cloned();
    shared.nonce = query.get("nonce").cloned();
    Ok(query)
}

async fn callback(pages: SocketAddr, query: &[(&str, &str)]) -> Result<(u16, HeaderMap, String)> {
    let response = client()?
        .get(format!("http://{pages}{}", telegram::CALLBACK_PATH))
        .query(query)
        .send()
        .await?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    Ok((status, headers, response.text().await?))
}

#[tokio::test]
async fn a_trial_sign_in_reads_the_telegram_id_from_the_id_token() -> Result<()> {
    let (pages, fake) = serve().await?;
    let query = start(pages, &fake).await?;
    assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
    assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
    assert_eq!(
        query.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    let state = query.get("state").context("a state")?;

    let (status, headers, page) =
        callback(pages, &[("code", "the-code"), ("state", state)]).await?;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Connexion Telegram réussie"), "{page}");
    assert!(
        page.contains(&USER.to_string()),
        "the id claim, not sub: {page}"
    );
    assert!(!page.contains("424242"), "never sub");
    assert!(page.contains("Dans la liste bêta : oui"));
    assert!(page.contains("Nonce renvoyé : oui"));
    assert!(page.contains("Client ID = identifiant du bot : oui"));
    assert!(page.contains("aud, exp, iat, id, iss, name, nonce, sub"));
    assert!(!page.contains("<b>Basile"), "the name is never shown");
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        headers
            .get("x-frame-options")
            .and_then(|value| value.to_str().ok()),
        Some("DENY")
    );
    assert!(
        headers
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|policy| policy.contains("frame-ancestors 'none'"))
    );

    // An attempt is used once
    let (status, _, page) = callback(pages, &[("code", "the-code"), ("state", state)]).await?;
    assert_eq!(status, 400);
    assert!(page.contains("Essai inconnu"), "{page}");
    Ok(())
}

#[tokio::test]
async fn refusals_are_shown_not_followed() -> Result<()> {
    let (pages, fake) = serve().await?;

    // An unknown state, before anything else
    let (status, _, page) = callback(pages, &[("code", "the-code"), ("state", "forged")]).await?;
    assert_eq!(status, 400);
    assert!(page.contains("Essai inconnu"));

    // Telegram's error, escaped
    let query = start(pages, &fake).await?;
    let state = query.get("state").context("a state")?;
    let (status, _, page) = callback(
        pages,
        &[("error", "access_denied<script>"), ("state", state)],
    )
    .await?;
    assert_eq!(status, 400);
    assert!(page.contains("access_denied&lt;script&gt;"), "{page}");

    // A code Telegram refuses
    let query = start(pages, &fake).await?;
    let state = query.get("state").context("a state")?;
    let (status, _, page) = callback(pages, &[("code", "stolen"), ("state", state)]).await?;
    assert_eq!(status, 502);
    assert!(page.contains("invalid_grant"), "{page}");

    // A verifier that is not the one of the challenge
    let query = start(pages, &fake).await?;
    let state = query.get("state").context("a state")?.clone();
    fake.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .challenge = Some("other".to_string());
    let (status, _, page) = callback(pages, &[("code", "the-code"), ("state", &state)]).await?;
    assert_eq!(status, 502, "{page}");
    Ok(())
}
