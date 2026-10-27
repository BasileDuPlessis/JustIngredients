//! Gemini client: one `generateContent` call per image, with retries on temporary failures,
//! one deadline per image, and a circuit breaker that stops calling the service while it is
//! down.

use std::time::{Duration, Instant};

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, RETRY_AFTER};
use serde_json::value::RawValue;
use tracing::{info, warn, Instrument};

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

/// Production endpoint of the Gemini API.
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Retry schedule: 1 s doubling up to 8 s, shortened by up to 20% at random. Retries stop at
/// the image deadline (`GeminiConfig::timeout_secs`), which replaces `max_elapsed`.
pub const DEFAULT_BACKOFF: Backoff = Backoff {
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(8),
    max_elapsed: Duration::from_secs(40),
    jitter: 0.2,
};

/// Longest single attempt; a normal call takes about 2 s. Keeps room for a retry within the
/// image deadline.
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(20);

/// Consecutive failed images before the breaker opens, and how long it stays open.
pub const BREAKER_FAILURE_THRESHOLD: u32 = 5;
pub const BREAKER_OPEN_DURATION: Duration = Duration::from_secs(60);

/// Connection setup timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Largest response body read: an answer is a few KB, an error body less.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Extracts ingredients with the Gemini API.
#[derive(Debug)]
pub struct GeminiExtractor {
    client: reqwest::Client,
    /// `x-goog-api-key` value, marked sensitive so it is never printed.
    api_key: HeaderValue,
    model: String,
    thinking_level: ThinkingLevel,
    base_url: String,
    schema: Box<RawValue>,
    backoff: Backoff,
    /// Time allowed for one image, retries included.
    deadline: Duration,
    breaker: CircuitBreaker,
}

/// Result of one successful API call.
struct CallOutput {
    extraction: Extraction,
    usage: UsageMetadata,
    model_version: Option<String>,
    finish_reason: Option<String>,
}

impl GeminiExtractor {
    /// Creates a client for the production endpoint.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::Config`] when the key is missing or not a valid header value, the
    /// model id is invalid, or the HTTP client cannot be built.
    pub fn new(config: &GeminiConfig) -> Result<Self, ExtractionError> {
        if config.api_key.is_empty() {
            return Err(ExtractionError::Config(
                "GEMINI_API_KEY is not set".to_string(),
            ));
        }
        let mut api_key = HeaderValue::from_str(config.api_key.expose()).map_err(|_| {
            ExtractionError::Config("GEMINI_API_KEY contains invalid characters".to_string())
        })?;
        api_key.set_sensitive(true);
        if !is_valid_model_id(&config.model) {
            return Err(ExtractionError::Config(format!(
                "invalid GEMINI_MODEL '{}'",
                config.model
            )));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            // The key header would follow a redirect to another host: never follow.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                ExtractionError::Config(format!("HTTP client: {}", describe(&error)))
            })?;
        let schema = response_schema().map_err(|error| {
            ExtractionError::Config(format!("invalid response schema: {error}"))
        })?;

