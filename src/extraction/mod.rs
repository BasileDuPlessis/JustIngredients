//! # Ingredient Extraction
//!
//! Turns a recipe image into structured ingredients. The Gemini vision API replaces the
//! Tesseract OCR pipeline (see `specs/tasks/llm-extraction-migration.md`).
//!
//! The bot depends only on [`IngredientExtractor`], so it can be tested with a fake
//! extractor and the backend can change without touching the handlers. [`mapping`] turns an
//! [`Extraction`] into the `MeasurementMatch` values used by the review, edit and save code.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod config;
pub mod gemini;
pub mod mapping;
pub mod prompt;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::Deserialize;

pub use config::{ExtractionBackend, ExtractionConfig, GeminiConfig, ThinkingLevel};

/// Image formats accepted by the extraction API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMime {
    Jpeg,
    Png,
    Webp,
}

impl ImageMime {
    /// MIME type sent with the image.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }

    /// Detects the format from the file signature, ignoring any file name or declared type.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Self::Png)
        } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }
}

/// An image to extract ingredients from, held in memory.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageInput {
    bytes: Vec<u8>,
    mime: ImageMime,
}

impl ImageInput {
    /// Wraps image bytes, detecting the format from their signature.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::UnsupportedImage`] when the bytes are not JPEG, PNG or WebP.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ExtractionError> {
        let mime = ImageMime::sniff(&bytes).ok_or_else(|| {
            ExtractionError::UnsupportedImage("expected a JPEG, PNG or WebP image".to_string())
        })?;
        Ok(Self { bytes, mime })
    }

    /// The image file content.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The detected image format.
    pub fn mime(&self) -> ImageMime {
        self.mime
    }
}

impl std::fmt::Debug for ImageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageInput")
            .field("mime", &self.mime)
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// Language of the recipe, as detected by the extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeLanguage {
    Fr,
    En,
    #[serde(other)]
    Other,
}

/// One ingredient as returned by the extractor (field names match the JSON schema in
/// `config/extraction/schema.json`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExtractedIngredient {
    /// Ingredient in the source language, without quantity, unit or article.
    pub name: String,
    /// Decimal quantity; `None` when the recipe gives none ("sel").
    pub quantity: Option<f64>,
    /// Unit as written or canonical; normalized again by [`mapping`].
    pub unit: Option<String>,
    /// The extractor is unsure about this line (hard to read, ambiguous quantity, range).
    pub needs_review: bool,
    /// The line exactly as printed.
    pub source_text: String,
}

/// Result of one extraction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Extraction {
    /// `false` when the image is not a recipe or ingredient list.
    pub is_recipe: bool,
    pub language: RecipeLanguage,
    /// The ingredient section as plain text, one line per ingredient.
    pub transcription: String,
    pub ingredients: Vec<ExtractedIngredient>,
}

/// Why an extraction failed. Messages never contain credentials or URLs with credentials.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExtractionError {
    /// Missing or invalid configuration (e.g. no API key).
    #[error("extraction is not configured: {0}")]
    Config(String),
    /// The image is not JPEG, PNG or WebP.
    #[error("unsupported image: {0}")]
    UnsupportedImage(String),
    /// The image exceeds the size accepted for extraction.
    #[error("image too large: {size} bytes (maximum {max})")]
    ImageTooLarge { size: u64, max: u64 },
    /// The service rejected the call with HTTP 429; `retry_after` comes from `Retry-After`.
    #[error("extraction service rate limit reached")]
    RateLimited { retry_after: Option<Duration> },
    /// The call did not complete within the configured timeout.
    #[error("extraction service timed out")]
    Timeout,
    /// The service could not be reached (DNS, TLS, connection).
    #[error("extraction service unreachable: {0}")]
    Network(String),
    /// The service answered with another HTTP error status.
    #[error("extraction service returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    /// The model declined to answer (e.g. safety block); the reason is the API's.
    #[error("extraction refused by the model: {0}")]
    Blocked(String),
    /// The response hit the output token limit before the JSON was complete.
    #[error("extraction output was cut off before the end")]
    Truncated,
    /// The response could not be parsed into an [`Extraction`].
    #[error("extraction returned an invalid response: {0}")]
    InvalidResponse(String),
    /// Calls are suspended by the circuit breaker after repeated failures.
    #[error("extraction temporarily disabled after repeated failures")]
    CircuitOpen,
}

