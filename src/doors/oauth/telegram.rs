//! Signing in with Telegram (`specs/tasks/oauth.md`, §2 and §7): the bot is an OpenID Connect
//! client of `https://oauth.telegram.org` ("Log In With Telegram"). The user's browser goes to
//! Telegram's `/auth` with our PKCE challenge and a random `state`; Telegram sends it back to
//! [`CALLBACK_PATH`] with a code, exchanged server side with the login secret from BotFather.
//!
//! The ID token comes straight from Telegram's token endpoint over TLS, so its signature is not
//! checked (OIDC Core §3.1.3.7, 6); its issuer, audience and expiry are. The user is its `id`
//! claim (scope `profile`), the Telegram id the bot knows; `sub` is another number.

use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub use crate::config::TelegramLoginConfig as LoginConfig;
use crate::secret::SecretString;

/// Telegram's OpenID provider.
pub const ISSUER: &str = "https://oauth.telegram.org";

/// Where Telegram sends the user back (registered in BotFather, Login Widget, Allowed URLs).
pub const CALLBACK_PATH: &str = "/oauth/telegram/callback";

/// The scopes asked of Telegram: the user's id, nothing else is used.
pub const SCOPES: &str = "openid profile";

/// Longest wait for Telegram's token endpoint.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Largest answer read from the token endpoint.
const MAX_ANSWER_BYTES: usize = 16 * 1024;

/// The provider's addresses: Telegram's, or a test server's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub issuer: String,
    pub authorize: String,
    pub token: String,
}

impl Endpoints {
    pub fn telegram() -> Self {
        Self {
            issuer: ISSUER.to_string(),
            authorize: format!("{ISSUER}/auth"),
            token: format!("{ISSUER}/token"),
        }
    }
}

/// Why a sign-in with Telegram failed. Never carries a code, a token or the secret.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoginError {
    #[error("Telegram could not be reached: {0}")]
    Unreachable(String),
    #[error("Telegram refused the code: {0}")]
    Refused(String),
    #[error("Telegram's ID token was refused: {0}")]
    BadToken(&'static str),
}

/// What the ID token says, once checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The user's Telegram id (the `id` claim).
    pub telegram_id: i64,
    /// The `nonce` claim, when Telegram returned one.
    pub nonce: Option<String>,
    /// The signing algorithm the token's header names (not checked, see the module).
    pub algorithm: Option<String>,
    /// The account's name for the consent page: `name`, else `@preferred_username` (64
    /// characters at most). Never logged.
    pub display_name: Option<String>,
    /// The names of the claims received, sorted (for the spike's report and logs: no values).
    pub claim_names: Vec<String>,
}

/// One sign-in's secrets with Telegram: the `state`, the PKCE verifier, the nonce. Never
/// printed.
#[derive(Clone)]
pub struct Attempt {
    pub state: String,
    pub verifier: SecretString,
    pub nonce: String,
}

impl std::fmt::Debug for Attempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Attempt([REDACTED])")
    }
}

impl Attempt {
    /// Fresh random values (32 bytes each, base64url: 43 characters, as PKCE wants).
    pub fn new() -> Self {
        Self {
            state: random(),
            verifier: SecretString::new(random()),
            nonce: random(),
        }
    }

    /// The PKCE S256 challenge of the verifier.
    pub fn challenge(&self) -> String {
        challenge(self.verifier.expose())
    }
}

impl Default for Attempt {
    fn default() -> Self {
        Self::new()
    }
}

/// The S256 challenge of a PKCE verifier: base64url(SHA-256(verifier)), no padding.
pub fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The bot as Telegram's OpenID client.
#[derive(Debug, Clone)]
pub struct TelegramLogin {
    config: LoginConfig,
    endpoints: Endpoints,
    redirect_uri: String,
    http: reqwest::Client,
}

