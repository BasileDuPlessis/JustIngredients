//! OAuth 2.1 for the MCP door (`specs/tasks/oauth.md`): claude.ai and ChatGPT connect without a
//! key, the user signing in with Telegram. The bot is both the protected resource (`/mcp`) and
//! its authorization server, on one origin.
//!
//! This module serves the discovery documents ([`metadata`]) and the authorization server
//! ([`server`]) for the allowed assistants ([`clients`]), users signing in with Telegram
//! ([`telegram`]). Mounted only when the Telegram login is configured.

pub mod clients;
pub mod metadata;
pub mod page;
pub mod server;
pub mod telegram;

/// How often the sign-ins, codes and tokens that can no longer be used are deleted.
const PURGE_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Deletes, every few hours, what OAuth can no longer use (`engine::oauth::purge`); runs while
/// the bot does.
pub async fn run_purge(pool: sqlx::PgPool) {
    let mut every = tokio::time::interval(PURGE_EVERY);
    loop {
        every.tick().await;
        match crate::engine::oauth::purge(&pool).await {
            Ok(0) => {}
            Ok(deleted) => tracing::info!(deleted, "Purged expired OAuth rows"),
            Err(error) => tracing::warn!(error = %error, "Could not purge expired OAuth rows"),
        }
    }
}

/// The addresses OAuth is about, from the app's public host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSettings {
    /// The authorization server's issuer: the origin, without a path or a trailing slash
    /// (`https://just-ingredients.fly.dev`). Compared as an exact string by clients.
    pub issuer: String,
    /// The protected resource: the MCP door's URL as the user enters it in Claude or ChatGPT.
    pub resource: String,
}

impl OAuthSettings {
    /// For the app served at `https://{host}`.
    pub fn for_host(host: &str) -> Self {
        let issuer = format!("https://{host}");
        let resource = format!("{issuer}{}", super::mcp::PATH);
        Self { issuer, resource }
    }

    /// The URL of the protected resource metadata (RFC 9728, the well-known path inserted before
    /// the resource's path).
    pub fn resource_metadata_url(&self) -> String {
        format!(
            "{}/.well-known/oauth-protected-resource{}",
            self.issuer,
            super::mcp::PATH
        )
    }

    /// The `WWW-Authenticate` value of a 401 from the MCP door: where to discover how to sign
    /// in, and the scope needed to start.
    pub fn challenge(&self) -> String {
        format!(
            "Bearer resource_metadata=\"{}\", scope=\"{}\"",
            self.resource_metadata_url(),
            crate::engine::tokens::SCOPE_RECIPES_READ
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_addresses_come_from_the_host_without_trailing_slash() {
        let settings = OAuthSettings::for_host("just-ingredients.fly.dev");
        assert_eq!(settings.issuer, "https://just-ingredients.fly.dev");
        assert_eq!(settings.resource, "https://just-ingredients.fly.dev/mcp");
        assert_eq!(
            settings.resource_metadata_url(),
            "https://just-ingredients.fly.dev/.well-known/oauth-protected-resource/mcp"
        );
        assert_eq!(
            settings.challenge(),
            "Bearer resource_metadata=\"https://just-ingredients.fly.dev/.well-known/oauth-protected-resource/mcp\", scope=\"recipes:read\""
        );
    }
}
