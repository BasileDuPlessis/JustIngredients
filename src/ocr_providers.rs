use async_trait::async_trait;
use crate::ocr_errors::OcrError;

pub mod ppocr_provider;
pub mod tesseract_provider;

#[async_trait]
pub trait OcrProvider: Send + Sync {
    async fn extract_text(&self, image_path: &str) -> Result<String, OcrError>;
    fn name(&self) -> &'static str;
    fn cost_per_request(&self) -> f64;
    fn supports_language(&self, lang: &str) -> bool;
    fn is_local(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OcrProviderType {
    Tesseract,
    Ppocr,
}