impl TelegramLogin {
    /// The login for the app whose origin is `issuer` (ours, `https://…fly.dev`).
    pub fn new(config: LoginConfig, issuer: &str, endpoints: Endpoints) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?;
        Ok(Self {
            config,
            endpoints,
            redirect_uri: format!("{issuer}{CALLBACK_PATH}"),
            http,
        })
    }

    pub fn client_id(&self) -> &str {
        &self.config.client_id
    }

    /// The address of Telegram's sign-in page for `attempt`.
    pub fn authorize_url(&self, attempt: &Attempt) -> anyhow::Result<String> {
        let challenge = attempt.challenge();
        let url = reqwest::Url::parse_with_params(
            &self.endpoints.authorize,
            [
                ("client_id", self.config.client_id.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", SCOPES),
                ("state", attempt.state.as_str()),
                ("nonce", attempt.nonce.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )?;
        Ok(url.into())
    }

    /// Exchanges the code Telegram sent back for the user's checked identity.
    pub async fn exchange(
        &self,
        code: &str,
        verifier: &SecretString,
    ) -> Result<Identity, LoginError> {
        let response = self
            .http
            .post(&self.endpoints.token)
            .basic_auth(&self.config.client_id, Some(self.config.secret.expose()))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("client_id", self.config.client_id.as_str()),
                ("code_verifier", verifier.expose()),
            ])
            .send()
            .await
            .map_err(|error| LoginError::Unreachable(without_url(&error)))?;
        let status = response.status();
        let body = read_limited(response).await?;
        let answer: TokenAnswer = serde_json::from_slice(&body).map_err(|_| {
            if status.is_success() {
                LoginError::BadToken("the answer is not JSON")
            } else {
                LoginError::Refused(format!("HTTP {}", status.as_u16()))
            }
        })?;
        if let Some(error) = answer.error {
            return Err(LoginError::Refused(short(&error)));
        }
        if !status.is_success() {
            return Err(LoginError::Refused(format!("HTTP {}", status.as_u16())));
        }
        let id_token = answer.id_token.ok_or(LoginError::BadToken("no ID token"))?;
        check_id_token(
            &id_token,
            &self.endpoints.issuer,
            &self.config.client_id,
            now(),
        )
    }
}