        Ok(Self {
            client,
            api_key,
            model: config.model.clone(),
            thinking_level: config.thinking_level,
            base_url: DEFAULT_BASE_URL.to_string(),
            schema,
            backoff: DEFAULT_BACKOFF,
            deadline: Duration::from_secs(config.timeout_secs),
            breaker: CircuitBreaker::new(BREAKER_FAILURE_THRESHOLD, BREAKER_OPEN_DURATION),
        })
    }

    /// Sends requests to `base_url` instead of the production endpoint (tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Replaces the retry schedule (its `max_elapsed` is replaced by the image deadline).
    pub fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Replaces the time allowed for one image.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Replaces the circuit breaker.
    pub fn with_breaker(mut self, breaker: CircuitBreaker) -> Self {
        self.breaker = breaker;
        self
    }

    /// Checks the key and the model with `GET /models/{model}`, which is free: an invalid key or a
    /// retired model shows at startup instead of failing every photo.
    ///
    /// # Errors
    ///
    /// The API error ([`ExtractionError::Api`] with the status) or a transport error.
    pub async fn verify_access(&self) -> Result<(), ExtractionError> {
        let url = format!(
            "{}/models/{}",
            self.base_url.trim_end_matches('/'),
            self.model
        );
        let response = self
            .client
            .get(url)
            .timeout(CONNECT_TIMEOUT)
            .header("x-goog-api-key", self.api_key.clone())
            .send()
            .await
            .map_err(|error| request_error(&error))?;
        let status = response.status();
        let retry_after = retry_after(response.headers());
        let body = read_limited(response).await?;
        if status.is_success() {
            Ok(())
        } else {
            Err(error_from_status(
                status.as_u16(),
                &String::from_utf8_lossy(&body),
                retry_after,
            ))
        }
    }

    async fn extract_image(&self, image: &ImageInput) -> Result<Extraction, ExtractionError> {
        let started = Instant::now();
        let permit = match self.breaker.try_acquire() {
            Ok(permit) => permit,
            Err(error) => {
                record_llm_request(&self.model, error.kind(), started.elapsed(), 0);
                warn!(model = %self.model, "Gemini circuit open: image rejected without calling the API");
                return Err(error);
            }
        };

        // Built once per image and shared by the attempts: the base64 image is the bulk of it.
        let request = GenerateContentRequest::new(
            image,
            EXTRACTION_PROMPT,
            &self.schema,
            self.thinking_level,
        );
        let body = Bytes::from(serde_json::to_vec(&request).map_err(|error| {
            ExtractionError::InvalidResponse(format!("cannot serialize the request: {error}"))
        })?);
        drop(request);

        let deadline = started + self.deadline;
        let backoff = Backoff {
            max_elapsed: self.deadline,
            ..self.backoff
        };
        let mut attempts = 0u32;
        let result = retry_with_backoff(
            backoff,
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
                self.call_once(body.clone(), deadline)
            },
        )
        .await;

        // Only outages count against the service: a refused or invalid answer means it is up.
        match &result {
            Err(error) if error.is_transient() => permit.failure(),
            _ => permit.success(),
        }

        let elapsed = started.elapsed();
        let retries = attempts.saturating_sub(1);
        match result {
            Ok(output) => {
                record_llm_request(&self.model, "success", elapsed, retries);
                info!(
                    model = %self.model,
                    model_version = output.model_version.as_deref().unwrap_or("unknown"),
                    finish_reason = output.finish_reason.as_deref().unwrap_or("unknown"),
                    attempts,
                    elapsed_ms = elapsed.as_millis(),
                    input_tokens = output.usage.prompt_token_count,
                    output_tokens = output.usage.output_tokens(),
                    ingredients = output.extraction.ingredients.len(),
                    "Gemini extraction succeeded"
                );
                Ok(output.extraction)
            }
            Err(error) => {
                record_llm_request(&self.model, error.kind(), elapsed, retries);
                warn!(
                    model = %self.model,
                    attempts,
                    elapsed_ms = elapsed.as_millis(),
                    error = %error,
                    "Gemini extraction failed"
                );
                Err(error)
            }
        }
    }

    async fn call_once(
        &self,
        body: Bytes,
        deadline: Instant,
    ) -> Result<CallOutput, ExtractionError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ExtractionError::Timeout);
        }
        let url = format!(
            "{}/models/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            self.model
        );
        let response = self
            .client
            .post(url)
            .timeout(remaining.min(ATTEMPT_TIMEOUT))
            // The key travels in a header, never in the URL, so it cannot end up in errors.
            .header("x-goog-api-key", self.api_key.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| request_error(&error))?;

        let status = response.status();
        let retry_after = retry_after(response.headers());
        let bytes = read_limited(response).await?;
        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(error_from_status(status.as_u16(), &text, retry_after));
        }

        let parsed: GenerateContentResponse = serde_json::from_slice(&bytes).map_err(|error| {
            ExtractionError::InvalidResponse(format!("response is not the expected JSON: {error}"))
        })?;
        // Billed even when the answer is then rejected (truncated, blocked, invalid).
        let usage = parsed.usage_metadata.unwrap_or_default();
        record_llm_usage(
            &self.model,
            usage.prompt_token_count,
            usage.output_tokens(),
            estimated_cost_micro_usd(&self.model, &usage),
        );
        let model_version = parsed.model_version.clone();
        let finish_reason = parsed.finish_reason().map(str::to_string);
        Ok(CallOutput {
            extraction: parsed.into_extraction()?,
            usage,
            model_version,
            finish_reason,
        })
    }
}

impl IngredientExtractor for GeminiExtractor {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn extract<'a>(&'a self, image: &'a ImageInput) -> ExtractionFuture<'a> {
        let span = tracing::info_span!(
            "gemini_extract",
            model = %self.model,
            image_bytes = image.bytes().len()
        );
        Box::pin(self.extract_image(image).instrument(span))
    }
}

