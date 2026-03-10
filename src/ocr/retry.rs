//! # OCR Retry Module
//!
//! Retry logic and utilities for OCR processing, including exponential backoff
//! with jitter to prevent thundering herd problems.

/// Calculate retry delay with exponential backoff
///
/// Implements exponential backoff with jitter to prevent thundering herd problems.
/// Delay increases exponentially with each retry attempt, with random jitter added
/// to distribute retry attempts over time.
///
/// ## Algorithm Overview
///
/// The function calculates retry delays using this formula:
/// ```text
/// delay = min(base_delay * (2^(attempt-1)), max_delay)
/// jitter = random(0, delay/4)
/// final_delay = delay + jitter
/// ```
///
/// ## Exponential Backoff Logic
///
/// - **Base Delay**: Starting delay for first retry (typically 1000ms)
/// - **Exponential Growth**: Delay doubles with each retry attempt
/// - **Maximum Cap**: Prevents excessively long delays
/// - **Jitter Addition**: Random component prevents synchronized retries
///
/// ## Delay Progression Examples
///
/// | Attempt | Base Delay | Exponential | Jitter Range | Final Delay Range |
/// |---------|------------|-------------|--------------|-------------------|
/// | 1       | 1000ms     | 1000ms      | 0-250ms      | 1000-1250ms      |
/// | 2       | 1000ms     | 2000ms      | 0-500ms      | 2000-2500ms      |
/// | 3       | 1000ms     | 4000ms      | 0-1000ms     | 4000-5000ms      |
/// | 4       | 1000ms     | 8000ms      | 0-2000ms     | 8000-10000ms     |
/// | 5+      | 1000ms     | 10000ms*    | 0-2500ms     | 10000-12500ms    |
///
/// *Capped at max_retry_delay_ms
///
/// ## Jitter Implementation
///
/// Jitter is calculated as a random value between 0 and delay/4:
/// - **Purpose**: Distribute retry attempts over time
/// - **Range**: 0 to 25% of the base delay
/// - **Randomness**: Uses `rand::random()` for uniform distribution
/// - **Thread Safety**: Each call generates independent random values
///
/// ## Configuration Parameters
///
/// - `base_retry_delay_ms`: Base delay for first retry (default: 1000ms)
/// - `max_retry_delay_ms`: Maximum delay cap (default: 10000ms)
/// - `attempt`: Current retry attempt number (1-based)
///
/// ## Benefits
///
/// - **Load Distribution**: Prevents server overload during failures
/// - **Thundering Herd Prevention**: Jitter distributes retry attempts
/// - **Configurable**: Adjustable for different environments
/// - **Predictable**: Exponential growth with known bounds
/// - **Resource Protection**: Gives failing services time to recover
///
/// ## Usage in Retry Logic
///
/// ```rust
/// use just_ingredients::ocr_config::RecoveryConfig;
/// use just_ingredients::ocr::calculate_retry_delay;
///
/// let config = RecoveryConfig::default();
/// // Simulate retry logic
/// let mut results = Vec::new();
/// for attempt in 1..=3 {
///     let delay = calculate_retry_delay(attempt, &config);
///     results.push(delay);
///     // In real code: tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
/// }
/// // Results will be different due to jitter, but within expected ranges
/// assert!(results[0] >= 1000 && results[0] <= 1250); // ~1000ms + jitter
/// assert!(results[1] >= 2000 && results[1] <= 2500); // ~2000ms + jitter
/// assert!(results[2] >= 4000 && results[2] <= 5000); // ~4000ms + jitter
/// ```
///
/// ## Performance Considerations
///
/// - **Computation**: Minimal CPU overhead (bit operations + random generation)
/// - **Memory**: No allocations, uses only primitive types
/// - **Thread Safety**: Safe for concurrent use
/// - **Predictability**: Deterministic exponential growth with random jitter
///
/// # Arguments
///
/// * `attempt` - Current retry attempt number (1-based, first retry = 1)
/// * `recovery` - Recovery configuration with delay settings
///
/// # Returns
///
/// Returns delay in milliseconds before next retry attempt
///
/// # Examples
///
/// ```rust
/// use just_ingredients::ocr::{calculate_retry_delay, RecoveryConfig};
///
/// let config = RecoveryConfig::default();
/// // First retry: ~1000-1250ms (1000ms + jitter)
/// let delay1 = calculate_retry_delay(1, &config);
/// // Second retry: ~2000-2500ms (2000ms + jitter)
/// let delay2 = calculate_retry_delay(2, &config);
/// // Third retry: ~4000-5000ms (4000ms + jitter)
/// let delay3 = calculate_retry_delay(3, &config);
/// ```
/// - **Capped**: Prevents excessively long delays
pub fn calculate_retry_delay(attempt: u32, recovery: &crate::ocr_config::RecoveryConfig) -> u64 {
    // Calculate exponential backoff with minimal precision loss
    // For retry delays, precision loss is acceptable as delays are typically small
    #[allow(clippy::cast_precision_loss)]
    let base_delay = recovery.base_retry_delay_ms as f64;

    #[allow(clippy::cast_precision_loss)]
    let exponential_delay = base_delay * (2.0_f64).powf((attempt - 1) as f64);

    #[allow(clippy::cast_precision_loss)]
    let delay = exponential_delay.min(recovery.max_retry_delay_ms as f64) as u64;

    // Add some jitter to prevent thundering herd
    let jitter = (rand::random::<u64>() % (delay / 4)) as u64;
    delay + jitter
}
