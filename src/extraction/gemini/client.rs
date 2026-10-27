//! Gemini client: one `generateContent` call per image, with retries on temporary failures
//! and a circuit breaker that stops calling the service while it is down.

use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde_json::value::RawValue;
use tracing::{info, warn};

use super::types::{
    error_from_status, GenerateContentRequest, GenerateContentResponse, UsageMetadata,
};
use crate::extraction::breaker::CircuitBreaker;
use crate::extraction::config::is_valid_model_id;
use crate::extraction::pricing::estimated_cost_micro_usd;
use crate::extraction::prompt::{response_schema, EXTRACTION_PROMPT};
use crate::extraction::{
    Extraction, ExtractionError, ExtractionFuture, GeminiConfig, ImageInput, IngredientExtractor,
    ThinkingLevel,
};
use crate::observability::{record_llm_request, record_llm_usage};
use crate::retry::{retry_with_backoff, Backoff, RetryDecision};
use crate::secret::SecretString;

/// Production endpoint of the Gemini API.
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Retries for one image: 1 s doubling up to 8 s, none starting later than 20 s after the
/// first attempt, so a user waits at most about one timeout plus 20 s.
pub const DEFAULT_BACKOFF: Backoff = Backoff {
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(8),
    max_elapsed: Duration::from_secs(20),
};

/// Consecutive failed images before the breaker opens, and how long it stays open.
pub const BREAKER_FAILURE_THRESHOLD: u32 = 5;
pub const BREAKER_OPEN_DURATION: Duration = Duration::from_secs(60);

/// Connection setup timeout, within the total timeout of `GeminiConfig::timeout_secs`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Extracts ingredients with the Gemini API.
#[derive(Debug)]
pub struct GeminiExtractor {
    client: reqwest::Client,
    api_key: SecretString,
    model: String,
    thinking_level: ThinkingLevel,
    base_url: String,
    schema: Box<RawValue>,
    backoff: Backoff,
    breaker: CircuitBreaker,
}

/// Result of one successful API call.
struct CallOutput {
    extraction: Extraction,
    usage: UsageMetadata,
    model_version: Option<String>,
}

impl GeminiExtractor {
    /// Creates a client for the production endpoint.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::Config`] when the key is missing, the model id is invalid, or the
    /// HTTP client cannot be built.
    pub fn new(config: &GeminiConfig) -> Result<Self, ExtractionError> {
        if config.api_key.is_empty() {
            return Err(ExtractionError::Config(
                "GEMINI_API_KEY is not set".to_string(),
            ));
        }
        if !is_valid_model_id(&config.model) {
            return Err(ExtractionError::Config(format!(
                "invalid GEMINI_MODEL '{}'",
                config.model
            )));
        }
        let timeout = Duration::from_secs(config.timeout_secs);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT.min(timeout))
            .build()
            .map_err(|error| {
                ExtractionError::Config(format!("HTTP client: {}", error.without_url()))
            })?;
        let schema = response_schema().map_err(|error| {
            ExtractionError::Config(format!("invalid response schema: {error}"))
        })?;

