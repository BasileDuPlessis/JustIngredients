//! The bot's metrics (plan O2, the catalogue of `specs/tasks/observability.md`), served on the
//! internal port and collected by Fly. All are named `ji_…`; none carries a user or recipe
//! content as a label (privacy, and Fly drops metrics with too many label values).

use anyhow::Result;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Installs the process-wide recorder; its handle renders the metrics for `/metrics`.
pub fn install_recorder() -> Result<PrometheusHandle> {
    Ok(PrometheusBuilder::new().install_recorder()?)
}

// --- Usage ---

/// What happened to a photo sent to the bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoResult {
    /// Its ingredients were read.
    Read,
    /// Read, but no recipe or ingredient list on it.
    NotRecipe,
    /// Not readable: not an image, too small, blurred.
    Unreadable,
    /// Over the size limit.
    TooLarge,
    /// The user's daily number of photos was reached.
    DailyLimit,
    /// Gemini failed (timeout, quota, outage, invalid answer).
    GeminiError,
}

impl PhotoResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::NotRecipe => "not_recipe",
            Self::Unreadable => "unreadable",
            Self::TooLarge => "too_large",
            Self::DailyLimit => "daily_limit",
            Self::GeminiError => "gemini_error",
        }
    }
}

/// A photo handled: `ji_photos_total{result}`.
pub fn photo(result: PhotoResult) {
    metrics::counter!("ji_photos_total", "result" => result.as_str()).increment(1);
    super::usage::add(|day| match result {
        PhotoResult::Read => day.photos_read += 1,
        _ => day.photos_failed += 1,
    });
}

/// Recipes saved from one photo: `ji_recipes_created_total{kind}`, `photo` for the recipe and
/// `linked` for each other part of a photo in parts.
pub fn recipes_created(parts: usize) {
    if parts == 0 {
        return;
    }
    super::usage::add(|day| day.recipes_created += i64::try_from(parts).unwrap_or(0));
    metrics::counter!("ji_recipes_created_total", "kind" => "photo").increment(1);
    let linked = u64::try_from(parts - 1).unwrap_or(0);
    if linked > 0 {
        metrics::counter!("ji_recipes_created_total", "kind" => "linked").increment(linked);
    }
}

/// Something a user did with the bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Help,
    List,
    Search,
    CardOpened,
    LinkedOpened,
    Rename,
    Edit,
    Delete,
    ReviewConfirmed,
    IngredientEdited,
    IngredientDeleted,
    UnitViewChosen,
}

impl Action {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Help => "help",
            Self::List => "list",
            Self::Search => "search",
            Self::CardOpened => "card_opened",
            Self::LinkedOpened => "linked_opened",
            Self::Rename => "rename",
            Self::Edit => "edit",
            Self::Delete => "delete",
            Self::ReviewConfirmed => "review_confirmed",
            Self::IngredientEdited => "ingredient_edited",
            Self::IngredientDeleted => "ingredient_deleted",
            Self::UnitViewChosen => "unit_view_chosen",
        }
    }
}

/// An action: `ji_actions_total{action}`.
pub fn action(action: Action) {
    metrics::counter!("ji_actions_total", "action" => action.as_str()).increment(1);
}

/// Users seen in the last 24 hours and 7 days, and all users: `ji_users{period}` (computed from
/// the database every hour).
pub fn set_users(last_day: u64, last_week: u64, total: u64) {
    metrics::gauge!("ji_users", "period" => "day").set(last_day as f64);
    metrics::gauge!("ji_users", "period" => "week").set(last_week as f64);
    metrics::gauge!("ji_users", "period" => "total").set(total as f64);
}

/// Recipes saved, all users together: `ji_recipes`.
pub fn set_recipes(total: u64) {
    metrics::gauge!("ji_recipes").set(total as f64);
}

// --- Gemini and its cost ---

/// One photo sent to Gemini: `ji_gemini_requests_total{model, result}` (`success` or an
/// `ExtractionError::kind`; `circuit_open` when the breaker refused it without a call), its
/// duration with retries, and the retries.
pub fn gemini_request(model: &str, result: &'static str, duration: Duration, retries: u32) {
    if result != "circuit_open" {
        super::usage::add(|day| {
            day.gemini_calls += 1;
            if result != "success" {
                day.gemini_failures += 1;
            }
        });
    }
    metrics::counter!("ji_gemini_requests_total", "model" => model.to_string(), "result" => result)
        .increment(1);
    metrics::histogram!("ji_gemini_duration_seconds", "model" => model.to_string())
        .record(duration.as_secs_f64());
    if retries > 0 {
        metrics::counter!("ji_gemini_retries_total", "model" => model.to_string())
            .increment(u64::from(retries));
    }
}

/// The tokens billed for one call and, when the model's price is known, its estimated cost in
/// micro-USD (`ji_gemini_cost_micro_usd_total / 1e6` is in USD).
pub fn gemini_usage(model: &str, input_tokens: u64, output_tokens: u64, cost: Option<u64>) {
    metrics::counter!("ji_gemini_tokens_total", "model" => model.to_string(), "kind" => "input")
        .increment(input_tokens);
    metrics::counter!("ji_gemini_tokens_total", "model" => model.to_string(), "kind" => "output")
        .increment(output_tokens);
    if let Some(cost) = cost {
        metrics::counter!("ji_gemini_cost_micro_usd_total", "model" => model.to_string())
            .increment(cost);
        super::usage::add(|day| day.gemini_cost_micro_usd += i64::try_from(cost).unwrap_or(0));
    }
}

