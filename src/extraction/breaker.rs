//! Circuit breaker for the extraction service.
//!
//! ```text
//! CLOSED ──failures ≥ threshold──► OPEN ──open_duration elapsed──► HALF-OPEN
//!   ▲                                ▲                                 │
//!   └────────── trial succeeds ──────┼──────────── trial fails ────────┘
//! ```
//!
//! In HALF-OPEN a single trial call goes through; concurrent calls are rejected until it
//! reports. A trial whose permit is dropped without an outcome (cancelled request) frees the
//! slot for the next call instead of leaving the breaker stuck.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::extraction::ExtractionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { trial_in_flight: bool },
}

/// Stops calling a failing service for a while, then probes it with one call.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Mutex<State>,
    failure_threshold: u32,
    open_duration: Duration,
}

impl CircuitBreaker {
    /// Opens after `failure_threshold` consecutive failures, for `open_duration`.
    pub fn new(failure_threshold: u32, open_duration: Duration) -> Self {
        Self {
            state: Mutex::new(State::Closed { failures: 0 }),
            failure_threshold: failure_threshold.max(1),
            open_duration,
        }
    }

    /// Allows a call, or returns [`ExtractionError::CircuitOpen`]. Report the outcome on the
    /// returned permit.
    pub fn try_acquire(&self) -> Result<Permit<'_>, ExtractionError> {
        let mut state = self.lock();
        let trial = match *state {
            State::Closed { .. } => false,
            State::Open { until } if Instant::now() < until => {
                return Err(ExtractionError::CircuitOpen)
            }
            State::Open { .. }
            | State::HalfOpen {
                trial_in_flight: false,
            } => {
                *state = State::HalfOpen {
                    trial_in_flight: true,
                };
                true
            }
            State::HalfOpen {
                trial_in_flight: true,
            } => return Err(ExtractionError::CircuitOpen),
        };
        Ok(Permit {
            breaker: self,
            trial,
            reported: false,
        })
    }

    /// `true` while calls are rejected (open, or half-open with a trial in flight).
    pub fn is_open(&self) -> bool {
        match *self.lock() {
            State::Closed { .. }
            | State::HalfOpen {
                trial_in_flight: false,
            } => false,
            State::Open { until } => Instant::now() < until,
            State::HalfOpen {
                trial_in_flight: true,
            } => true,
        }
    }

    /// Ends the open period now, as if `open_duration` had elapsed (tests).
    #[cfg(test)]
    fn expire_open_period(&self) {
        let mut state = self.lock();
        if let State::Open { .. } = *state {
            *state = State::Open {
                until: Instant::now(),
            };
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // The state is a plain value: a panic while holding the lock cannot corrupt it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn on_success(&self) {
        *self.lock() = State::Closed { failures: 0 };
    }

    fn on_failure(&self, trial: bool) {
        let mut state = self.lock();
        let open = State::Open {
            until: Instant::now() + self.open_duration,
        };
        *state = match *state {
            _ if trial => open,
            State::Closed { failures } if failures + 1 >= self.failure_threshold => open,
            State::Closed { failures } => State::Closed {
                failures: failures + 1,
            },
            other => other,
        };
    }

    fn on_abandoned(&self, trial: bool) {
        if trial {
            let mut state = self.lock();
            if *state
                == (State::HalfOpen {
                    trial_in_flight: true,
                })
            {
                *state = State::HalfOpen {
                    trial_in_flight: false,
                };
            }
        }
    }
}

/// Permission for one call; report how it went with [`Permit::success`] or
/// [`Permit::failure`].
#[derive(Debug)]
pub struct Permit<'a> {
    breaker: &'a CircuitBreaker,
    trial: bool,
    reported: bool,
}

impl Permit<'_> {
    /// The service answered (even with a client error such as a blocked prompt).
    pub fn success(mut self) {
        self.reported = true;
        self.breaker.on_success();
    }

    /// The service failed (timeout, network error, rate limit, server error).
    pub fn failure(mut self) {
        self.reported = true;
        self.breaker.on_failure(self.trial);
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if !self.reported {
            self.breaker.on_abandoned(self.trial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Long enough that no CI pause can close the breaker during a test; tests that need the
    // open period to end call expire_open_period.
    const OPEN_FOR: Duration = Duration::from_secs(60);

    fn fail(breaker: &CircuitBreaker) -> Result<(), ExtractionError> {
        breaker.try_acquire().map(Permit::failure)
    }

    #[test]
    fn opens_after_consecutive_failures() -> Result<(), ExtractionError> {
        let breaker = CircuitBreaker::new(3, OPEN_FOR);
        fail(&breaker)?;
        fail(&breaker)?;
        assert!(!breaker.is_open());
        fail(&breaker)?;
        assert!(breaker.is_open());
        assert!(matches!(
            breaker.try_acquire(),
            Err(ExtractionError::CircuitOpen)
        ));
        Ok(())
    }

    #[test]
    fn a_success_resets_the_failure_count() -> Result<(), ExtractionError> {
        let breaker = CircuitBreaker::new(2, OPEN_FOR);
        fail(&breaker)?;
        breaker.try_acquire()?.success();
        fail(&breaker)?;
        assert!(!breaker.is_open());
        Ok(())
    }

    #[test]
    fn half_open_lets_one_trial_through() -> Result<(), ExtractionError> {
        let breaker = CircuitBreaker::new(1, OPEN_FOR);
        fail(&breaker)?;
        breaker.expire_open_period();

        let trial = breaker.try_acquire()?;
        assert!(matches!(
            breaker.try_acquire(),
            Err(ExtractionError::CircuitOpen)
        ));
        trial.success();
        assert!(!breaker.is_open());
        breaker.try_acquire()?.success();
        Ok(())
    }

    #[test]
    fn a_failed_trial_reopens_immediately() -> Result<(), ExtractionError> {
        let breaker = CircuitBreaker::new(3, OPEN_FOR);
        for _ in 0..3 {
            fail(&breaker)?;
        }
        breaker.expire_open_period();
        fail(&breaker)?;
        assert!(breaker.is_open(), "one failed trial is enough");
        Ok(())
    }

    #[test]
    fn an_abandoned_trial_frees_the_slot() -> Result<(), ExtractionError> {
        let breaker = CircuitBreaker::new(1, OPEN_FOR);
        fail(&breaker)?;
        breaker.expire_open_period();
        drop(breaker.try_acquire()?);
        breaker.try_acquire()?.success();
        assert!(!breaker.is_open());
        Ok(())
    }
}
