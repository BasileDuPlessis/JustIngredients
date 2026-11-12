//! The discovery documents (`specs/tasks/oauth.md`, §2): what a client reads first, after the
//! 401 of the MCP door.
//!
//! - Protected resource metadata (RFC 9728) at `/.well-known/oauth-protected-resource/mcp` (the
//!   well-known path inserted before the resource's path) and at the root form: the resource and
//!   its one authorization server.
//! - Authorization server metadata (RFC 8414) at `/.well-known/oauth-authorization-server`
//!   (the issuer has no path). No `openid-configuration`: the bot is not an OpenID provider.
//!
//! Public, read only, the same for everyone: served without a token, with a CORS header so that
//! browser-based tools can read them.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use super::OAuthSettings;
use crate::engine::tokens::{SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE};

/// The path of the authorization endpoint.
pub const AUTHORIZE_PATH: &str = "/oauth/authorize";

/// The path of the token endpoint.
pub const TOKEN_PATH: &str = "/oauth/token";

/// The discovery documents' routes.
pub fn router(settings: Arc<OAuthSettings>) -> Router {
    Router::new()
        .route(
            &format!(
                "/.well-known/oauth-protected-resource{}",
                crate::doors::mcp::PATH
            ),
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        // Any other well-known document (`openid-configuration`…) does not exist: said plainly,
        // not by the health server's fallback
        .route("/.well-known/{*rest}", get(not_found))
        .with_state(settings)
}

/// RFC 9728: the resource (exactly the URL the user enters), its authorization server, the scopes.
pub fn protected_resource_document(settings: &OAuthSettings) -> serde_json::Value {
    json!({
        "resource": settings.resource,
        "authorization_servers": [settings.issuer],
        "scopes_supported": [SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE],
        "bearer_methods_supported": ["header"],
        "resource_name": "JustIngredients",
    })
}

/// RFC 8414: the endpoints, PKCE S256 only, public clients (`none`) known by their Client ID
/// Metadata Document, `iss` on authorization responses (RFC 9207).
pub fn authorization_server_document(settings: &OAuthSettings) -> serde_json::Value {
    json!({
        "issuer": settings.issuer,
        "authorization_endpoint": format!("{}{AUTHORIZE_PATH}", settings.issuer),
        "token_endpoint": format!("{}{TOKEN_PATH}", settings.issuer),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
        "scopes_supported": [SCOPE_RECIPES_READ, SCOPE_RECIPES_WRITE],
    })
}

async fn protected_resource(State(settings): State<Arc<OAuthSettings>>) -> Response {
    document(&protected_resource_document(&settings))
}

async fn authorization_server(State(settings): State<Arc<OAuthSettings>>) -> Response {
    document(&authorization_server_document(&settings))
}

async fn not_found() -> Response {
    (
        axum::http::StatusCode::NOT_FOUND,
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=300"),
        )],
        "Not found",
    )
        .into_response()
}

fn document(value: &serde_json::Value) -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            ),
        ],
        value.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documents_name_one_issuer_and_the_exact_resource() {
        let settings = OAuthSettings::for_host("just-ingredients.fly.dev");
        let resource = protected_resource_document(&settings);
        assert_eq!(resource["resource"], "https://just-ingredients.fly.dev/mcp");
        assert_eq!(
            resource["authorization_servers"],
            json!(["https://just-ingredients.fly.dev"])
        );
        assert!(
            !resource["scopes_supported"]
                .as_array()
                .is_some_and(|scopes| scopes.iter().any(|scope| scope == "offline_access")),
            "no offline_access in the resource's scopes"
        );

        let server = authorization_server_document(&settings);
        assert_eq!(
            server["issuer"], resource["authorization_servers"][0],
            "the same exact string"
        );
        assert_eq!(
            server["authorization_endpoint"],
            "https://just-ingredients.fly.dev/oauth/authorize"
        );
        assert_eq!(server["code_challenge_methods_supported"], json!(["S256"]));
        assert_eq!(
            server["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
        assert_eq!(server["client_id_metadata_document_supported"], true);
        assert_eq!(
            server["authorization_response_iss_parameter_supported"],
            true
        );
        assert!(server.get("registration_endpoint").is_none(), "no DCR");
    }
}