        Ok(Self {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            thinking_level: config.thinking_level,
            base_url: DEFAULT_BASE_URL.to_string(),
            schema,
            backoff: DEFAULT_BACKOFF,
            breaker: CircuitBreaker::new(BREAKER_FAILURE_THRESHOLD, BREAKER_OPEN_DURATION),
        })
    }

    /// Sends requests to `base_url` instead of the production endpoint (tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Replaces the retry schedule.
    pub fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Replaces the circuit breaker.
    pub fn with_breaker(mut self, breaker: CircuitBreaker) -> Self {
        self.breaker = breaker;
        self
    }

    async fn extract_image(&self, image: &ImageInput) -> Result<Extraction, ExtractionError> {
        let permit = self.breaker.try_acquire()?;
        let started = Instant::now();
        let mut attempts = 0u32;

        let result = retry_with_backoff(
            self.backoff,
            |error: &ExtractionError| match error {
                ExtractionError::RateLimited {
                    retry_after: Some(delay),
                } => RetryDecision::RetryAfter(*delay),
                error if error.is_transient() => RetryDecision::Retry,
                _ => RetryDecision::Stop,
            },
            |retry, delay, error| {
                warn!(
                    model = %self.model,
                    retry,
                    retry_in_ms = delay.as_millis(),
                    error = %error,
                    "Gemini call failed, retrying"
                );
            },
            || {
                attempts += 1;
                self.call_once(image)
            },
        )
        .await;

        // Only outages count against the service: a refused or invalid answer means it is up.
        match &result {
            Err(error) if error.is_transient() => permit.failure(),
            _ => permit.success(),
        }

        let elapsed = started.elapsed();
        let elapsed_ms = elapsed.as_millis();
        let retries = attempts.saturating_sub(1);
        match result {
            Ok(output) => {
                record_llm_request(&self.model, "success", elapsed, retries);
                record_llm_usage(
                    &self.model,
                    output.usage.prompt_token_count,
                    output.usage.output_tokens(),
                    estimated_cost_micro_usd(&self.model, &output.usage),
                );
                info!(
                    model = %self.model,
                    model_version = output.model_version.as_deref().unwrap_or("unknown"),
                    attempts,
                    elapsed_ms,
                    input_tokens = output.usage.prompt_token_count,
                    output_tokens = output.usage.output_tokens(),
                    ingredients = output.extraction.ingredients.len(),
                    "Gemini extraction succeeded"
                );
                Ok(output.extraction)
            }
            Err(error) => {
                record_llm_request(&self.model, error.kind(), elapsed, retries);
                warn!(model = %self.model, attempts, elapsed_ms, error = %error, "Gemini extraction failed");
                Err(error)
            }
        }
    }

    async fn call_once(&self, image: &ImageInput) -> Result<CallOutput, ExtractionError> {
        let request = GenerateContentRequest::new(
            image,
            EXTRACTION_PROMPT,
            &self.schema,
            self.thinking_level,
        );
        let url = format!(
            "{}/models/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            self.model
        );
        let response = self
            .client
            .post(url)
            // The key travels in a header, never in the URL, so it cannot end up in errors.
            .header("x-goog-api-key", self.api_key.expose())
            .json(&request)
            .send()
            .await
            .map_err(request_error)?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = retry_after(response.headers());
            let body = response.text().await.unwrap_or_default();
            return Err(error_from_status(status.as_u16(), &body, retry_after));
        }

        let body: GenerateContentResponse = response.json().await.map_err(|error| {
            if error.is_timeout() {
                ExtractionError::Timeout
            } else {
                ExtractionError::InvalidResponse(format!(
                    "unreadable response body: {}",
                    error.without_url()
                ))
            }
        })?;
        let usage = body.usage_metadata.unwrap_or_default();
        let model_version = body.model_version.clone();
        Ok(CallOutput {
            extraction: body.into_extraction()?,
            usage,
            model_version,
        })
    }
}

impl IngredientExtractor for GeminiExtractor {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn extract<'a>(&'a self, image: &'a ImageInput) -> ExtractionFuture<'a> {
        Box::pin(self.extract_image(image))
    }
}

fn request_error(error: reqwest::Error) -> ExtractionError {
    if error.is_timeout() {
        ExtractionError::Timeout
    } else {
        ExtractionError::Network(error.without_url().to_string())
    }
}

