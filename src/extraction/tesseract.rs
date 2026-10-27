//! Tesseract OCR behind [`IngredientExtractor`], kept as a fallback during the Gemini rollout
//! (`EXTRACTION_BACKEND=tesseract`). Removed with the OCR code in Phase 5 of
//! `specs/tasks/llm-extraction-migration.md`.

use std::io::Write;

use crate::bot::image_processing::process_ingredients_with_recovery;
use crate::circuit_breaker::CircuitBreaker;
use crate::extraction::{
    ExtractedIngredient, Extraction, ExtractionError, ExtractionFuture, ImageInput, ImageMime,
    IngredientExtractor, RecipeLanguage,
};
use crate::instance_manager::OcrInstanceManager;
use crate::ocr_config::OcrConfig;
use crate::ocr_errors::OcrError;
use crate::validation::parse_quantity;

/// The previous OCR pipeline: Tesseract text extraction, then regex measurement detection
/// with crop-and-retry recovery of unreadable quantities.
pub struct TesseractExtractor {
    config: OcrConfig,
    instances: OcrInstanceManager,
    breaker: CircuitBreaker,
}

impl std::fmt::Debug for TesseractExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TesseractExtractor")
            .field("languages", &self.config.languages)
            .finish_non_exhaustive()
    }
}

impl Default for TesseractExtractor {
    fn default() -> Self {
        let config = OcrConfig::default();
        let breaker = CircuitBreaker::new(config.recovery.clone());
        Self {
            config,
            instances: OcrInstanceManager::default(),
            breaker,
        }
    }
}

impl TesseractExtractor {
    async fn extract_image(&self, image: &ImageInput) -> Result<Extraction, ExtractionError> {
        // The OCR code reads files and picks the decoder from the extension.
        let suffix = match image.mime() {
            ImageMime::Jpeg => ".jpg",
            ImageMime::Png => ".png",
            ImageMime::Webp => ".webp",
        };
        let mut file = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .map_err(|error| {
                ExtractionError::InvalidResponse(format!("temporary file: {error}"))
            })?;
        file.write_all(image.bytes()).map_err(|error| {
            ExtractionError::InvalidResponse(format!("temporary file: {error}"))
        })?;
        let path = file.path().to_string_lossy().to_string();

        let (text, _confidence) = crate::ocr::extract_text_from_image(
            &path,
            &self.config,
            &self.instances,
            &self.breaker,
        )
        .await
        .map_err(ocr_error)?;
        if text.trim().is_empty() {
            return Ok(Extraction {
                is_recipe: false,
                language: RecipeLanguage::Other,
                transcription: String::new(),
                ingredients: Vec::new(),
            });
        }

        let matches = process_ingredients_with_recovery(
            &text,
            &path,
            &self.config,
            &self.instances,
            &self.breaker,
            None,
        )
        .await;
        let ingredients = matches
            .into_iter()
            .map(|measurement| ExtractedIngredient {
                quantity: parse_quantity(&measurement.quantity),
                name: measurement.ingredient_name,
                unit: measurement.measurement,
                needs_review: measurement.requires_quantity_confirmation,
                source_text: String::new(),
            })
            .collect();
        // Text was found: show it when no ingredient is detected, as before.
        Ok(Extraction {
            is_recipe: true,
            language: RecipeLanguage::Other,
            transcription: text,
            ingredients,
        })
    }
}

impl IngredientExtractor for TesseractExtractor {
    fn name(&self) -> &'static str {
        "tesseract"
    }

    fn extract<'a>(&'a self, image: &'a ImageInput) -> ExtractionFuture<'a> {
        Box::pin(self.extract_image(image))
    }
}

fn ocr_error(error: OcrError) -> ExtractionError {
    match error {
        OcrError::Validation(message) | OcrError::ImageLoad(message) => {
            ExtractionError::UnsupportedImage(message)
        }
        OcrError::Timeout(_) => ExtractionError::Timeout,
        other => ExtractionError::InvalidResponse(format!("OCR failed: {other}")),
    }
}
