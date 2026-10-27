//! Estimated cost of extraction calls, for the `llm_estimated_cost_micro_usd_total` metric.
//!
//! Prices are USD per million tokens on the paid standard tier, read from
//! ai.google.dev/gemini-api/docs/pricing on 2026-10-26 (same table as
//! `scripts/llm_benchmark`). Thinking tokens are billed as output. Check the page before
//! relying on the numbers; an unknown model records tokens but no cost.

use crate::extraction::gemini::types::UsageMetadata;

/// Price of one model, USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

const PRICES: &[(&str, ModelPrice)] = &[
    (
        "gemini-3.8-flash",
        ModelPrice {
            input_per_million: 0.75,
            output_per_million: 3.75,
        },
    ),
    (
        "gemini-3.6-flash",
        ModelPrice {
            input_per_million: 0.75,
            output_per_million: 3.75,
        },
    ),
    (
        "gemini-3.5-flash-lite",
        ModelPrice {
            input_per_million: 0.30,
            output_per_million: 2.50,
        },
    ),
    (
        "gemini-3.1-flash-lite",
        ModelPrice {
            input_per_million: 0.25,
            output_per_million: 1.50,
        },
    ),
];

/// Price of `model`. Exact match only: a prefix match would price a future
/// `gemini-3.6-flash-lite` as `gemini-3.6-flash`. Unknown models record tokens, not cost.
pub fn price_for(model: &str) -> Option<ModelPrice> {
    PRICES
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, price)| *price)
}

/// Estimated cost of one call in micro-USD (1e-6 USD), rounded to the nearest unit.
pub fn estimated_cost_micro_usd(model: &str, usage: &UsageMetadata) -> Option<u64> {
    let price = price_for(model)?;
    // Price per million tokens equals micro-USD per token.
    let micro_usd = usage.prompt_token_count as f64 * price.input_per_million
        + usage.output_tokens() as f64 * price.output_per_million;
    Some(micro_usd.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_model_ids_are_priced() {
        assert!(price_for("gemini-3.5-flash-lite").is_some());
        assert_eq!(
            price_for("gemini-3.6-flash-lite"),
            None,
            "not priced as gemini-3.6-flash"
        );
        assert_eq!(price_for("gemini-3.5-flash-lite-preview"), None);
        assert_eq!(price_for("unknown-model"), None);
    }

    #[test]
    fn cost_matches_the_benchmark_measurement() {
        // Usage of the recorded success fixture: 1561 input, 492 output tokens.
        let usage = UsageMetadata {
            prompt_token_count: 1561,
            candidates_token_count: 492,
            thoughts_token_count: 0,
        };
        // 1561 * 0.30 + 492 * 2.50 = 468.3 + 1230 = 1698.3 micro-USD = 0.0017 USD per photo.
        assert_eq!(
            estimated_cost_micro_usd("gemini-3.5-flash-lite", &usage),
            Some(1698)
        );
        assert_eq!(estimated_cost_micro_usd("unknown-model", &usage), None);
    }

    #[test]
    fn thinking_tokens_are_billed_as_output() {
        let usage = UsageMetadata {
            prompt_token_count: 0,
            candidates_token_count: 100,
            thoughts_token_count: 100,
        };
        assert_eq!(
            estimated_cost_micro_usd("gemini-3.5-flash-lite", &usage),
            Some(500)
        );
    }
}
