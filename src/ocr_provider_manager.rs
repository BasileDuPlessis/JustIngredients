use std::collections::HashMap;
use std::sync::Arc;
use crate::ocr_providers::{OcrProvider, OcrProviderType};
use crate::ocr_providers::ppocr_provider::PpocrProvider;
use crate::ocr_providers::tesseract_provider::TesseractProvider;
use crate::ocr_config::{PpocrConfig, OcrConfig};
use crate::ocr_errors::OcrError;
use crate::instance_manager::OcrInstanceManager;

pub struct OcrProviderManager {
    providers: HashMap<OcrProviderType, Box<dyn OcrProvider>>,
    primary_provider: OcrProviderType,
    #[allow(dead_code)]
    fallback_providers: Vec<OcrProviderType>,
}

impl OcrProviderManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            primary_provider: OcrProviderType::Tesseract, // default
            fallback_providers: vec![OcrProviderType::Tesseract],
        }
    }

    pub fn register_provider(&mut self, provider_type: OcrProviderType, provider: Box<dyn OcrProvider>) {
        self.providers.insert(provider_type, provider);
    }

    pub fn register_default_tesseract_provider(&mut self, instance_manager: Arc<OcrInstanceManager>, config: OcrConfig) {
        let provider = Box::new(TesseractProvider::new(instance_manager, config));
        self.register_provider(OcrProviderType::Tesseract, provider);
    }

    pub fn register_ppocr_provider(&mut self, config: PpocrConfig) -> Result<(), OcrError> {
        let provider = Box::new(PpocrProvider::new(&config)?);
        self.register_provider(OcrProviderType::Ppocr, provider);
        Ok(())
    }

    pub fn set_primary_provider(&mut self, provider_type: OcrProviderType) {
        self.primary_provider = provider_type;
    }

    pub async fn extract_text(&self, image_path: &str) -> Result<String, OcrError> {
        let provider = self.providers.get(&self.primary_provider)
            .ok_or_else(|| OcrError::Extraction(format!("No provider registered for {:?}", self.primary_provider)))?;

        provider.extract_text(image_path).await
    }
}

impl Default for OcrProviderManager {
    fn default() -> Self {
        Self::new()
    }
}