//! # Circuit Breaker Module
//!
//! This module implements the circuit breaker pattern for OCR operations.
//! It prevents cascading failures by temporarily stopping requests when
//! OCR operations fail repeatedly.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::ocr_config::RecoveryConfig;

/// Circuit breaker for OCR operations
///
/// Implements the circuit breaker pattern to prevent cascading failures in OCR processing.
/// This pattern protects the system by temporarily stopping requests when OCR operations
/// fail repeatedly, allowing time for recovery.
///
/// ## State Machine Algorithm
///
/// The circuit breaker operates with three states and transitions based on failure patterns:
///
/// ```text
/// CLOSED ────failures ≥ threshold────► OPEN
///    ▲                                      │
///    │                                      │
///    └─────────reset timeout───────────────┘
///                    │
///                    ▼
///                 HALF-OPEN ───success───► CLOSED
///                    │
///                    └────failure───────► OPEN
/// ```
///
/// ## State Transitions
///
/// - **CLOSED → OPEN**: When failure count reaches `circuit_breaker_threshold`
/// - **OPEN → HALF-OPEN**: After `circuit_breaker_reset_secs` timeout elapses
/// - **HALF-OPEN → CLOSED**: On first successful operation
/// - **HALF-OPEN → OPEN**: On operation failure during testing
///
/// ## Failure Threshold Logic
///
/// The circuit breaker opens when the failure count reaches the configured threshold.
/// It automatically resets after the specified timeout period to allow testing of service recovery.
///
/// ```text
/// if failure_count >= threshold {
///     if time_since_last_failure < reset_timeout {
///         return OPEN;  // Block requests
///     } else {
///         reset_counter();  // Allow testing
///         return CLOSED;
///     }
/// }
/// return CLOSED;  // Normal operation
/// ```
///
/// ## Thread Safety
///
/// All state mutations use `Mutex<T>` for thread-safe access:
/// - `failure_count`: Tracks consecutive failures
/// - `last_failure_time`: Timestamp of most recent failure
/// - Atomic state transitions prevent race conditions
///
/// ## Configuration Parameters
///
/// - `circuit_breaker_threshold`: Failures before opening (default: 5)
/// - `circuit_breaker_reset_secs`: Seconds before reset attempt (default: 60)
///
/// ## Benefits
///
/// - **Fast Failure**: Prevents wasting resources on failing operations
/// - **Automatic Recovery**: Self-healing after timeout period
/// - **Load Protection**: Prevents cascade failures during outages
/// - **Configurable**: Adjustable thresholds for different environments
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_count: Mutex<u32>,
    last_failure_time: Mutex<Option<Instant>>,
    config: RecoveryConfig,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Recovery configuration with circuit breaker settings
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::ocr_config::RecoveryConfig;
    /// use just_ingredients::circuit_breaker::CircuitBreaker;
    ///
    /// let config = RecoveryConfig::default();
    /// let circuit_breaker = CircuitBreaker::new(config);
    /// ```
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            failure_count: Mutex::new(0),
            last_failure_time: Mutex::new(None),
            config,
        }
    }

    /// Check if circuit breaker is open (blocking requests)
    ///
    /// Implements the core circuit breaker state machine logic with automatic reset handling.
    ///
    /// ## Algorithm Flow
    ///
    /// ```text
    /// 1. Check failure count against threshold
    /// 2. If threshold exceeded:
    ///    a. Check time since last failure
    ///    b. If reset timeout not elapsed: OPEN (block requests)
    ///    c. If reset timeout elapsed: Reset counters, CLOSED (allow testing)
    /// 3. If threshold not exceeded: CLOSED (normal operation)
    /// ```
    ///
    /// ## State Determination Logic
    ///
    /// The method atomically checks the current failure state and determines if requests
    /// should be blocked. It automatically resets the circuit breaker after the configured
    /// timeout period to allow testing of service recovery.
    ///
    /// ## Automatic Reset Behavior
    ///
    /// - **No Manual Intervention**: Circuit automatically resets after timeout
    /// - **Single Test Request**: First request after reset tests service health
    /// - **State Preservation**: Reset only occurs when timeout expires
    ///
    /// ## Thread Safety Considerations
    ///
    /// - Uses `Mutex` for atomic state access
    /// - Read operations don't block writes
    /// - State transitions are atomic and consistent
    ///
    /// # Returns
    ///
    /// `true` if circuit is open and should block requests, `false` if closed
    ///
    /// # Behavior
    ///
    /// - Returns `true` when failure count >= threshold and reset time hasn't elapsed
    /// - Automatically resets to closed state after reset timeout
    /// - Thread-safe using internal mutexes
    pub fn is_open(&self) -> bool {
        let failure_count = *self
            .failure_count
            .lock()
            .expect("Failed to acquire failure count lock");
        let last_failure = *self
            .last_failure_time
            .lock()
            .expect("Failed to acquire last failure time lock");

        if failure_count >= self.config.circuit_breaker_threshold {
            if let Some(last_time) = last_failure {
                let elapsed = last_time.elapsed();
                if elapsed < Duration::from_secs(self.config.circuit_breaker_reset_secs) {
                    return true; // Circuit is still open
                }
                // Reset circuit breaker
                *self
                    .failure_count
                    .lock()
                    .expect("Failed to acquire failure count lock") = 0;
                *self
                    .last_failure_time
                    .lock()
                    .expect("Failed to acquire last failure time lock") = None;
            }
        }
        false
    }

    /// Record a failure to increment the failure counter
    ///
    /// Should be called whenever an OCR operation fails.
    /// Updates failure count and last failure timestamp.
    ///
    /// # Thread Safety
    ///
    /// Uses internal mutex for thread-safe updates.
    pub fn record_failure(&self) {
        *self
            .failure_count
            .lock()
            .expect("Failed to acquire failure count lock") += 1;
        *self
            .last_failure_time
            .lock()
            .expect("Failed to acquire last failure time lock") = Some(Instant::now());
    }

    /// Record a success to reset the failure counter
    ///
    /// Should be called whenever an OCR operation succeeds.
    /// Resets failure count and clears last failure timestamp.
    ///
    /// # Thread Safety
    ///
    /// Uses internal mutex for thread-safe updates.
    pub fn record_success(&self) {
        *self
            .failure_count
            .lock()
            .expect("Failed to acquire failure count lock") = 0;
        *self
            .last_failure_time
            .lock()
            .expect("Failed to acquire last failure time lock") = None;
    }
}
