//! Request and response bodies of the Gemini `generateContent` REST endpoint, and the
//! interpretation of a response into an [`Extraction`] or an [`ExtractionError`].
//!
//! Fixtures recorded from the real API live in `tests/fixtures/gemini/`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::extraction::{Extraction, ExtractionError, ImageInput, ThinkingLevel};

/// Upper bound on response tokens: a long ingredient list with its transcription stays far
/// below it, and a runaway response stops with `MAX_TOKENS` instead of costing more.
pub const MAX_OUTPUT_TOKENS: u32 = 8192;

/// Body of `POST /v1beta/models/{model}:generateContent`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest<'a> {
    contents: [Content<'a>; 1],
    generation_config: GenerationConfig<'a>,
}

#[derive(Debug, Serialize)]
struct Content<'a> {
    role: &'static str,
    parts: [Part<'a>; 2],
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Part<'a> {
    Image {
        #[serde(rename = "inlineData")]
        inline_data: InlineData,
    },
    Text {
        text: &'a str,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: &'static str,
    data: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig<'a> {
    response_mime_type: &'static str,
    response_json_schema: &'a RawValue,
    thinking_config: ThinkingConfig,
    max_output_tokens: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThinkingConfig {
    thinking_level: &'static str,
}

impl<'a> GenerateContentRequest<'a> {
    /// Builds the request: image first, then the instructions (the order Gemini recommends).
    pub fn new(
        image: &ImageInput,
        prompt: &'a str,
        schema: &'a RawValue,
        thinking_level: ThinkingLevel,
    ) -> Self {
        Self {
            contents: [Content {
                role: "user",
                parts: [
                    Part::Image {
                        inline_data: InlineData {
                            mime_type: image.mime().as_str(),
                            data: base64::engine::general_purpose::STANDARD.encode(image.bytes()),
                        },
                    },
                    Part::Text { text: prompt },
                ],
            }],
            generation_config: GenerationConfig {
                response_mime_type: "application/json",
                response_json_schema: schema,
                thinking_config: ThinkingConfig {
                    thinking_level: thinking_level.as_str(),
                },
                max_output_tokens: MAX_OUTPUT_TOKENS,
            },
        }
    }
}

/// Successful (HTTP 200) body of `generateContent`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    prompt_feedback: Option<PromptFeedback>,
    /// Token counts, used for metrics and cost.
    pub usage_metadata: Option<UsageMetadata>,
    /// Exact model version that answered.
    pub model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<CandidateContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<ResponsePart>,
}

#[derive(Debug, Deserialize)]
struct ResponsePart {
    text: Option<String>,
    #[serde(default)]
    thought: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    block_reason: Option<String>,
}

/// Token usage reported by the API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    #[serde(default)]
    pub prompt_token_count: u64,
    #[serde(default)]
    pub candidates_token_count: u64,
    /// Thinking tokens, billed as output tokens.
    #[serde(default)]
    pub thoughts_token_count: u64,
}

impl UsageMetadata {
    /// Tokens billed at the output price (answer and thinking).
    pub fn output_tokens(&self) -> u64 {
        self.candidates_token_count + self.thoughts_token_count
    }
}

impl GenerateContentResponse {
    /// Why the first candidate stopped (`STOP`, `MAX_TOKENS`, `SAFETY`, …), for logs.
    pub fn finish_reason(&self) -> Option<&str> {
        self.candidates.first()?.finish_reason.as_deref()
    }

    /// Interprets the response: the model's JSON answer, or why there is none.
    ///
    /// # Errors
    ///
    /// - [`ExtractionError::Blocked`] when the prompt or the answer was blocked;
    /// - [`ExtractionError::Truncated`] when the answer stopped at the token limit;
    /// - [`ExtractionError::InvalidResponse`] when there is no answer or it is not valid JSON
    ///   for [`Extraction`].
    pub fn into_extraction(self) -> Result<Extraction, ExtractionError> {
        if let Some(reason) = self.prompt_feedback.and_then(|f| f.block_reason) {
            return Err(ExtractionError::Blocked(reason));
        }
        let candidate = self.candidates.into_iter().next().ok_or_else(|| {
            ExtractionError::InvalidResponse("response has no candidates".to_string())
        })?;

        match candidate.finish_reason.as_deref() {
            None | Some("STOP") => {}
            Some("MAX_TOKENS") => return Err(ExtractionError::Truncated),
            Some("RECITATION") => return Err(ExtractionError::Recitation),
            Some(reason) => return Err(ExtractionError::Blocked(reason.to_string())),
        }

        let text: String = candidate
            .content
            .map(|content| content.parts)
            .unwrap_or_default()
            .into_iter()
            .filter(|part| !part.thought)
            .filter_map(|part| part.text)
            .collect();
        if text.trim().is_empty() {
            return Err(ExtractionError::InvalidResponse(
                "response has no text".to_string(),
            ));
        }
        serde_json::from_str(&text).map_err(|error| {
            ExtractionError::InvalidResponse(format!("answer does not match the schema: {error}"))
        })
    }
}