impl ExtractionError {
    /// Whether retrying the same request later can succeed: rate limits (429), timeouts,
    /// network errors and server errors (5xx).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Timeout | Self::Network(_)
        ) || matches!(self, Self::Api { status, .. } if *status == 429 || *status >= 500)
    }
}

/// Future returned by [`IngredientExtractor::extract`].
pub type ExtractionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Extraction, ExtractionError>> + Send + 'a>>;

/// Extracts ingredients from an image. Object safe, so the bot can hold an
/// `Arc<dyn IngredientExtractor>` chosen from [`ExtractionConfig::backend`].
pub trait IngredientExtractor: Send + Sync {
    /// Short name for logs and metrics, e.g. `"gemini"`.
    fn name(&self) -> &'static str;

    /// Extracts the ingredients shown in `image`.
    fn extract<'a>(&'a self, image: &'a ImageInput) -> ExtractionFuture<'a>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_formats_are_detected_from_their_signature() {
        assert_eq!(
            ImageMime::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(ImageMime::Jpeg)
        );
        assert_eq!(
            ImageMime::sniff(b"\x89PNG\r\n\x1a\n...."),
            Some(ImageMime::Png)
        );
        assert_eq!(
            ImageMime::sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageMime::Webp)
        );
        assert_eq!(ImageMime::sniff(b"GIF89a"), None);
        assert_eq!(ImageMime::sniff(b""), None);
    }

    #[test]
    fn image_input_rejects_unknown_formats() {
        assert!(matches!(
            ImageInput::from_bytes(b"%PDF-1.7".to_vec()),
            Err(ExtractionError::UnsupportedImage(_))
        ));
        let image = ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, 0x00]);
        assert!(matches!(image, Ok(ref i) if i.mime() == ImageMime::Jpeg));
    }

    #[test]
    fn image_debug_does_not_dump_the_bytes() {
        let printed = match ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, 0x42]) {
            Ok(image) => format!("{image:?}"),
            Err(error) => format!("unexpected error: {error}"),
        };
        assert_eq!(printed, "ImageInput { mime: Jpeg, len: 4 }");
    }

    #[test]
    fn model_json_deserializes_including_unknown_languages() -> Result<(), serde_json::Error> {
        let extraction: Extraction = serde_json::from_str(
            r#"{"is_recipe": true, "language": "de", "transcription": "150 g Mehl",
                "ingredients": [{"name": "mehl", "quantity": 150, "unit": "g",
                                 "needs_review": false, "source_text": "150 g Mehl"},
                                {"name": "salz", "quantity": null, "unit": null,
                                 "needs_review": false, "source_text": "Salz"}]}"#,
        )?;
        assert_eq!(extraction.language, RecipeLanguage::Other);
        assert_eq!(extraction.ingredients.len(), 2);
        assert_eq!(extraction.ingredients[0].quantity, Some(150.0));
        assert_eq!(extraction.ingredients[1].quantity, None);
        Ok(())
    }

    #[test]
    fn only_temporary_failures_are_transient() {
        assert!(ExtractionError::Timeout.is_transient());
        assert!(ExtractionError::RateLimited { retry_after: None }.is_transient());
        assert!(ExtractionError::Api {
            status: 503,
            message: String::new()
        }
        .is_transient());
        assert!(ExtractionError::Api {
            status: 429,
            message: String::new()
        }
        .is_transient());
        assert!(!ExtractionError::Api {
            status: 400,
            message: String::new()
        }
        .is_transient());
        assert!(!ExtractionError::Blocked("SAFETY".to_string()).is_transient());
        assert!(!ExtractionError::InvalidResponse(String::new()).is_transient());
    }
}
