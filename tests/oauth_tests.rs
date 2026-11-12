//! The OAuth door (`specs/tasks/oauth.md`) against a fake Telegram over real sockets: an
//! assistant's sign-in from `/oauth/authorize` to the MCP tools (a database), each refusal, and
//! the bare trial of the Telegram sign-in on staging (no database).
//!
//! The database tests need Postgres: `cargo test --test oauth_tests -- --include-ignored` with
//! `DATABASE_URL` set to a throwaway database.

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
use just_ingredients::db::init_database_schema;
use just_ingredients::doors::guard::Limits;
use just_ingredients::doors::oauth::server::{self, AuthorizationServer};
use just_ingredients::doors::oauth::spike::{self, Spike};
use just_ingredients::doors::oauth::telegram::{self, Endpoints, TelegramLogin};
use just_ingredients::doors::oauth::{OAuthSettings, metadata};
use just_ingredients::engine::oauth::pkce_challenge;
use just_ingredients::engine::tokens::Realm;
use just_ingredients::secret::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;

const CLIENT_ID: &str = "123456";
const SECRET: &str = "login-secret";
const ALICE: i64 = 8_194_026_187;
const STRANGER: i64 = 777;
const HOST: &str = "127.0.0.1";
const CLAUDE: &str = "https://claude.ai/oauth/mcp-oauth-client-metadata";
const CLAUDE_CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

fn origin() -> String {
    format!("https://{HOST}")
}

/// What the fake Telegram was told by the test: the challenge and nonce of the sign-in page,
/// and who signs in.
#[derive(Debug, Default)]
struct Fake {
    challenge: Option<String>,
    nonce: Option<String>,
    user: i64,
}

type Shared = Arc<Mutex<Fake>>;

fn encode(value: &Value) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
}