/// `Retry-After` in seconds; the HTTP-date form is not used by the Gemini API.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SUCCESS: &str = include_str!("../../../tests/fixtures/gemini/success.json");
    const NOT_FOUND: &str =
        include_str!("../../../tests/fixtures/gemini/error_404_model_not_found.json");
    const MODEL: &str = "gemini-test";
    const KEY: &str = "test-key-123";
    const ENDPOINT: &str = "/models/gemini-test:generateContent";

    const FAST: Backoff = Backoff {
        initial_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(50),
        max_elapsed: Duration::from_secs(2),
    };

    fn extractor(
        server: &MockServer,
        timeout_secs: u64,
    ) -> Result<GeminiExtractor, ExtractionError> {
        let config = GeminiConfig {
            api_key: SecretString::new(KEY),
            model: MODEL.to_string(),
            timeout_secs,
            ..GeminiConfig::default()
        };
        Ok(GeminiExtractor::new(&config)?
            .with_base_url(server.uri())
            .with_backoff(FAST))
    }

    fn image() -> Result<ImageInput, ExtractionError> {
        ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00])
    }

    async fn requests(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .map(|r| r.len())
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn sends_the_key_in_a_header_and_returns_the_extraction() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(ENDPOINT))
            .and(header("x-goog-api-key", KEY))
            .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS))
            .expect(1)
            .mount(&server)
            .await;

        let extraction = extractor(&server, 5)?.extract(&image()?).await?;
        assert_eq!(extraction.ingredients.len(), 7);
        let request = server.received_requests().await.unwrap_or_default();
        assert!(!request[0].url.as_str().contains(KEY), "key not in the URL");
        Ok(())
    }

    #[tokio::test]
    async fn server_errors_are_retried() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_body_string("{}"))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS))
            .with_priority(2)
            .mount(&server)
            .await;

        let extraction = extractor(&server, 5)?.extract(&image()?).await?;
        assert!(extraction.is_recipe);
        assert_eq!(requests(&server).await, 3);
        Ok(())
    }

    #[tokio::test]
    async fn rate_limits_honor_retry_after() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS))
            .with_priority(2)
            .mount(&server)
            .await;

        extractor(&server, 5)?.extract(&image()?).await?;
        assert_eq!(requests(&server).await, 2);
        Ok(())
    }

    #[tokio::test]
    async fn client_errors_are_not_retried() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(404).set_body_string(NOT_FOUND))
            .mount(&server)
            .await;

        let error = extractor(&server, 5)?.extract(&image()?).await;
        assert!(
            matches!(error, Err(ExtractionError::Api { status: 404, .. })),
            "{error:?}"
        );
        assert_eq!(requests(&server).await, 1);
        Ok(())
    }

    #[tokio::test]
    async fn slow_responses_time_out() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(SUCCESS)
                    .set_delay(Duration::from_secs(3)),
            )
            .mount(&server)
            .await;

        let extractor = extractor(&server, 1)?.with_backoff(Backoff {
            max_elapsed: Duration::from_millis(500),
            ..FAST
        });
        let error = extractor.extract(&image()?).await;
        assert!(matches!(error, Err(ExtractionError::Timeout)), "{error:?}");
        Ok(())
    }

    #[tokio::test]
    async fn breaker_stops_calls_after_repeated_outages() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_body_string("{}"))
            .mount(&server)
            .await;

        let extractor = extractor(&server, 5)?
            .with_backoff(Backoff {
                max_elapsed: Duration::ZERO,
                ..FAST
            })
            .with_breaker(CircuitBreaker::new(2, Duration::from_secs(60)));
        let image = image()?;
        for _ in 0..2 {
            assert!(matches!(
                extractor.extract(&image).await,
                Err(ExtractionError::Api { status: 503, .. })
            ));
        }
        assert!(matches!(
            extractor.extract(&image).await,
            Err(ExtractionError::CircuitOpen)
        ));
        assert_eq!(requests(&server).await, 2, "no call while open");
        Ok(())
    }

    #[test]
    fn missing_key_and_bad_model_are_rejected() {
        let no_key = GeminiConfig::default();
        assert!(matches!(
            GeminiExtractor::new(&no_key),
            Err(ExtractionError::Config(_))
        ));
        let bad_model = GeminiConfig {
            api_key: SecretString::new(KEY),
            model: "../models/x".to_string(),
            ..GeminiConfig::default()
        };
        assert!(matches!(
            GeminiExtractor::new(&bad_model),
            Err(ExtractionError::Config(_))
        ));
    }

    #[test]
    fn debug_output_does_not_contain_the_key() -> Result<(), ExtractionError> {
        let config = GeminiConfig {
            api_key: SecretString::new(KEY),
            ..GeminiConfig::default()
        };
        let printed = format!("{:?}", GeminiExtractor::new(&config)?);
        assert!(!printed.contains(KEY));
        Ok(())
    }
}