#[derive(Deserialize)]
struct TokenAnswer {
    id_token: Option<String>,
    error: Option<String>,
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, LoginError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| LoginError::Unreachable(without_url(&error)))?
    {
        if body.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err(LoginError::BadToken("the answer is too long"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// A request error without its URL (it has no secret, but keeps logs short).
fn without_url(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timeout".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else {
        "request failed".to_string()
    }
}

/// An error code from Telegram, kept short and printable.
fn short(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_graphic())
        .take(64)
        .collect()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Checks an ID token received from the token endpoint: its issuer, that it is for us (`aud`),
/// its expiry at `now` (Unix seconds); reads the user's id. The signature is not checked (see
/// the module).
pub fn check_id_token(
    token: &str,
    issuer: &str,
    client_id: &str,
    now: i64,
) -> Result<Identity, LoginError> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(LoginError::BadToken("not a JWT"));
    };
    let decode = |part: &str| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(part.trim_end_matches('='))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    };
    let header = decode(header).ok_or(LoginError::BadToken("unreadable header"))?;
    let claims = decode(payload).ok_or(LoginError::BadToken("unreadable claims"))?;
    let claims = claims
        .as_object()
        .ok_or(LoginError::BadToken("the claims are not an object"))?;

    if claims.get("iss").and_then(|iss| iss.as_str()) != Some(issuer) {
        return Err(LoginError::BadToken("another issuer"));
    }
    let names_us = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => text == client_id,
        serde_json::Value::Number(number) => number.to_string() == client_id,
        _ => false,
    };
    let for_us = match claims.get("aud") {
        Some(serde_json::Value::Array(audiences)) => audiences.iter().any(names_us),
        Some(audience) => names_us(audience),
        None => false,
    };
    if !for_us {
        return Err(LoginError::BadToken("not for this bot"));
    }
    match claims.get("exp").and_then(serde_json::Value::as_i64) {
        Some(expiry) if expiry > now => {}
        Some(_) => return Err(LoginError::BadToken("expired")),
        None => return Err(LoginError::BadToken("no expiry")),
    }
    let telegram_id = match claims.get("id") {
        Some(serde_json::Value::Number(number)) => number.as_i64(),
        Some(serde_json::Value::String(text)) => text.parse().ok(),
        _ => None,
    }
    .filter(|id| *id > 0)
    .ok_or(LoginError::BadToken("no Telegram id (scope profile)"))?;

    let text_claim = |name: &str| {
        claims
            .get(name)
            .and_then(|value| value.as_str())
            .map(|text| {
                text.trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(64)
                    .collect::<String>()
            })
            .filter(|text| !text.is_empty())
    };
    let display_name = text_claim("name")
        .or_else(|| text_claim("preferred_username").map(|username| format!("@{username}")));
    let mut claim_names: Vec<String> = claims.keys().cloned().collect();
    claim_names.sort();
    Ok(Identity {
        telegram_id,
        nonce: claims
            .get("nonce")
            .and_then(|nonce| nonce.as_str())
            .map(str::to_string),
        algorithm: header.get("alg").and_then(|alg| alg.as_str()).map(short),
        display_name,
        claim_names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jwt(claims: &serde_json::Value) -> String {
        let encode = |value: &serde_json::Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
        };
        format!(
            "{}.{}.c2lnbmF0dXJl",
            encode(&json!({ "alg": "RS256", "typ": "JWT" })),
            encode(claims)
        )
    }

    fn claims() -> serde_json::Value {
        json!({
            "iss": ISSUER, "aud": "123456", "sub": "998877665544", "iat": 1_000, "exp": 2_000,
            "id": 8_194_026_187_i64, "name": "Basile", "nonce": "n-1"
        })
    }

    #[test]
    fn a_good_token_gives_the_id_claim_not_sub() {
        let identity = check_id_token(&jwt(&claims()), ISSUER, "123456", 1_500);
        let identity = identity.as_ref().map_err(ToString::to_string);
        assert_eq!(
            identity.map(|identity| identity.telegram_id),
            Ok(8_194_026_187)
        );
        let identity = check_id_token(&jwt(&claims()), ISSUER, "123456", 1_500).ok();
        assert_eq!(
            identity
                .as_ref()
                .and_then(|identity| identity.nonce.as_deref()),
            Some("n-1")
        );
        assert_eq!(
            identity
                .as_ref()
                .and_then(|identity| identity.algorithm.as_deref()),
            Some("RS256")
        );
        assert_eq!(
            identity.map(|identity| identity.claim_names),
            Some(
                ["aud", "exp", "iat", "id", "iss", "name", "nonce", "sub"]
                    .map(String::from)
                    .to_vec()
            )
        );
    }

    #[test]
    fn the_audience_may_be_a_number_or_a_list() {
        for audience in [json!(123_456), json!(["other", "123456"])] {
            let mut claims = claims();
            claims["aud"] = audience.clone();
            assert!(
                check_id_token(&jwt(&claims), ISSUER, "123456", 1_500).is_ok(),
                "{audience}"
            );
        }
    }

    #[test]
    fn each_failed_check_refuses_the_token() {
        let refused = |change: &dyn Fn(&mut serde_json::Value), now: i64| {
            let mut claims = claims();
            change(&mut claims);
            check_id_token(&jwt(&claims), ISSUER, "123456", now).err()
        };
        assert_eq!(
            refused(
                &|claims| claims["iss"] = json!("https://evil.example"),
                1_500
            ),
            Some(LoginError::BadToken("another issuer"))
        );
        assert_eq!(
            refused(&|claims| claims["aud"] = json!("654321"), 1_500),
            Some(LoginError::BadToken("not for this bot"))
        );
        assert_eq!(
            refused(&|_| {}, 2_000),
            Some(LoginError::BadToken("expired"))
        );
        assert_eq!(
            refused(
                &|claims| {
                    if let Some(claims) = claims.as_object_mut() {
                        claims.remove("id");
                    }
                },
                1_500
            ),
            Some(LoginError::BadToken("no Telegram id (scope profile)"))
        );
        assert_eq!(
            check_id_token("a.b", ISSUER, "123456", 1_500).err(),
            Some(LoginError::BadToken("not a JWT"))
        );
        assert_eq!(
            check_id_token("e30.!!!.c2ln", ISSUER, "123456", 1_500).err(),
            Some(LoginError::BadToken("unreadable claims"))
        );
    }

    #[test]
    fn the_challenge_is_rfc_7636s_example() {
        // RFC 7636, appendix B
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let attempt = Attempt::new();
        assert_eq!(attempt.verifier.expose().len(), 43);
        assert_ne!(attempt.state, Attempt::new().state);
    }

    #[test]
    fn the_sign_in_address_has_pkce_and_our_callback() -> anyhow::Result<()> {
        let login = TelegramLogin::new(
            LoginConfig {
                client_id: "123456".to_string(),
                secret: SecretString::new("s"),
            },
            "https://just-ingredients-staging.fly.dev",
            Endpoints::telegram(),
        )?;
        let attempt = Attempt::new();
        let url = reqwest::Url::parse(&login.authorize_url(&attempt)?)?;
        assert_eq!(
            url.as_str().split('?').next(),
            Some("https://oauth.telegram.org/auth")
        );
        let query: std::collections::HashMap<String, String> =
            url.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("https://just-ingredients-staging.fly.dev/oauth/telegram/callback")
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile")
        );
        assert_eq!(
            query.get("code_challenge"),
            Some(&challenge(attempt.verifier.expose()))
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(
            !url.as_str().contains(attempt.verifier.expose()),
            "never the verifier"
        );
        Ok(())
    }
}
