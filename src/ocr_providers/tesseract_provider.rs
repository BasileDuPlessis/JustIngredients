use std::sync::Arc;
use async_trait::async_trait;
use tracing::{info, warn};
use crate::ocr_providers::OcrProvider;
use crate::ocr_errors::OcrError;
use crate::instance_manager::OcrInstanceManager;
use crate::ocr_config::OcrConfig;

pub struct TesseractProvider {
    instance_manager: Arc<OcrInstanceManager>,
    config: OcrConfig,
}

impl TesseractProvider {
    pub fn new(instance_manager: Arc<OcrInstanceManager>, config: OcrConfig) -> Self {
        Self {
            instance_manager,
            config,
        }
    }
}

#[async_trait]
impl OcrProvider for TesseractProvider {
    async fn extract_text(&self, image_path: &str) -> Result<String, OcrError> {
        // Start timing the actual OCR processing
        let ocr_start_time = std::time::Instant::now();

        // Create a timeout for the operation
        let timeout_duration = tokio::time::Duration::from_secs(self.config.recovery.operation_timeout_secs);

        let result = tokio::time::timeout(timeout_duration, async {
            // Get or create OCR instance from the manager
            let instance = self.instance_manager
                .get_instance(&self.config)
                .map_err(|e| OcrError::Initialization(e.to_string()))?;

            // Perform OCR processing with the reused instance
            let extracted_text = {
                let mut tess = instance
                    .lock()
                    .expect("Failed to acquire Tesseract instance lock");
                // Set the image for OCR processing
                tess.set_image(image_path).map_err(|e| {
                    OcrError::ImageLoad(format!("Failed to load image for OCR: {e}"))
                })?;

                // Extract text from the image
                tess.get_utf8_text().map_err(|e| {
                    OcrError::Extraction(format!(
                        "Failed to extract text from image: {e}"
                    ))
                })?
            };

            // Clean up the extracted text (remove extra whitespace and empty lines)
            let cleaned_text = extracted_text
                .trim()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<&str>>()
                .join("\n");

            // Apply OCR error correction for common fraction misreads
            let corrected_text = crate::ocr::correct_ocr_fraction_errors(&cleaned_text);

            Ok(corrected_text)
        })
        .await;

        let ocr_duration = ocr_start_time.elapsed();
        let ocr_ms = ocr_duration.as_millis();

        match result {
            Ok(Ok(text)) => {
                info!(
                    "Tesseract OCR processing completed in {}ms, extracted {} characters",
                    ocr_ms,
                    text.len()
                );
                Ok(text)
            }
            Ok(Err(e)) => {
                warn!("Tesseract OCR processing failed after {ocr_ms}ms: {e:?}");
                Err(e)
            }
            Err(_) => {
                warn!(
                    "Tesseract OCR processing timed out after {}ms (limit: {}s)",
                    ocr_ms, self.config.recovery.operation_timeout_secs
                );
                Err(OcrError::Timeout(format!(
                    "OCR operation timed out after {} seconds",
                    self.config.recovery.operation_timeout_secs
                )))
            }
        }
    }

    fn name(&self) -> &'static str {
        "Tesseract"
    }

    fn cost_per_request(&self) -> f64 {
        0.0 // Tesseract is free
    }

    fn supports_language(&self, lang: &str) -> bool {
        // Tesseract supports English and French by default
        lang == "en" || lang == "fr"
    }

    fn is_local(&self) -> bool {
        true // Tesseract runs locally
    }
}