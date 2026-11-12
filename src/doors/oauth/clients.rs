//! The assistants allowed to sign users in (`specs/tasks/oauth.md`, §7): known clients only, each
//! with its exact redirect URIs, pinned here rather than fetched from its Client ID Metadata
//! Document (no request forgery, no cache, nothing a client's server could change).
//!
//! Both are public clients: they authenticate at the token endpoint with PKCE alone
//! (`token_endpoint_auth_method` `none`; ChatGPT's document names `private_key_jwt` first and
//! lists `none` too). Claude Code is not here: it uses the personal keys of `/cle`.

/// A client allowed to sign users in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Client {
    /// Its client id: the URL of its metadata document, compared exactly.
    pub id: &'static str,
    /// The name the consent page shows: the host of its client id, not a self-declared name.
    pub name: &'static str,
    /// Where it may be sent back, compared exactly.
    pub redirect_uris: &'static [&'static str],
}

/// The allowed clients.
pub const CLIENTS: &[Client] = &[
    Client {
        id: "https://claude.ai/oauth/mcp-oauth-client-metadata",
        name: "claude.ai",
        redirect_uris: &["https://claude.ai/api/mcp/auth_callback"],
    },
    Client {
        id: "https://chatgpt.com/oauth/client.json",
        name: "chatgpt.com",
        redirect_uris: &["https://chatgpt.com/connector_platform_oauth_redirect"],
    },
];

/// The allowed client of `id`, if any.
pub fn find(id: &str) -> Option<&'static Client> {
    CLIENTS.iter().find(|client| client.id == id)
}

impl Client {
    /// Whether `redirect_uri` is one of the client's, exactly.
    pub fn allows_redirect(&self, redirect_uri: &str) -> bool {
        self.redirect_uris.contains(&redirect_uri)
    }

    /// The origin of a redirect URI (`https://claude.ai`), for the consent page's
    /// `form-action`: the browser follows our 303 to it after the form is posted.
    pub fn redirect_origin(redirect_uri: &str) -> Option<String> {
        let url = reqwest::Url::parse(redirect_uri).ok()?;
        (url.scheme() == "https").then(|| url.origin().ascii_serialization())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_clients_and_their_exact_redirects() {
        let claude = find("https://claude.ai/oauth/mcp-oauth-client-metadata");
        assert_eq!(claude.map(|client| client.name), Some("claude.ai"));
        assert!(claude.is_some_and(|client| {
            client.allows_redirect("https://claude.ai/api/mcp/auth_callback")
        }));
        for other in [
            "https://claude.ai/api/mcp/auth_callback/",
            "https://claude.ai/api/mcp/auth_callback?x=1",
            "https://evil.example/api/mcp/auth_callback",
            "http://claude.ai/api/mcp/auth_callback",
        ] {
            assert!(
                !claude.is_some_and(|client| client.allows_redirect(other)),
                "{other}"
            );
        }
        assert!(
            find("https://claude.ai/oauth/claude-code-client-metadata").is_none(),
            "Claude Code uses /cle keys"
        );
        assert!(find("https://chatgpt.com/oauth/client.json/").is_none());
        assert!(find("https://evil.example/client.json").is_none());
    }

    #[test]
    fn every_redirect_is_https_with_an_origin() {
        for client in CLIENTS {
            assert!(client.id.starts_with("https://"));
            for redirect in client.redirect_uris {
                assert!(Client::redirect_origin(redirect).is_some(), "{redirect}");
            }
        }
        assert_eq!(
            Client::redirect_origin("https://chatgpt.com/connector_platform_oauth_redirect")
                .as_deref(),
            Some("https://chatgpt.com")
        );
        assert_eq!(Client::redirect_origin("http://localhost:3000/cb"), None);
    }
}
