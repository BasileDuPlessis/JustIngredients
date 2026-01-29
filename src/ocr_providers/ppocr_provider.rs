use std::path::PathBuf;
use async_trait::async_trait;
use tokio::process::Command;
use crate::ocr_providers::OcrProvider;
use crate::ocr_errors::OcrError;
use crate::ocr_config::PpocrConfig;

pub struct PpocrProvider {
    python_executable: PathBuf,
    #[allow(dead_code)]
    model_cache_dir: PathBuf,
    #[allow(dead_code)]
    language: String,
    #[allow(dead_code)]
    use_gpu: bool,
    #[allow(dead_code)]
    timeout_seconds: u64,
}

impl PpocrProvider {
    pub fn new(config: &PpocrConfig) -> Result<Self, OcrError> {
        Ok(Self {
            python_executable: config.python_path.clone(),
            model_cache_dir: config.model_cache_dir.clone(),
            language: config.language.clone(),
            use_gpu: config.use_gpu,
            timeout_seconds: config.timeout_seconds,
        })
    }

    fn build_ppocr_script(&self, image_path: &str) -> String {
        let use_gpu_str = if self.use_gpu { "True" } else { "False" };
        format!(r#"
import paddleocr
import sys
import json
import os

def main():
    image_path = r"{}"
    lang = "{}"
    use_gpu = {}
    
    try:
        # Initialize OCR
        ocr = paddleocr.PaddleOCR(
            use_angle_cls=True,
            lang=lang,
            use_gpu=use_gpu,
            show_log=False
        )
        
        # Process image
        result = ocr.ocr(image_path, cls=True)
        
        # Extract text
        text_results = []
        if result and result[0]:
            for line in result[0]:
                text_results.append(line[1][0])
        
        print(json.dumps(text_results))
        
    except Exception as e:
        print(json.dumps({{"error": str(e)}}))
        sys.exit(1)

if __name__ == "__main__":
    main()
"#, image_path.escape_default(), self.language, use_gpu_str)
    }

    fn parse_ppocr_output(&self, output: &str) -> Result<String, OcrError> {
        let json_value: serde_json::Value = serde_json::from_str(output.trim())
            .map_err(|e| OcrError::Extraction(format!("Failed to parse PPOCR output: {}", e)))?;
        
        if let Some(error) = json_value.get("error") {
            return Err(OcrError::Extraction(format!("PPOCR error: {}", error)));
        }
        
        if let Some(text_array) = json_value.as_array() {
            let text_lines: Vec<String> = text_array
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
            Ok(text_lines.join("\n"))
        } else {
            Err(OcrError::Extraction("Unexpected PPOCR output format".to_string()))
        }
    }
}

#[async_trait]
impl OcrProvider for PpocrProvider {
    async fn extract_text(&self, image_path: &str) -> Result<String, OcrError> {
        let script = self.build_ppocr_script(image_path);
        
        let output = Command::new(&self.python_executable)
            .arg("-c")
            .arg(&script)
            .output()
            .await
            .map_err(|e| OcrError::Extraction(format!("Failed to run PPOCR: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OcrError::Extraction(format!("PPOCR process failed: {}", stderr)));
        }
        
        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| OcrError::Extraction(format!("Invalid UTF-8 output: {}", e)))?;
        
        self.parse_ppocr_output(&stdout)
    }

    fn name(&self) -> &'static str { "PPOCR" }
    fn cost_per_request(&self) -> f64 { 0.0 }
    fn supports_language(&self, lang: &str) -> bool { lang == "en" || lang == "fr" }
    fn is_local(&self) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_config() -> PpocrConfig {
        PpocrConfig {
            enabled: true,
            python_path: PathBuf::from("/usr/bin/python3"),
            model_cache_dir: PathBuf::from("/tmp/ppocr"),
            language: "en".to_string(),
            use_gpu: false,
            timeout_seconds: 30,
        }
    }
    
    #[test]
    fn test_ppocr_provider_creation() {
        let config = test_config();
        let provider = PpocrProvider::new(&config).unwrap();
        assert_eq!(provider.name(), "PPOCR");
        assert_eq!(provider.cost_per_request(), 0.0);
        assert!(provider.is_local());
    }
    
    #[test]
    fn test_language_support() {
        let config = test_config();
        let provider = PpocrProvider::new(&config).unwrap();
        assert!(provider.supports_language("en"));
        assert!(provider.supports_language("fr"));
        assert!(!provider.supports_language("de"));
    }
    
    #[tokio::test]
    async fn test_ppocr_extraction_with_mock() {
        // This would need a mock Python script for testing
        // For now, test the error case
        let config = test_config();
        let provider = PpocrProvider::new(&config).unwrap();
        let result = provider.extract_text("nonexistent.png").await;
        assert!(result.is_err());
    }
}