//! # Telegram Connection
//!
//! Checks the bot token with `getMe` before the dispatcher starts. teloxide panics when its own
//! first `getMe` fails; here an unreachable Telegram API is retried with backoff and a rejected
//! token ends the process with one clear error.

use std::future::Future;
use std::time::Duration;

use teloxide::types::Me;
use teloxide::{ApiError, RequestError};
use tracing::{info, warn};

use crate::retry::{retry_with_backoff, Backoff, RetryDecision};

/// Retry schedule while Telegram is unreachable at startup: 1 s doubling up to 30 s, for at most
/// 10 minutes, then the process exits and the restart policy starts it again.
pub const TELEGRAM_CONNECT_BACKOFF: Backoff = Backoff {
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(30),
    max_elapsed: Duration::from_secs(600),
    jitter: 0.0,
};

/// Calls `getMe` until Telegram answers.
///
/// # Errors
///
/// Returns an error at once when Telegram rejects the token, or after `backoff` when the API
/// stays unreachable.
pub async fn connect_with_retry(bot: &teloxide::Bot, backoff: Backoff) -> anyhow::Result<Me> {
    use teloxide::requests::{Request, Requester};
    let me = retry_get_me(backoff, || bot.get_me().send())
        .await
        .map_err(startup_error)?;
    info!(username = %me.username(), "Connected to Telegram");
    Ok(me)
}

/// Runs `attempt` until it succeeds, retrying everything except a rejected token.
async fn retry_get_me<F, Fut>(backoff: Backoff, attempt: F) -> Result<Me, RequestError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Me, RequestError>>,
{
    retry_with_backoff(
        backoff,
        retry_decision,
        |retry, delay, error| {
            warn!(
                retry,
                retry_in_ms = delay.as_millis(),
                error = %error,
                "Telegram API unavailable, retrying"
            );
        },
        attempt,
    )
    .await
}

fn retry_decision(error: &RequestError) -> RetryDecision {
    match error {
        RequestError::Api(ApiError::InvalidToken) => RetryDecision::Stop,
        RequestError::RetryAfter(seconds) => RetryDecision::RetryAfter(seconds.duration()),
        _ => RetryDecision::Retry,
    }
}

fn startup_error(error: RequestError) -> anyhow::Error {
    match error {
        RequestError::Api(ApiError::InvalidToken) => anyhow::anyhow!(
            "Telegram rejected TELEGRAM_BOT_TOKEN (revoked or mistyped): set the token given by @BotFather"
        ),
        error => anyhow::anyhow!("Telegram API still unreachable after retrying: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use teloxide::types::Seconds;

    const FAST: Backoff = Backoff {
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(5),
        max_elapsed: Duration::from_millis(200),
        jitter: 0.0,
    };

    fn network_error() -> RequestError {
        RequestError::Io(Arc::new(std::io::Error::from(
            std::io::ErrorKind::ConnectionRefused,
        )))
    }

    fn me() -> Me {
        serde_json::from_value(serde_json::json!({
            "id": 1,
            "is_bot": true,
            "first_name": "JustIngredients",
            "username": "just_ingredients_bot",
            "can_join_groups": false,
            "can_read_all_group_messages": false,
            "supports_inline_queries": false,
            "can_connect_to_business": false,
            "has_main_web_app": false
        }))
        .expect("valid getMe answer")
    }

    #[test]
    fn only_a_rejected_token_stops_the_retries() {
        assert_eq!(
            retry_decision(&RequestError::Api(ApiError::InvalidToken)),
            RetryDecision::Stop
        );
        assert_eq!(retry_decision(&network_error()), RetryDecision::Retry);
        assert_eq!(
            retry_decision(&RequestError::RetryAfter(Seconds::from_seconds(3))),
            RetryDecision::RetryAfter(Duration::from_secs(3))
        );
    }

    #[tokio::test]
    async fn unreachable_telegram_is_retried_until_it_answers() {
        let calls = AtomicU32::new(0);
        let result = retry_get_me(FAST, || {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if call < 2 {
                    Err(network_error())
                } else {
                    Ok(me())
                }
            }
        })
        .await;
        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rejected_token_fails_at_once_with_a_clear_message() {
        let calls = AtomicU32::new(0);
        let error = retry_get_me(FAST, || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(RequestError::Api(ApiError::InvalidToken)) }
        })
        .await
        .map_err(startup_error)
        .expect_err("token rejected");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(error.to_string().contains("TELEGRAM_BOT_TOKEN"));
    }
}