/// A JSON answer (axum's `json` feature is not enabled).
fn json_answer(status: StatusCode, value: &Value) -> Response {
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

/// Telegram's token endpoint: the bot's credentials, the code, our callback, the PKCE verifier.
async fn telegram_token(
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
    let (challenge, nonce, user) = {
        let fake = fake.lock().unwrap_or_else(PoisonError::into_inner);
        (fake.challenge.clone(), fake.nonce.clone(), fake.user)
    };
    let verified = field("code_verifier").map(telegram::challenge) == challenge;
    if field("grant_type") != Some("authorization_code")
        || field("code") != Some("the-code")
        || field("client_id") != Some(CLIENT_ID)
        || field("redirect_uri") != Some(&format!("{}{}", origin(), telegram::CALLBACK_PATH))
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
            "exp": now + 3600, "id": user, "name": "<b>Basile</b>", "nonce": nonce
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
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok(address)
}

/// The fake Telegram, then the MCP door with its authorization server (and the trial when
/// `trial`), served; their address and the fake's state.
async fn serve(pool: PgPool, trial: bool) -> Result<(SocketAddr, Shared)> {
    let fake: Shared = Arc::new(Mutex::new(Fake {
        user: ALICE,
        ..Fake::default()
    }));
    let telegram = listen(
        Router::new()
            .route("/token", post(telegram_token))
            .with_state(Arc::clone(&fake)),
    )
    .await?;
    let login = TelegramLogin::new(
        TelegramLoginConfig {
            client_id: CLIENT_ID.to_string(),
            secret: SecretString::new(SECRET),
        },
        &origin(),
        Endpoints {
            issuer: telegram::ISSUER.to_string(),
            authorize: format!("{}/auth", telegram::ISSUER),
            token: format!("http://{telegram}/token"),
        },
    )?;
    let settings = OAuthSettings::for_host(HOST);
    let spike = trial.then(|| {
        Arc::new(Spike::new(
            login.clone(),
            vec![ALICE],
            Some(CLIENT_ID.to_string()),
        ))
    });
    let mut app = just_ingredients::doors::mcp::router(
        pool.clone(),
        Realm::Production,
        vec![HOST.to_string()],
        Limits::default(),
        Some(&settings),
    )
    .merge(server::router(AuthorizationServer::new(
        pool,
        Realm::Production,
        settings,
        login,
        vec![ALICE],
        spike.clone(),
    )));
    if let Some(spike) = spike {
        app = app.merge(spike::router(spike));
    }
    Ok((listen(app).await?, fake))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// An answer: status, headers, body.
struct Answer {
    status: u16,
    headers: HeaderMap,
    body: String,
}

impl Answer {
    async fn of(response: reqwest::Response) -> Result<Self> {
        Ok(Self {
            status: response.status().as_u16(),
            headers: response.headers().clone(),
            body: response.text().await?,
        })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The redirect's address (without its query) and its query.
    fn location(&self) -> Result<(String, HashMap<String, String>)> {
        let mut url = reqwest::Url::parse(self.header("location").context("a redirect")?)?;
        let query = url.query_pairs().into_owned().collect();
        url.set_query(None);
        Ok((url.to_string(), query))
    }

    /// The binding cookie set (empty: cleared), if any.
    fn binding(&self) -> Option<String> {
        self.headers
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(|cookie| {
                cookie
                    .split(';')
                    .next()?
                    .strip_prefix(&format!("{}=", server::BINDING_COOKIE))
                    .map(str::to_string)
            })
    }
}

/// A browser: our server's address and its binding cookie.
struct Browser {
    address: SocketAddr,
    cookie: Option<String>,
}

impl Browser {
    fn new(address: SocketAddr) -> Self {
        Self {
            address,
            cookie: None,
        }
    }

    async fn send(&mut self, request: reqwest::RequestBuilder) -> Result<Answer> {
        let request = match &self.cookie {
            Some(cookie) => {
                request.header("Cookie", format!("{}={cookie}", server::BINDING_COOKIE))
            }
            None => request,
        };
        let answer = Answer::of(
            request
                .header("Accept-Language", "fr-FR,fr;q=0.9")
                .send()
                .await?,
        )
        .await?;
        if let Some(cookie) = answer.binding() {
            self.cookie = (!cookie.is_empty()).then_some(cookie);
        }
        Ok(answer)
    }

    async fn get(&mut self, path_and_query: &str) -> Result<Answer> {
        let request = client()?.get(format!("http://{}{path_and_query}", self.address));
        self.send(request).await
    }

    async fn post_form(&mut self, path: &str, form: &[(&str, &str)]) -> Result<Answer> {
        let request = client()?
            .post(format!("http://{}{path}", self.address))
            .form(form);
        self.send(request).await
    }

    /// Telegram's answer with its code, for `state`.
    async fn telegram_answers(&mut self, state: &str) -> Result<Answer> {
        self.get(&format!(
            "{}?code=the-code&state={state}",
            telegram::CALLBACK_PATH
        ))
        .await
    }
}

/// The authorization request claude.ai makes, with `changes` to its parameters (`None`: left
/// out).
fn authorize_path(changes: &[(&str, Option<&str>)]) -> Result<String> {
    let challenge = pkce_challenge(VERIFIER);
    let resource = format!("{}/mcp", origin());
    let mut params: Vec<(&str, Option<&str>)> = vec![
        ("response_type", Some("code")),
        ("client_id", Some(CLAUDE)),
        ("redirect_uri", Some(CLAUDE_CALLBACK)),
        ("code_challenge", Some(&challenge)),
        ("code_challenge_method", Some("S256")),
        ("state", Some("claude-state")),
        ("scope", Some("recipes:read")),
        ("resource", Some(&resource)),
    ];
    for (name, value) in changes {
        match params.iter_mut().find(|(param, _)| param == name) {
            Some(param) => param.1 = *value,
            None => params.push((name, *value)),
        }
    }
    let present: Vec<(&str, &str)> = params
        .iter()
        .filter_map(|(name, value)| value.map(|value| (*name, value)))
        .collect();
    let url = reqwest::Url::parse_with_params("http://x/oauth/authorize", present)?;
    Ok(format!(
        "{}?{}",
        url.path(),
        url.query().unwrap_or_default()
    ))
}

/// Starts a sign-in as claude.ai sends it; Telegram's `state` (the fake is told the challenge and
/// nonce, as Telegram would be).
async fn start(browser: &mut Browser, fake: &Shared) -> Result<String> {
    let answer = browser.get(&authorize_path(&[])?).await?;
    assert_eq!(answer.status, 303, "{}", answer.body);
    let (telegram_page, query) = answer.location()?;
    assert_eq!(telegram_page, format!("{}/auth", telegram::ISSUER));
    assert!(browser.cookie.is_some(), "the browser is bound");
    let mut shared = fake.lock().unwrap_or_else(PoisonError::into_inner);
    shared.challenge = query.get("code_challenge").cloned();
    shared.nonce = query.get("nonce").cloned();
    query.get("state").cloned().context("a state")
}

/// The consent form's token on the consent page.
fn consent_token(page: &str) -> Result<String> {
    let marker = "name=\"consent\" value=\"";
    let start = page.find(marker).context("the consent form")? + marker.len();
    let end = page[start..].find('"').context("the value's end")?;
    Ok(page[start..start + end].to_string())
}

/// A form's fields.
type Fields<'a> = Vec<(&'a str, &'a str)>;

/// The token endpoint: status, JSON, headers.
async fn token(address: SocketAddr, form: &[(&str, &str)]) -> Result<(u16, Value, HeaderMap)> {
    let answer = Answer::of(
        client()?
            .post(format!("http://{address}{}", server::TOKEN_PATH))
            .form(form)
            .send()
            .await?,
    )
    .await?;
    Ok((
        answer.status,
        serde_json::from_str(&answer.body).unwrap_or(Value::Null),
        answer.headers,
    ))
}

/// The status of a tools/list with `access_token`, and how many tools.
async fn tool_count(address: SocketAddr, access_token: &str) -> Result<(u16, usize)> {
    let response = client()?
        .post(format!("http://{address}/mcp"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .send()
        .await?;
    let status = response.status().as_u16();
    let body: Value = serde_json::from_str(&response.text().await?).unwrap_or(Value::Null);
    Ok((
        status,
        body["result"]["tools"].as_array().map_or(0, Vec::len),
    ))
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn claude_connects_with_telegram_and_uses_the_tools(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let (address, fake) = serve(pool.clone(), false).await?;
    let mut browser = Browser::new(address);

    // claude.ai sends the browser here; we send it to Telegram, bound by a cookie
    let state = start(&mut browser, &fake).await?;

    // Telegram's answer: the code exchanged, the user recognised, on to the consent page
    let answer = browser.telegram_answers(&state).await?;
    assert_eq!(answer.status, 303, "{}", answer.body);
    assert_eq!(answer.header("location"), Some(server::CONSENT_PATH));

    // The consent page: who asks, where the answer goes, reading only unless ticked
    let page = browser.get(server::CONSENT_PATH).await?;
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("claude.ai demande l"), "{}", page.body);
    assert!(page.body.contains("type=\"checkbox\" name=\"write\""));
    assert!(
        !page.body.contains("checked"),
        "writing is never ticked for the user"
    );
    assert!(!page.body.contains("<script"), "no script");
    let policy = page.header("content-security-policy").unwrap_or_default();
    assert!(
        policy.contains("form-action 'self' https://claude.ai"),
        "{policy}"
    );
    assert!(policy.contains("frame-ancestors 'none'"));
    assert_eq!(page.header("x-frame-options"), Some("DENY"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    let form = consent_token(&page.body)?;

    // Allowed with writing: back to claude.ai with our code, its state and our issuer
    let answer = browser
        .post_form(
            server::CONSENT_PATH,
            &[("consent", &form), ("decision", "allow"), ("write", "yes")],
        )
        .await?;
    assert_eq!(answer.status, 303, "{}", answer.body);
    let (back, query) = answer.location()?;
    assert_eq!(back, CLAUDE_CALLBACK);
    assert_eq!(query.get("state").map(String::as_str), Some("claude-state"));
    assert_eq!(query.get("iss"), Some(&origin()));
    let code = query.get("code").context("a code")?.clone();
    assert!(browser.cookie.is_none(), "the binding is cleared");

    // claude.ai's server exchanges the code
    let resource = format!("{}/mcp", origin());
    let (status, tokens, headers) = token(
        address,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", CLAUDE_CALLBACK),
            ("client_id", CLAUDE),
            ("code_verifier", VERIFIER),
            ("resource", &resource),
        ],
    )
    .await?;
    assert_eq!(status, 200, "{tokens}");
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    assert_eq!(tokens["scope"], "recipes:read recipes:write");
    let access = tokens["access_token"].as_str().context("an access token")?;
    let refresh = tokens["refresh_token"]
        .as_str()
        .context("a refresh token")?;
    assert!(access.starts_with("jio_") && refresh.starts_with("jir_"));

    // The tools, with writing
    assert_eq!(tool_count(address, access).await?, (200, 7));

    // Renewed: a new pair (unknown scopes asked are ignored)
    let (status, renewed, _) = token(
        address,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", CLAUDE),
            ("scope", "recipes:read offline_access"),
        ],
    )
    .await?;
    assert_eq!(status, 200, "{renewed}");
    let renewed_access = renewed["access_token"].as_str().context("renewed")?;
    assert_eq!(tool_count(address, renewed_access).await?, (200, 7));

    // The code presented again: refused, and the grant it made is revoked
    let (status, refused, _) = token(
        address,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", CLAUDE_CALLBACK),
            ("client_id", CLAUDE),
            ("code_verifier", VERIFIER),
        ],
    )
    .await?;
    assert_eq!(
        (status, refused["error"].as_str()),
        (400, Some("invalid_grant"))
    );
    assert_eq!(tool_count(address, renewed_access).await?.0, 401);
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn every_refusal_is_shown_or_sent_back_with_the_issuer(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let (address, fake) = serve(pool.clone(), false).await?;
    let mut browser = Browser::new(address);

    // Before the client and its redirect are known: a page, never a redirect
    for changes in [
        vec![("client_id", Some("https://evil.example/client.json"))],
        vec![(
            "client_id",
            Some("https://claude.ai/oauth/claude-code-client-metadata"),
        )],
        vec![("redirect_uri", Some("https://evil.example/callback"))],
        vec![("redirect_uri", None)],
    ] {
        let answer = browser.get(&authorize_path(&changes)?).await?;
        assert_eq!(answer.status, 400, "{changes:?}");
        assert!(answer.header("location").is_none(), "{changes:?}");
        assert!(answer.body.contains("Connexion impossible"));
    }

    // After: an OAuth error back to the client, with its state and our issuer
    for (changes, error) in [
        (
            vec![("response_type", Some("token"))],
            "unsupported_response_type",
        ),
        (
            vec![("code_challenge_method", Some("plain"))],
            "invalid_request",
        ),
        (vec![("code_challenge_method", None)], "invalid_request"),
        (vec![("code_challenge", Some("short"))], "invalid_request"),
        (
            vec![("resource", Some("https://other.example/mcp"))],
            "invalid_target",
        ),
    ] {
        let answer = browser.get(&authorize_path(&changes)?).await?;
        assert_eq!(answer.status, 303, "{changes:?}");
        let (back, query) = answer.location()?;
        assert_eq!(back, CLAUDE_CALLBACK);
        assert_eq!(
            query.get("error").map(String::as_str),
            Some(error),
            "{changes:?}"
        );
        assert_eq!(query.get("state").map(String::as_str), Some("claude-state"));
        assert_eq!(query.get("iss"), Some(&origin()));
    }

    // Telegram's answer in another browser: a page, nothing sent back
    let state = start(&mut browser, &fake).await?;
    let mut other = Browser::new(address);
    let answer = other.telegram_answers(&state).await?;
    assert_eq!(answer.status, 400);
    assert!(answer.header("location").is_none());
    assert!(answer.body.contains("autre navigateur"), "{}", answer.body);

    // Telegram refused: access_denied back to the client
    let answer = browser
        .get(&format!(
            "{}?error=access_denied&state={state}",
            telegram::CALLBACK_PATH
        ))
        .await?;
    assert_eq!(answer.status, 303, "{}", answer.body);
    let (_, query) = answer.location()?;
    assert_eq!(
        query.get("error").map(String::as_str),
        Some("access_denied")
    );
    assert_eq!(query.get("iss"), Some(&origin()));

    // Refused on the consent page: access_denied, and the page is used up
    let state = start(&mut browser, &fake).await?;
    browser.telegram_answers(&state).await?;
    let page = browser.get(server::CONSENT_PATH).await?;
    let form = consent_token(&page.body)?;
    let kept = browser.cookie.clone();
    let answer = browser
        .post_form(
            server::CONSENT_PATH,
            &[("consent", &form), ("decision", "deny")],
        )
        .await?;
    let (_, query) = answer.location()?;
    assert_eq!(
        query.get("error").map(String::as_str),
        Some("access_denied")
    );
    assert_eq!(query.get("state").map(String::as_str), Some("claude-state"));
    assert_eq!(query.get("iss"), Some(&origin()));
    browser.cookie = kept;
    let again = browser
        .post_form(
            server::CONSENT_PATH,
            &[("consent", &form), ("decision", "allow")],
        )
        .await?;
    assert_eq!(again.status, 400, "decided once");

    // A form posted without the browser's cookie (another site's form): refused
    let state = start(&mut browser, &fake).await?;
    browser.telegram_answers(&state).await?;
    let form = consent_token(&browser.get(server::CONSENT_PATH).await?.body)?;
    let answer = other
        .post_form(
            server::CONSENT_PATH,
            &[("consent", &form), ("decision", "allow")],
        )
        .await?;
    assert_eq!(answer.status, 400);
    assert!(answer.header("location").is_none());

    // Someone outside the beta: a page, no code
    fake.lock().unwrap_or_else(PoisonError::into_inner).user = STRANGER;
    let state = start(&mut browser, &fake).await?;
    let answer = browser.telegram_answers(&state).await?;
    assert_eq!(answer.status, 403, "{}", answer.body);
    assert!(answer.header("location").is_none());

    // The token endpoint
    let made_up_refresh = format!("jir_{}", "A".repeat(43));
    let refusals: [(Fields<'_>, u16, &str); 5] = [
        (
            vec![
                ("grant_type", "authorization_code"),
                ("client_id", "https://evil.example/c"),
            ],
            401,
            "invalid_client",
        ),
        (
            vec![("grant_type", "password"), ("client_id", CLAUDE)],
            400,
            "unsupported_grant_type",
        ),
        (
            vec![
                ("grant_type", "refresh_token"),
                ("client_id", CLAUDE),
                ("client_secret", "s"),
                ("refresh_token", "r"),
            ],
            401,
            "invalid_client",
        ),
        (
            vec![
                ("grant_type", "authorization_code"),
                ("client_id", CLAUDE),
                ("code", "made-up"),
                ("redirect_uri", CLAUDE_CALLBACK),
                ("code_verifier", VERIFIER),
            ],
            400,
            "invalid_grant",
        ),
        (
            vec![
                ("grant_type", "refresh_token"),
                ("client_id", CLAUDE),
                ("refresh_token", &made_up_refresh),
            ],
            400,
            "invalid_grant",
        ),
    ];
    for (form, status, error) in refusals {
        let (got, answer, _) = token(address, &form).await?;
        assert_eq!(
            (got, answer["error"].as_str()),
            (status, Some(error)),
            "{form:?}"
        );
    }

    // Other well-known documents do not exist
    assert_eq!(
        browser
            .get("/.well-known/openid-configuration")
            .await?
            .status,
        404
    );
    let answer = browser
        .get("/.well-known/oauth-authorization-server")
        .await?;
    let document: Value = serde_json::from_str(&answer.body)?;
    assert_eq!(
        document["token_endpoint"],
        format!("{}{}", origin(), metadata::TOKEN_PATH)
    );
    Ok(())
}

/// Starts a trial of the Telegram sign-in; Telegram's `state`.
async fn trial(browser: &mut Browser, fake: &Shared) -> Result<String> {
    let answer = browser.get(spike::TRY_PATH).await?;
    assert_eq!(answer.status, 303);
    let (_, query) = answer.location()?;
    let mut shared = fake.lock().unwrap_or_else(PoisonError::into_inner);
    shared.challenge = query.get("code_challenge").cloned();
    shared.nonce = query.get("nonce").cloned();
    query.get("state").cloned().context("a state")
}

/// The trial of the Telegram sign-in (staging): its attempts come back through the authorization
/// server's callback, which hands them over before reading the database (the pool here never
/// connects).
#[tokio::test]
async fn a_trial_sign_in_reads_the_telegram_id_from_the_id_token() -> Result<()> {
    let pool = PgPool::connect_lazy("postgres://unused@127.0.0.1:1/unused")?;
    let (address, fake) = serve(pool, true).await?;
    let mut browser = Browser::new(address);

    let state = trial(&mut browser, &fake).await?;
    let page = browser.telegram_answers(&state).await?;
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("Connexion Telegram réussie"));
    assert!(page.body.contains(&ALICE.to_string()), "the id claim");
    assert!(!page.body.contains("424242"), "never sub");
    assert!(page.body.contains("Dans la liste bêta : oui"));
    assert!(page.body.contains("Nonce renvoyé : oui"));
    assert!(page.body.contains("Client ID = identifiant du bot : oui"));
    assert!(
        page.body
            .contains("aud, exp, iat, id, iss, name, nonce, sub")
    );
    assert!(!page.body.contains("<b>Basile"), "the name is never shown");
    assert_eq!(page.header("cache-control"), Some("no-store"));
    assert_eq!(page.header("x-frame-options"), Some("DENY"));

    // Telegram's error, escaped; a code Telegram refuses
    let state = trial(&mut browser, &fake).await?;
    let page = browser
        .get(&format!(
            "{}?error=access_denied%3Cscript%3E&state={state}",
            telegram::CALLBACK_PATH
        ))
        .await?;
    assert_eq!(page.status, 400);
    assert!(
        page.body.contains("access_denied&lt;script&gt;"),
        "{}",
        page.body
    );
    let state = trial(&mut browser, &fake).await?;
    let page = browser
        .get(&format!(
            "{}?code=stolen&state={state}",
            telegram::CALLBACK_PATH
        ))
        .await?;
    assert_eq!(page.status, 502);
    assert!(page.body.contains("invalid_grant"), "{}", page.body);
    Ok(())
}