/// A photo already read, answered from the cache: no paid call.
pub fn gemini_cache_hit() {
    metrics::counter!("ji_gemini_cache_hits_total").increment(1);
}

/// Whether the circuit breaker in front of Gemini is open (too many failures in a row).
pub fn set_gemini_breaker_open(open: bool) {
    metrics::gauge!("ji_gemini_breaker_open").set(if open { 1.0 } else { 0.0 });
}

// --- Health ---

/// An error: `ji_errors_total{component, kind}` (`gemini`, `telegram`, `database`, `handler`,
/// `internal`).
pub fn error(component: &'static str, kind: &'static str) {
    metrics::counter!("ji_errors_total", "component" => component, "kind" => kind).increment(1);
}

/// The time of this start, in seconds since 1970: restarts show as steps.
pub fn started() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or_default();
    metrics::gauge!("ji_started_at_seconds").set(now);
}

// --- Speed ---

/// A Telegram update handled (`message` or `callback`): `ji_update_duration_seconds{kind}`.
pub fn update_handled(kind: &'static str, duration: Duration) {
    metrics::histogram!("ji_update_duration_seconds", "kind" => kind)
        .record(duration.as_secs_f64());
}

/// A recipe card shown, from the button press to the card on screen.
pub fn card_shown(duration: Duration) {
    metrics::histogram!("ji_card_shown_seconds").record(duration.as_secs_f64());
}

/// A database operation: `ji_db_duration_seconds{operation}`.
pub fn db_query(operation: &str, duration: Duration) {
    metrics::histogram!("ji_db_duration_seconds", "operation" => operation.to_string())
        .record(duration.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};

    /// Metric name, labels and value from the debugging recorder.
    type Sample = (String, Vec<(String, String)>, DebugValue);

    fn record(f: impl FnOnce()) -> Vec<Sample> {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, f);
        snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(key, _, _, value)| {
                let labels = key
                    .key()
                    .labels()
                    .map(|label| (label.key().to_string(), label.value().to_string()))
                    .collect();
                (key.key().name().to_string(), labels, value)
            })
            .collect()
    }

    fn counter(samples: &[Sample], name: &str, label: (&str, &str)) -> Option<u64> {
        samples.iter().find_map(|(n, labels, value)| match value {
            DebugValue::Counter(count)
                if n == name && labels.iter().any(|(k, v)| k == label.0 && v == label.1) =>
            {
                Some(*count)
            }
            _ => None,
        })
    }

    #[test]
    fn gemini_calls_and_their_cost_are_counted() {
        let samples = record(|| {
            gemini_request("gemini-test", "success", Duration::from_millis(1500), 2);
            gemini_usage("gemini-test", 1561, 492, Some(1698));
        });
        assert_eq!(
            counter(&samples, "ji_gemini_requests_total", ("result", "success")),
            Some(1)
        );
        assert_eq!(
            counter(
                &samples,
                "ji_gemini_retries_total",
                ("model", "gemini-test")
            ),
            Some(2)
        );
        assert_eq!(
            counter(&samples, "ji_gemini_tokens_total", ("kind", "input")),
            Some(1561)
        );
        assert_eq!(
            counter(&samples, "ji_gemini_tokens_total", ("kind", "output")),
            Some(492)
        );
        assert_eq!(
            counter(
                &samples,
                "ji_gemini_cost_micro_usd_total",
                ("model", "gemini-test")
            ),
            Some(1698)
        );
    }

    #[test]
    fn a_photo_in_parts_counts_its_linked_recipes() {
        let samples = record(|| {
            recipes_created(3);
            recipes_created(1);
            photo(PhotoResult::Read);
            photo(PhotoResult::DailyLimit);
            action(Action::CardOpened);
        });
        assert_eq!(
            counter(&samples, "ji_recipes_created_total", ("kind", "photo")),
            Some(2)
        );
        assert_eq!(
            counter(&samples, "ji_recipes_created_total", ("kind", "linked")),
            Some(2)
        );
        assert_eq!(
            counter(&samples, "ji_photos_total", ("result", "read")),
            Some(1)
        );
        assert_eq!(
            counter(&samples, "ji_photos_total", ("result", "daily_limit")),
            Some(1)
        );
        assert_eq!(
            counter(&samples, "ji_actions_total", ("action", "card_opened")),
            Some(1)
        );
    }

    #[test]
    fn every_metric_is_named_ji() {
        let samples = record(|| {
            photo(PhotoResult::Read);
            recipes_created(2);
            action(Action::Search);
            set_users(1, 2, 3);
            set_recipes(4);
            gemini_request("m", "timeout", Duration::from_secs(1), 1);
            gemini_usage("m", 1, 1, Some(1));
            gemini_cache_hit();
            set_gemini_breaker_open(true);
            error("gemini", "timeout");
            started();
            update_handled("message", Duration::from_millis(5));
            card_shown(Duration::from_millis(5));
            db_query("save_recipe", Duration::from_millis(5));
        });
        assert!(samples.len() >= 14, "{}", samples.len());
        for (name, _, _) in &samples {
            assert!(name.starts_with("ji_"), "{name}");
        }
    }
}
