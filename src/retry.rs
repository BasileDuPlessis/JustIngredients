//! # Retry
//!
//! Retries an async operation with exponential backoff. The caller decides, per error,
//! whether to stop, retry, or retry after a delay given by the server (`Retry-After`).
//! Used by the database connection at startup and by the Gemini client.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::time::{Duration, Instant};

/// Exponential backoff schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backoff {
    /// Delay before the first retry.
    pub initial_delay: Duration,
    /// Upper bound for the delay between two attempts.
    pub max_delay: Duration,
    /// Give up when the next retry would start after this much time since the first attempt.
    pub max_elapsed: Duration,
    /// Random reduction of each delay, from 0.0 (none) to 1.0, so that clients failing at the
    /// same time do not retry at the same time.
    pub jitter: f64,
}

impl Backoff {
    /// Delay before retry number `retry` (1-based): doubles each time, capped at `max_delay`.
    pub fn delay_before_retry(&self, retry: u32) -> Duration {
        let factor = 2u32.saturating_pow(retry.saturating_sub(1));
        self.initial_delay
            .saturating_mul(factor)
            .min(self.max_delay)
    }
}

/// What to do after a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Return the error.
    Stop,
    /// Retry after the backoff delay.
    Retry,
    /// Retry after at least this delay (e.g. from `Retry-After`); stop if the server asks to
    /// wait longer than `max_delay`.
    RetryAfter(Duration),
}

/// Runs `attempt` until it succeeds, `decide` returns [`RetryDecision::Stop`], or the next
/// retry would start after `backoff.max_elapsed`. `on_retry` is called before each wait with
/// the retry number (1-based), the delay and the error, typically to log it.
///
/// # Errors
///
/// Returns the last error of `attempt`.
pub async fn retry_with_backoff<T, E, F, Fut>(
    backoff: Backoff,
    mut decide: impl FnMut(&E) -> RetryDecision,
    mut on_retry: impl FnMut(u32, Duration, &E),
    mut attempt: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let started = Instant::now();
    let mut retry = 0;

    loop {
        let error = match attempt().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        retry += 1;
        let backoff_delay = jittered(backoff.delay_before_retry(retry), backoff.jitter);
        let delay = match decide(&error) {
            RetryDecision::Stop => return Err(error),
            RetryDecision::Retry => backoff_delay,
            // Never sooner than the server allows, never sooner than our own backoff.
            RetryDecision::RetryAfter(server) if server <= backoff.max_delay => {
                server.max(backoff_delay)
            }
            RetryDecision::RetryAfter(_) => return Err(error),
        };
        if started.elapsed() + delay > backoff.max_elapsed {
            return Err(error);
        }
        on_retry(retry, delay, &error);
        tokio::time::sleep(delay).await;
    }
}

/// Reduces `delay` by a random fraction of up to `jitter` (clamped to 0..=1).
fn jittered(delay: Duration, jitter: f64) -> Duration {
    let jitter = jitter.clamp(0.0, 1.0);
    if jitter == 0.0 {
        return delay;
    }
    delay.mul_f64(1.0 - jitter * rand::random::<f64>())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Backoff = Backoff {
        initial_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(20),
        max_elapsed: Duration::from_millis(500),
        jitter: 0.0,
    };

    #[test]
    fn delay_doubles_and_is_capped() {
        let backoff = Backoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_elapsed: Duration::from_secs(600),
            jitter: 0.0,
        };
        let delays: Vec<u64> = (1..=7)
            .map(|retry| backoff.delay_before_retry(retry).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30]);
        assert_eq!(backoff.delay_before_retry(u32::MAX), backoff.max_delay);
    }

    #[tokio::test]
    async fn retries_until_success() {
        let mut attempts = 0;
        let mut retries = Vec::new();
        let result = retry_with_backoff(
            FAST,
            |_: &&str| RetryDecision::Retry,
            |retry, _, _| retries.push(retry),
            || {
                attempts += 1;
                let outcome = if attempts < 3 {
                    Err("down")
                } else {
                    Ok(attempts)
                };
                async move { outcome }
            },
        )
        .await;
        assert_eq!(result, Ok(3));
        assert_eq!(retries, vec![1, 2]);
    }

    #[tokio::test]
    async fn stop_decision_returns_the_first_error() {
        let mut attempts = 0;
        let result: Result<(), &str> = retry_with_backoff(
            FAST,
            |_| RetryDecision::Stop,
            |_, _, _| {},
            || {
                attempts += 1;
                async { Err("bad request") }
            },
        )
        .await;
        assert_eq!(result, Err("bad request"));
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn retries_stop_once_max_elapsed_is_reached() {
        let mut attempts = 0;
        let started = Instant::now();
        let result: Result<(), &str> = retry_with_backoff(
            FAST,
            |_| RetryDecision::Retry,
            |_, _, _| {},
            || {
                attempts += 1;
                async { Err("down") }
            },
        )
        .await;
        assert!(result.is_err());
        assert!(attempts > 3, "retried: {attempts} attempts");
        assert!(started.elapsed() < Duration::from_secs(2), "bounded");
    }

    #[tokio::test]
    async fn server_delays_are_a_minimum() {
        let mut delays = Vec::new();
        let mut attempts = 0;
        let backoff = Backoff {
            initial_delay: Duration::from_millis(15),
            ..FAST
        };
        let _: Result<(), &str> = retry_with_backoff(
            backoff,
            |_| RetryDecision::RetryAfter(Duration::ZERO),
            |_, delay, _| delays.push(delay),
            || {
                attempts += 1;
                let outcome = if attempts < 2 { Err("busy") } else { Ok(()) };
                async move { outcome }
            },
        )
        .await;
        assert_eq!(
            delays,
            vec![Duration::from_millis(15)],
            "Retry-After: 0 is not a tight loop"
        );
    }

    #[tokio::test]
    async fn server_delays_beyond_max_delay_stop_at_once() {
        let mut attempts = 0;
        let started = Instant::now();
        let result: Result<(), &str> = retry_with_backoff(
            FAST,
            |_| RetryDecision::RetryAfter(Duration::from_secs(60)),
            |_, _, _| {},
            || {
                attempts += 1;
                async { Err("come back in a minute") }
            },
        )
        .await;
        assert_eq!(result, Err("come back in a minute"));
        assert_eq!(attempts, 1);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn jitter_only_shortens_delays() {
        let delay = Duration::from_millis(100);
        assert_eq!(jittered(delay, 0.0), delay);
        for _ in 0..100 {
            let jittered = jittered(delay, 0.5);
            assert!(jittered <= delay && jittered >= delay / 2, "{jittered:?}");
        }
    }
}
