//! `/cle` (`/key`): a user's personal tokens for the doors other than Telegram (an MCP server
//! first): `specs/tasks/engine-api.md`, §4. During the beta, only the Telegram ids of
//! `API_BETA_USERS` see it; anyone else gets the unknown-command answer, as for `/stats`.
//!
//! The keys message lists the active tokens (their hint, never the secret) with "➕ Nouvelle clé"
//! and one "🗑️ Révoquer" button each. A new token is shown once, in a message of its own,
//! protected from forwarding, under a spoiler; that message is kept when actions end and is
//! edited to a masked text 10 minutes later ([`run_secret_sweeper`]): an edit works at any age,
//! unlike a delete, and the sweep reads the recorded messages, so a restart does not forget it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage};
use tracing::{info, warn};

use super::cleanup;
use super::html::{Html, edit_html, send_html};
use crate::engine::tokens::{self, Realm, TokenInfo};
use crate::engine::{EngineError, Owner};
use crate::localization::{LocalizationManager, t_args_lang, t_lang};

/// A token message is masked this long after it was sent.
pub const SHOWN_SECS: i32 = 600;

/// How often the masking runs.
const SWEEP_EVERY: Duration = Duration::from_secs(60);

/// A token message that could not be masked for this long is no longer tried.
const GIVE_UP_SECS: i32 = 7 * 86_400;

/// Callback data of "➕ Nouvelle clé".
const NEW_KEY: &str = "api_key_new";

/// Callback data of "🗑️ Révoquer …" starts with this, then the token's id.
const REVOKE_PREFIX: &str = "api_key_revoke:";

/// Who may create tokens, and where they are used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiAccess {
    /// The Telegram ids of the beta (`API_BETA_USERS`).
    pub beta_users: Vec<i64>,
    pub realm: Realm,
    /// The MCP server's address, told with a new token; `None` when not known (local runs).
    pub mcp_url: Option<String>,
}

impl ApiAccess {
    pub fn from_config(config: &crate::config::ApiConfig) -> Self {
        Self {
            beta_users: config.beta_users.clone(),
            realm: Realm::from_app_name(config.app_name.as_deref()),
            mcp_url: crate::doors::public_host(config.app_name.as_deref())
                .map(|host| format!("https://{host}{}", crate::doors::mcp::PATH)),
        }
    }

    pub fn allows(&self, telegram_id: i64) -> bool {
        self.beta_users.contains(&telegram_id)
    }
}

/// Whether a button's data belongs to the keys message.
pub fn is_key_button(data: &str) -> bool {
    data == NEW_KEY || data.starts_with(REVOKE_PREFIX)
}

/// `/cle`: the keys message at the bottom, with its buttons and the list button.
pub async fn show_keys(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    owner: Owner,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    let keys = tokens::list(pool, owner).await?;
    let (text, keyboard) = render_keys(&keys, localization, language_code);
    let sent = bot
        .send_message(chat_id, text)
        .reply_markup(keyboard)
        .disable_notification(true)
        .await?;
    super::home::keep_at_bottom(bot, chat_id, sent.id).await;
    Ok(())
}

/// A button of the keys message, pressed by `q.from`: a new token, or one revoked. Nothing for a
/// user outside the beta (a forged button).
pub async fn handle_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: &PgPool,
    dialogue: &crate::dialogue::RecipeDialogue,
    access: &ApiAccess,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let telegram_id = super::telegram_id(q.from.id);
    let Some(MaybeInaccessibleMessage::Regular(message)) = &q.message else {
        return Ok(());
    };
    if !access.allows(telegram_id) {
        return Ok(());
    }
    let owner = Owner::telegram(telegram_id);
    let chat_id = message.chat.id;
    let language_code = q.from.language_code.as_deref();

    if data == NEW_KEY {
        // Like a command, the new key ends a running action: the list below deletes its messages
        if dialogue.get().await?.is_some_and(|state| state.is_action()) {
            dialogue.exit().await?;
        }
        match tokens::create(pool, owner, access.realm).await {
            Ok(token) => {
                crate::observability::action(crate::observability::Action::ApiKeyCreated);
                info!(hint = %token.hint, "API token created");
                show_once(
                    bot,
                    pool,
                    chat_id,
                    owner,
                    &token,
                    access,
                    localization,
                    language_code,
                )
                .await;
            }
            // A button of an older list: the new one says how many there are
            Err(EngineError::LimitReached) => {}
            Err(error) => return Err(error.into()),
        }
        // The list again, below the token
        return show_keys(bot, pool, chat_id, owner, localization, language_code).await;
    }

    if let Some(id) = data.strip_prefix(REVOKE_PREFIX) {
        let id: i64 = id.parse().unwrap_or(0);
        match tokens::revoke(pool, owner, id).await {
            Ok(()) => {
                crate::observability::action(crate::observability::Action::ApiKeyRevoked);
                info!(token_id = id, "API token revoked");
            }
            // Revoked already, or not this user's: the list shows what is left
            Err(EngineError::NotFound) => {}
            Err(error) => return Err(error.into()),
        }
        let keys = tokens::list(pool, owner).await?;
        let (text, keyboard) = render_keys(&keys, localization, language_code);
        if let Err(error) = bot
            .edit_message_text(chat_id, message.id, text)
            .reply_markup(keyboard)
            .await
        {
            warn!(error = %error, "Could not update the keys message");
        }
    }
    Ok(())
}