/// Error body returned with non-2xx statuses: `{"error": {"code", "message", "status"}}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    message: String,
    status: Option<String>,
    /// `google.rpc` details; `RetryInfo.retryDelay` ("7s") says when to retry a 429.
    #[serde(default)]
    details: Vec<serde_json::Value>,
}

impl ErrorDetail {
    fn retry_delay(&self) -> Option<std::time::Duration> {
        self.details
            .iter()
            .filter(|detail| {
                detail["@type"]
                    .as_str()
                    .is_some_and(|kind| kind.ends_with("google.rpc.RetryInfo"))
            })
            .find_map(|detail| {
                let seconds: f64 = detail["retryDelay"]
                    .as_str()?
                    .strip_suffix('s')?
                    .parse()
                    .ok()?;
                (seconds.is_finite() && seconds >= 0.0)
                    .then(|| std::time::Duration::from_secs_f64(seconds))
            })
    }
}

/// Longest API error message kept in [`ExtractionError::Api`].
const MAX_ERROR_MESSAGE_CHARS: usize = 300;

/// Turns a non-2xx response into an error. `retry_after` is the parsed `Retry-After` header.
pub fn error_from_status(
    status: u16,
    body: &str,
    retry_after: Option<std::time::Duration>,
) -> ExtractionError {
    let parsed = serde_json::from_str::<ErrorBody>(body).ok();
    if status == 429 {
        return ExtractionError::RateLimited {
            retry_after: retry_after.or_else(|| parsed.as_ref()?.error.retry_delay()),
        };
    }
    let message = match parsed {
        Some(parsed) => match parsed.error.status {
            Some(code) => format!("{code}: {}", parsed.error.message),
            None => parsed.error.message,
        },
        None => "unexpected error body".to_string(),
    };
    ExtractionError::Api {
        status,
        message: message.chars().take(MAX_ERROR_MESSAGE_CHARS).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::prompt::{response_schema, EXTRACTION_PROMPT};
    use crate::extraction::RecipeLanguage;

    const SUCCESS: &str = include_str!("../../../tests/fixtures/gemini/success.json");
    const NOT_FOUND: &str =
        include_str!("../../../tests/fixtures/gemini/error_404_model_not_found.json");
    const INVALID_ARGUMENT: &str =
        include_str!("../../../tests/fixtures/gemini/error_400_invalid_argument.json");

    fn response(json: &str) -> Result<GenerateContentResponse, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn request_has_the_shape_the_api_expects() -> Result<(), Box<dyn std::error::Error>> {
        let image = ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, 0x01])?;
        let schema = response_schema()?;
        let request =
            GenerateContentRequest::new(&image, EXTRACTION_PROMPT, &schema, ThinkingLevel::Minimal);
        let body: serde_json::Value = serde_json::from_str(&serde_json::to_string(&request)?)?;

        let parts = &body["contents"][0]["parts"];
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(parts[0]["inlineData"]["mimeType"], "image/jpeg");
        assert_eq!(parts[0]["inlineData"]["data"], "/9j/AQ==");
        assert_eq!(parts[1]["text"], EXTRACTION_PROMPT);
        let config = &body["generationConfig"];
        assert_eq!(config["responseMimeType"], "application/json");
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "minimal");
        assert_eq!(config["maxOutputTokens"], MAX_OUTPUT_TOKENS);
        assert_eq!(
            config["responseJsonSchema"]["required"][0], "is_recipe",
            "schema embedded as-is"
        );
        Ok(())
    }

    #[test]
    fn recorded_success_response_becomes_an_extraction() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = response(SUCCESS)?;
        let usage = parsed.usage_metadata.unwrap_or_default();
        assert!(usage.prompt_token_count > 1000, "image tokens included");
        assert!(usage.output_tokens() > 0);
        let extraction = parsed.into_extraction()?;
        assert!(extraction.is_recipe);
        assert_eq!(extraction.language, RecipeLanguage::En);
        assert_eq!(extraction.ingredients.len(), 7);
        assert_eq!(extraction.ingredients[1].name, "unsalted butter");
        assert_eq!(extraction.ingredients[1].unit.as_deref(), Some("tbsp"));
        Ok(())
    }

    #[test]
    fn thought_parts_are_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let extraction = response(
            r#"{"candidates": [{"finishReason": "STOP", "content": {"parts": [
                {"text": "thinking about it", "thought": true},
                {"text": "{\"is_recipe\": false, \"language\": \"other\", \"transcription\": \"\", \"ingredients\": []}"}
            ]}}]}"#,
        )?
        .into_extraction()?;
        assert!(!extraction.is_recipe);
        Ok(())
    }

    #[test]
    fn blocked_truncated_and_empty_responses_are_errors() -> Result<(), serde_json::Error> {
        let cases = [
            (
                r#"{"promptFeedback": {"blockReason": "SAFETY"}}"#,
                "blocked prompt",
            ),
            (
                r#"{"candidates": [{"finishReason": "SAFETY"}]}"#,
                "blocked answer",
            ),
            (
                r#"{"candidates": [{"finishReason": "MAX_TOKENS", "content": {"parts": [{"text": "{\"is_rec"}]}}]}"#,
                "truncated",
            ),
            (r#"{"candidates": []}"#, "no candidate"),
            (
                r#"{"candidates": [{"finishReason": "STOP", "content": {"parts": []}}]}"#,
                "no text",
            ),
            (
                r#"{"candidates": [{"finishReason": "STOP", "content": {"parts": [{"text": "{\"oops\": 1}"}]}}]}"#,
                "wrong JSON",
            ),
        ];
        let results: Vec<_> = cases
            .iter()
            .map(|(json, label)| response(json).map(|r| (r.into_extraction(), *label)))
            .collect::<Result<_, _>>()?;
        assert!(matches!(results[0].0, Err(ExtractionError::Blocked(ref r)) if r == "SAFETY"));
        assert!(matches!(results[1].0, Err(ExtractionError::Blocked(_))));
        assert!(matches!(results[2].0, Err(ExtractionError::Truncated)));
        for (result, label) in &results[3..] {
            assert!(
                matches!(result, Err(ExtractionError::InvalidResponse(_))),
                "{label}: {result:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn recorded_error_bodies_become_api_errors() {
        let not_found = error_from_status(404, NOT_FOUND, None);
        assert!(
            matches!(&not_found, ExtractionError::Api { status: 404, message } if message.starts_with("NOT_FOUND: ")),
            "{not_found:?}"
        );
        assert!(!not_found.is_transient());

        let invalid = error_from_status(400, INVALID_ARGUMENT, None);
        assert!(
            matches!(&invalid, ExtractionError::Api { status: 400, message } if message.contains("Thinking level")),
            "{invalid:?}"
        );
    }

    #[test]
    fn recitation_and_retry_delay_in_the_body_are_recognized() -> Result<(), serde_json::Error> {
        let recitation = response(r#"{"candidates": [{"finishReason": "RECITATION"}]}"#)?;
        assert_eq!(recitation.finish_reason(), Some("RECITATION"));
        assert!(matches!(
            recitation.into_extraction(),
            Err(ExtractionError::Recitation)
        ));

        let body = r#"{"error": {"code": 429, "message": "Quota exceeded", "status": "RESOURCE_EXHAUSTED",
            "details": [{"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "7s"}]}}"#;
        assert!(matches!(
            error_from_status(429, body, None),
            ExtractionError::RateLimited { retry_after: Some(d) } if d.as_secs() == 7
        ));
        assert!(
            matches!(
                error_from_status(429, body, Some(std::time::Duration::from_secs(2))),
                ExtractionError::RateLimited { retry_after: Some(d) } if d.as_secs() == 2
            ),
            "the header wins"
        );
        Ok(())
    }

    #[test]
    fn rate_limits_and_odd_bodies_are_handled() {
        let limited = error_from_status(429, "{}", Some(std::time::Duration::from_secs(7)));
        assert!(matches!(
            limited,
            ExtractionError::RateLimited { retry_after: Some(d) } if d.as_secs() == 7
        ));
        let html = error_from_status(502, "<html>Bad gateway</html>", None);
        assert!(
            matches!(&html, ExtractionError::Api { status: 502, message } if message == "unexpected error body")
        );
        assert!(html.is_transient());
        let long = format!(r#"{{"error": {{"message": "{}"}}}}"#, "x".repeat(1000));
        assert!(matches!(
            error_from_status(400, &long, None),
            ExtractionError::Api { message, .. } if message.chars().count() == MAX_ERROR_MESSAGE_CHARS
        ));
    }
}