/// Reads the body, failing past [`MAX_RESPONSE_BYTES`]. Read errors (connection reset,
/// timeout) are transport errors, retried like a failed request.
async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, ExtractionError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| request_error(&error))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ExtractionError::InvalidResponse(format!(
                "response larger than {MAX_RESPONSE_BYTES} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn request_error(error: &reqwest::Error) -> ExtractionError {
    if error.is_timeout() {
        ExtractionError::Timeout
    } else {
        ExtractionError::Network(describe(error))
    }
}

/// reqwest's `Display` only says "error sending request"; add the causes. The request URL
/// carries no credentials (the key is a header), and reqwest prints it only in `Debug`.
fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
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
    use crate::secret::SecretString;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
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
        jitter: 0.0,
    };

    fn config() -> GeminiConfig {
        GeminiConfig {
            api_key: SecretString::new(KEY),
            model: MODEL.to_string(),
            ..GeminiConfig::default()
        }
    }

    fn extractor(server: &MockServer) -> Result<GeminiExtractor, ExtractionError> {
        Ok(GeminiExtractor::new(&config())?
            .with_base_url(server.uri())
            .with_backoff(FAST)
            .with_deadline(Duration::from_secs(5)))
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

        let extraction = extractor(&server)?.extract(&image()?).await?;
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

        let extraction = extractor(&server)?.extract(&image()?).await?;
        assert!(extraction.is_recipe);
        assert_eq!(requests(&server).await, 3);
        Ok(())
    }

    #[tokio::test]
    async fn rate_limits_wait_at_least_retry_after() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS))
            .with_priority(2)
            .mount(&server)
            .await;

        let extractor = extractor(&server)?.with_backoff(Backoff {
            max_delay: Duration::from_secs(2),
            ..FAST
        });
        let started = Instant::now();
        extractor.extract(&image()?).await?;
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "waited for Retry-After"
        );
        assert_eq!(requests(&server).await, 2);
        Ok(())
    }

    #[tokio::test]
    async fn client_errors_are_not_retried_and_do_not_open_the_breaker(
    ) -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(404).set_body_string(NOT_FOUND))
            .mount(&server)
            .await;

        let extractor =
            extractor(&server)?.with_breaker(CircuitBreaker::new(1, Duration::from_secs(60)));
        for _ in 0..2 {
            let error = extractor.extract(&image()?).await;
            assert!(
                matches!(error, Err(ExtractionError::Api { status: 404, .. })),
                "{error:?}"
            );
        }
        assert_eq!(
            requests(&server).await,
            2,
            "one request each, breaker still closed"
        );
        Ok(())
    }

    #[tokio::test]
    async fn malformed_success_bodies_are_invalid_responses() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
            .mount(&server)
            .await;

        let error = extractor(&server)?.extract(&image()?).await;
        assert!(
            matches!(error, Err(ExtractionError::InvalidResponse(_))),
            "{error:?}"
        );
        assert_eq!(requests(&server).await, 1, "not retried");
        Ok(())
    }

    #[tokio::test]
    async fn slow_responses_stop_at_the_image_deadline() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(SUCCESS)
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;

        let extractor = extractor(&server)?.with_deadline(Duration::from_secs(1));
        let started = Instant::now();
        let error = extractor.extract(&image()?).await;
        assert!(matches!(error, Err(ExtractionError::Timeout)), "{error:?}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "deadline respected"
        );
        Ok(())
    }

    #[tokio::test]
    async fn breaker_stops_calls_after_repeated_outages() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_body_string("{}"))
            .mount(&server)
            .await;

        // A first retry delay longer than the deadline: one attempt per image.
        let extractor = extractor(&server)?
            .with_deadline(Duration::from_millis(500))
            .with_backoff(Backoff {
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(1),
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

    #[tokio::test]
    async fn access_check_reports_an_unknown_model() -> Result<(), ExtractionError> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models/gemini-test"))
            .and(header("x-goog-api-key", KEY))
            .respond_with(ResponseTemplate::new(404).set_body_string(NOT_FOUND))
            .mount(&server)
            .await;
        let error = extractor(&server)?.verify_access().await;
        assert!(
            matches!(error, Err(ExtractionError::Api { status: 404, .. })),
            "{error:?}"
        );

        let ok_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models/gemini-test"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"name": "models/gemini-test"}"#),
            )
            .mount(&ok_server)
            .await;
        extractor(&ok_server)?.verify_access().await
    }

    #[test]
    fn breaker_rejections_are_counted() -> Result<(), Box<dyn std::error::Error>> {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let breaker = CircuitBreaker::new(1, Duration::from_secs(60));
        breaker.try_acquire()?.failure();
        let extractor = GeminiExtractor::new(&config())?.with_breaker(breaker);
        let image = image()?;

        let result = metrics::with_local_recorder(&recorder, || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map(|runtime| runtime.block_on(extractor.extract(&image)))
        })?;
        assert!(matches!(result, Err(ExtractionError::CircuitOpen)));

        let recorded = snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .any(|(key, _, _, value)| {
                key.key().name() == "llm_requests_total"
                    && key
                        .key()
                        .labels()
                        .any(|l| l.key() == "outcome" && l.value() == "circuit_open")
                    && value == DebugValue::Counter(1)
            });
        assert!(recorded, "rejection recorded with outcome circuit_open");
        Ok(())
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let no_key = GeminiConfig::default();
        assert!(matches!(
            GeminiExtractor::new(&no_key),
            Err(ExtractionError::Config(_))
        ));
        let bad_model = GeminiConfig {
            model: "../models/x".to_string(),
            ..config()
        };
        assert!(matches!(
            GeminiExtractor::new(&bad_model),
            Err(ExtractionError::Config(_))
        ));
        let bad_key = GeminiConfig {
            api_key: SecretString::new("key\u{7f}with control"),
            ..config()
        };
        assert!(matches!(
            GeminiExtractor::new(&bad_key),
            Err(ExtractionError::Config(_))
        ));
    }

    #[test]
    fn debug_output_does_not_contain_the_key() -> Result<(), ExtractionError> {
        let printed = format!("{:?}", GeminiExtractor::new(&config())?);
        assert!(!printed.contains(KEY), "{printed}");
        Ok(())
    }
}