/// Shows a new token once, recorded to be masked. A token whose message could not be sent (it
/// may have arrived: a timeout) or recorded (it would never be masked) is revoked, and a
/// recorded failure masks the message at once: no token stays readable without a way to hide it.
#[allow(clippy::too_many_arguments)]
async fn show_once(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    owner: Owner,
    token: &tokens::NewToken,
    access: &ApiAccess,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    let shown = match send_secret(bot, chat_id, token, access, localization, language_code).await {
        Ok(sent) if cleanup::record_secret(chat_id, sent.id).await => return,
        Ok(sent) => {
            if let Err(error) = mask(bot, chat_id, sent.id, localization, language_code).await {
                warn!(error = %error, "Could not mask an unrecorded token message");
            }
            "not recorded"
        }
        Err(error) => {
            warn!(error = %error, "Could not send a new token");
            "not sent"
        }
    };
    warn!(hint = %token.hint, reason = shown, "New token revoked: its message cannot be masked");
    if let Err(error) = tokens::revoke(pool, owner, token.id).await {
        warn!(error = %error, hint = %token.hint, "Could not revoke the token");
    }
}

/// Edits a token message to the masked text.
async fn mask(
    bot: &Bot,
    chat_id: ChatId,
    message_id: teloxide::types::MessageId,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<(), teloxide::RequestError> {
    let masked = Html::text(&t_lang(localization, "api-key-masked", language_code));
    edit_html(bot, chat_id, message_id, masked)
        .await
        .map(|_| ())
}

/// The token's message: monospace under a spoiler, protected from forwarding. A spoiler refused
/// by Telegram falls back to monospace alone.
async fn send_secret(
    bot: &Bot,
    chat_id: ChatId,
    token: &tokens::NewToken,
    access: &ApiAccess,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<teloxide::types::Message> {
    let compose = |secret: Html| {
        let mut html = Html::text(&t_args_lang(
            localization,
            "api-key-created",
            &[("hint", &token.hint)],
            language_code,
        )) + Html::text("\n\n")
            + secret;
        if let Some(url) = &access.mcp_url {
            html += Html::text(&format!(
                "\n\n{}\n",
                t_lang(localization, "api-key-server", language_code)
            )) + Html::code(url);
        }
        html += Html::text(&format!(
            "\n{}\n",
            t_lang(localization, "api-key-header", language_code)
        )) + Html::code("Authorization: Bearer …");
        html
    };
    let sent = match send_html(bot, chat_id, compose(Html::secret(&token.secret)))
        .protect_content(true)
        .await
    {
        Ok(sent) => sent,
        Err(teloxide::RequestError::Api(error)) => {
            warn!(error = %error, "The token message was refused with a spoiler: sent without");
            send_html(bot, chat_id, compose(Html::code(&token.secret)))
                .protect_content(true)
                .await?
        }
        Err(error) => return Err(error.into()),
    };
    Ok(sent)
}

/// The keys message: what a key is for, the active ones, and the buttons.
fn render_keys(
    keys: &[TokenInfo],
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> (String, InlineKeyboardMarkup) {
    let text_of = |key: &str| t_lang(localization, key, language_code);
    let mut text = format!(
        "{}\n\n{}\n\n",
        text_of("api-keys-title"),
        text_of("api-keys-explain")
    );
    if keys.is_empty() {
        text.push_str(&text_of("api-keys-none"));
    }
    let date = |at| super::ui_builder::format_recipe_date(at, language_code, true, localization);
    let lines: Vec<String> = keys
        .iter()
        .map(|key| {
            let used = key.last_used_at.map_or_else(
                || text_of("api-keys-never-used"),
                |at| {
                    t_args_lang(
                        localization,
                        "api-keys-used",
                        &[("date", &date(at))],
                        language_code,
                    )
                },
            );
            t_args_lang(
                localization,
                "api-keys-line",
                &[
                    ("hint", &key.hint),
                    ("created", &date(key.created_at)),
                    ("used", &used),
                ],
                language_code,
            )
        })
        .collect();
    text.push_str(&lines.join("\n"));

    let mut rows = Vec::new();
    if i64::try_from(keys.len()).unwrap_or(i64::MAX) < tokens::MAX_ACTIVE {
        rows.push(vec![InlineKeyboardButton::callback(
            text_of("api-keys-new-button"),
            NEW_KEY,
        )]);
    } else {
        text.push_str("\n\n");
        text.push_str(&t_args_lang(
            localization,
            "api-keys-limit",
            &[("max", &tokens::MAX_ACTIVE.to_string())],
            language_code,
        ));
    }
    for key in keys {
        rows.push(vec![InlineKeyboardButton::callback(
            t_args_lang(
                localization,
                "api-keys-revoke-button",
                &[("hint", &key.hint)],
                language_code,
            ),
            format!("{REVOKE_PREFIX}{}", key.id),
        )]);
    }
    rows.push(vec![super::ui_builder::recipes_list_button(
        language_code,
        localization,
    )]);
    (text, InlineKeyboardMarkup::new(rows))
}

/// Masks the token messages sent over [`SHOWN_SECS`] ago, now and then every minute. A message
/// Telegram cannot edit any more (deleted by the user, too old for its type) is forgotten; a
/// network failure leaves it for the next round.
pub async fn run_secret_sweeper(bot: Bot, localization: Arc<LocalizationManager>) {
    let mut interval = tokio::time::interval(SWEEP_EVERY);
    loop {
        interval.tick().await;
        sweep_secrets(&bot, &localization).await;
    }
}

/// One round of [`run_secret_sweeper`]. A message is forgotten once masked, or when Telegram
/// says it never can be (deleted, not editable, the chat gone or the bot blocked); any other
/// failure is tried again, for a week at most.
pub async fn sweep_secrets(bot: &Bot, localization: &Arc<LocalizationManager>) {
    use teloxide::ApiError;
    for (chat_id, message_id, language) in cleanup::secrets_older_than(SHOWN_SECS).await {
        match mask(bot, chat_id, message_id, localization, language.as_deref()).await {
            Ok(())
            | Err(teloxide::RequestError::Api(
                ApiError::MessageNotModified
                | ApiError::MessageToEditNotFound
                | ApiError::MessageCantBeEdited
                | ApiError::BotBlocked
                | ApiError::BotKicked
                | ApiError::ChatNotFound
                | ApiError::UserDeactivated,
            )) => cleanup::untrack(chat_id, message_id).await,
            Err(error) => warn!(error = %error, "Could not mask a token message yet"),
        }
    }
    let given_up = cleanup::forget_secrets_older_than(GIVE_UP_SECS).await;
    if given_up > 0 {
        warn!(
            messages = given_up,
            "Token messages not masked after a week: given up"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localization() -> Arc<LocalizationManager> {
        crate::localization::create_localization_manager().expect("localization")
    }

    fn key(id: i64) -> TokenInfo {
        TokenInfo {
            id,
            hint: format!("ji_Ab{id}xyz"),
            created_at: chrono::Utc::now(),
            last_used_at: None,
        }
    }

    fn buttons(keyboard: &InlineKeyboardMarkup) -> Vec<String> {
        keyboard
            .inline_keyboard
            .iter()
            .flatten()
            .map(|button| match &button.kind {
                teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => data.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn the_keys_message_offers_a_new_key_until_the_limit() {
        let localization = localization();
        let (text, keyboard) = render_keys(&[], &localization, Some("fr"));
        assert!(text.contains(&t_lang(&localization, "api-keys-none", Some("fr"))));
        assert_eq!(buttons(&keyboard), [NEW_KEY, "show_list"]);

        let two = [key(1), key(2)];
        let (text, keyboard) = render_keys(&two, &localization, Some("fr"));
        assert!(text.contains("ji_Ab1xyz"), "{text}");
        assert_eq!(
            buttons(&keyboard),
            [NEW_KEY, "api_key_revoke:1", "api_key_revoke:2", "show_list"]
        );

        let full: Vec<TokenInfo> = (1..=tokens::MAX_ACTIVE).map(key).collect();
        let (text, keyboard) = render_keys(&full, &localization, Some("fr"));
        assert!(!buttons(&keyboard).contains(&NEW_KEY.to_string()));
        assert!(text.contains(&tokens::MAX_ACTIVE.to_string()), "{text}");
    }

    #[test]
    fn the_keys_message_never_holds_a_secret_and_buttons_are_recognised() {
        assert!(is_key_button(NEW_KEY));
        assert!(is_key_button("api_key_revoke:12"));
        assert!(!is_key_button("recipe_action:note:1"));
    }

    #[test]
    fn access_follows_the_beta_list_and_the_app() {
        let access = ApiAccess::from_config(&crate::config::ApiConfig {
            beta_users: vec![42],
            app_name: Some("just-ingredients-staging".to_string()),
        });
        assert!(access.allows(42));
        assert!(!access.allows(43));
        assert_eq!(access.realm, Realm::Staging);
        assert_eq!(
            access.mcp_url.as_deref(),
            Some("https://just-ingredients-staging.fly.dev/mcp")
        );
        assert!(!ApiAccess::default().allows(42), "nobody by default");
    }
}
