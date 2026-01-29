# PPOCR Implementation Study for JustIngredients

## Executive Summary

This document outlines the implementation of PaddleOCR (PPOCR) as an alternative OCR provider in the JustIngredients Telegram bot. The goal is to enhance OCR accuracy for recipe ingredient extraction while maintaining the existing architecture's reliability patterns.

## Current Architecture Analysis

### Core Components

**OCR Module (`src/ocr.rs`)**
- **Main Function**: `extract_text_from_image()` - orchestrates OCR processing
- **Validation**: `validate_image_with_format_limits()` - size and format checks
- **Processing**: `perform_ocr_extraction()` - actual Tesseract processing
- **Error Handling**: Custom `OcrError` types with circuit breaker integration

**Instance Management (`src/instance_manager.rs`)**
- **Purpose**: Pool and reuse OCR instances for performance
- **Current**: Tesseract instance caching with language-specific pools
- **Integration**: Works with circuit breaker and configuration

**Circuit Breaker (`src/circuit_breaker.rs`)**
- **Pattern**: Prevents cascade failures during OCR outages
- **Thresholds**: Configurable failure/success tracking
- **Integration**: Protects all OCR operations

**Configuration (`src/ocr_config.rs`)**
- **Structure**: `OcrConfig` with timeouts, limits, recovery settings
- **Languages**: Currently `eng+fra` for English/French
- **Limits**: File size limits by format (PNG:15MB, JPEG:10MB, etc.)

### Integration Points

**Bot Processing (`src/bot/image_processing.rs`)**
```rust
// Current flow:
download_and_process_image() 
    ↓
validate_image_format()
    ↓
extract_text_from_image() // Calls Tesseract
    ↓
process_extracted_text()
```

**Error Propagation**
- All OCR errors flow through `OcrError` enum
- Circuit breaker state affects processing decisions
- User-friendly messages via localization

## PPOCR Integration Architecture

### Provider Abstraction Layer

**New Trait Definition**
```rust
// src/ocr_providers.rs
#[async_trait]
pub trait OcrProvider: Send + Sync {
    async fn extract_text(&self, image_path: &str) -> Result<String, OcrError>;
    fn name(&self) -> &'static str;
    fn cost_per_request(&self) -> f64;
    fn supports_language(&self, lang: &str) -> bool;
    fn is_local(&self) -> bool;
}

// Provider types
pub enum OcrProviderType {
    Tesseract,
    Ppocr,
    Gpt4Vision,
}
```

**Provider Manager**
```rust
// src/ocr_provider_manager.rs
pub struct OcrProviderManager {
    providers: HashMap<OcrProviderType, Box<dyn OcrProvider>>,
    primary_provider: OcrProviderType,
    fallback_providers: Vec<OcrProviderType>,
}

impl OcrProviderManager {
    pub async fn extract_text(&self, image_path: &str) -> Result<String, OcrError> {
        // Try primary provider
        match self.try_provider(self.primary_provider, image_path).await {
            Ok(text) => return Ok(text),
            Err(e) => {
                warn!("Primary provider {} failed: {}", self.primary_provider.as_str(), e);
                // Try fallbacks...
            }
        }
    }
}
```

### PPOCR Provider Implementation

**Python Subprocess Approach (Recommended)**
```rust
// src/ocr_providers/ppocr_provider.rs
pub struct PpocrProvider {
    python_executable: PathBuf,
    model_cache_dir: PathBuf,
    language: String,
    use_gpu: bool,
}

impl PpocrProvider {
    pub fn new(config: &PpocrConfig) -> Result<Self> {
        Ok(Self {
            python_executable: config.python_path.clone(),
            model_cache_dir: config.model_cache_dir.clone(),
            language: config.language.clone(),
            use_gpu: config.use_gpu,
        })
    }
}

#[async_trait]
impl OcrProvider for PpocrProvider {
    async fn extract_text(&self, image_path: &str) -> Result<String, OcrError> {
        let script = self.build_ppocr_script(image_path);
        let output = self.run_python_script(&script).await?;
        self.parse_ppocr_output(&output)
    }

    fn name(&self) -> &'static str { "PPOCR" }
    fn cost_per_request(&self) -> f64 { 0.0 } // Local processing
    fn supports_language(&self, lang: &str) -> bool { lang == "en" || lang == "fr" }
    fn is_local(&self) -> bool { true }
}
```

**Python Script Template**
```python
# Embedded in Rust as string
PPOCR_SCRIPT = r#"
import paddleocr
import sys
import json
import os

def main():
    image_path = sys.argv[1]
    lang = sys.argv[2]
    use_gpu = sys.argv[3].lower() == 'true'
    
    # Initialize with caching
    ocr = paddleocr.PaddleOCR(
        use_angle_cls=True,
        lang=lang,
        use_gpu=use_gpu,
        show_log=False  # Reduce noise
    )
    
    # Process image
    result = ocr.ocr(image_path, cls=True)
    
    # Extract text lines
    text_results = []
    if result and result[0]:
        for line in result[0]:
            text_results.append(line[1][0])  # Text content
    
    print(json.dumps(text_results))

if __name__ == "__main__":
    main()
"#;
```

### Configuration Extensions

**New Config Structure**
```rust
// src/ocr_config.rs additions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PpocrConfig {
    pub enabled: bool,
    pub python_path: PathBuf,
    pub model_cache_dir: PathBuf,
    pub language: String,
    pub use_gpu: bool,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrProviderConfig {
    pub primary_provider: OcrProviderType,
    pub fallback_providers: Vec<OcrProviderType>,
    pub tesseract: TesseractConfig, // existing
    pub ppocr: PpocrConfig,         // new
    pub gpt4_vision: Gpt4VisionConfig, // future
}
```

### Instance Management Extensions

**PPOCR Instance Pooling**
```rust
// src/ppocr_instance_manager.rs
pub struct PpocrInstanceManager {
    instances: Mutex<HashMap<String, PpocrInstance>>,
    max_instances: usize,
    config: PpocrConfig,
}

struct PpocrInstance {
    process: std::process::Child,
    last_used: Instant,
    language: String,
}

impl PpocrInstanceManager {
    pub async fn get_instance(&self, language: &str) -> Result<PpocrInstanceHandle> {
        // Reuse or create Python process
        // Maintain pool of warm processes
    }
}
```

## Implementation Phases

### Phase 1: Core Integration (1-2 weeks)

**1. Create Provider Abstraction**
- Add `OcrProvider` trait
- Implement provider manager
- Update configuration structures

**2. Implement PPOCR Provider**
- Create `PpocrProvider` struct
- Implement Python subprocess communication
- Add error handling and timeouts

**3. Configuration Updates**
- Extend `OcrConfig` for PPOCR settings
- Add environment variable support
- Update config validation

**4. Basic Testing**
- Unit tests for PPOCR provider
- Integration tests with sample images
- Performance benchmarks vs Tesseract

### Phase 2: Advanced Features (1 week)

**1. Instance Pooling**
- Implement process reuse for PPOCR
- Add warm-up strategies
- Memory management

**2. Error Recovery**
- Circuit breaker integration
- Retry logic with backoff
- Fallback to Tesseract

**3. Monitoring & Observability**
- Add PPOCR-specific metrics
- Performance tracking
- Error rate monitoring

### Phase 3: Production Deployment (1 week)

**1. Docker Integration**
- Update Dockerfile for Python/PaddlePaddle
- Multi-stage build optimization
- Model caching in image

**2. Configuration Management**
- Environment-specific configs
- Secret management for API keys (future)
- Feature flags for gradual rollout

**3. A/B Testing Framework**
- Provider selection logic
- Result comparison and scoring
- Gradual traffic migration

## Code Changes Required

### New Files
```
src/ocr_providers.rs              # Provider trait and types
src/ocr_providers/ppocr_provider.rs    # PPOCR implementation
src/ocr_providers/tesseract_provider.rs # Tesseract wrapper
src/ocr_provider_manager.rs       # Provider orchestration
src/ppocr_instance_manager.rs     # Process pooling
```

### Modified Files
```
src/ocr.rs                        # Update extract_text_from_image()
src/ocr_config.rs                 # Add PPOCR config
src/main.rs                       # Initialize provider manager
src/bot/image_processing.rs       # Use provider manager
Cargo.toml                        # Add new dependencies
```

### Dependencies to Add
```toml
# Cargo.toml
[dependencies]
async-trait = "0.1"
serde_json = "1.0"
tokio = { version = "1.0", features = ["process"] }
```

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ppocr_provider_basic() {
        let provider = PpocrProvider::new(&test_config()).unwrap();
        let result = provider.extract_text("test_image.png").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_provider_fallback() {
        let manager = OcrProviderManager::new(test_config());
        // Test PPOCR → Tesseract fallback
    }
}
```

### Integration Tests
- Test with actual recipe images
- Compare accuracy vs Tesseract
- Performance benchmarking
- Memory usage monitoring

### A/B Testing
```rust
// Feature flag for gradual rollout
if feature_enabled("ppocr_rollout") {
    provider_manager.set_primary(PpocrProvider::new()?);
}
```

## Performance Expectations

### Benchmarks (Estimated)
- **Accuracy**: 15-25% improvement over Tesseract
- **Speed**: 50-200ms per image (similar to Tesseract)
- **Memory**: 2-3x Tesseract baseline (4GB+ recommended)
- **Cold Start**: 10-30s for model loading
- **Warm Performance**: Excellent after initialization

### Resource Requirements
- **CPU**: 4+ cores for parallel processing
- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 200-500MB for PPOCR models
- **GPU**: Optional, 2-3x speedup with CUDA

## Deployment Considerations

### Docker Configuration
```dockerfile
# Multi-stage build
FROM python:3.9-slim as ppocr-base
RUN pip install paddlepaddle paddleocr

FROM rust:1.70 as builder
# Build Rust application

FROM debian:bookworm-slim
# Copy Python environment
COPY --from=ppocr-base /usr/local/lib/python3.9 /usr/local/lib/python3.9
# Copy Rust binary
COPY --from=builder /app/target/release/just-ingredients /usr/local/bin/
```

### Environment Variables
```bash
# PPOCR Configuration
PPOCR_ENABLED=true
PPOCR_PYTHON_PATH=/usr/bin/python3
PPOCR_MODEL_CACHE_DIR=/app/models/ppocr
PPOCR_LANGUAGE=en
PPOCR_USE_GPU=false
PPOCR_TIMEOUT_SECONDS=30
```

### Health Checks
- PPOCR process availability
- Model loading status
- Memory usage monitoring
- Performance degradation detection

## Migration Plan

### Gradual Rollout Strategy
1. **Week 1**: Implement core PPOCR provider, internal testing
2. **Week 2**: Add provider manager, configuration, staging deployment
3. **Week 3**: A/B testing with 10% of traffic, performance monitoring
4. **Week 4**: Gradual increase to 50%, then 100% if metrics positive

### Rollback Plan
- Feature flag to disable PPOCR instantly
- Automatic fallback to Tesseract on failures
- Configuration to force Tesseract-only mode

### Monitoring & Alerts
- OCR accuracy metrics comparison
- Processing time tracking
- Error rate monitoring
- Cost tracking (for future API providers)

## Risk Assessment

### Technical Risks
- **Python Dependency**: Adds complexity to Rust-only codebase
- **Process Management**: Subprocess lifecycle management
- **Memory Usage**: Higher resource requirements
- **Model Loading**: Cold start performance impact

### Mitigation Strategies
- **Containerization**: Isolate Python environment
- **Resource Limits**: Set memory/CPU limits
- **Health Monitoring**: Automatic restart on failures
- **Gradual Rollout**: Feature flags for quick rollback

### Business Risks
- **Performance Regression**: Monitor closely during rollout
- **Increased Costs**: Resource usage increase
- **Maintenance Burden**: Additional dependency management

## Success Metrics

### Technical Metrics
- **Accuracy Improvement**: >15% better than Tesseract
- **Processing Time**: <250ms per image
- **Error Rate**: <5% failure rate
- **Memory Usage**: <6GB peak

### Business Metrics
- **User Satisfaction**: Improved ingredient extraction accuracy
- **Processing Success Rate**: >95% of images processed successfully
- **System Reliability**: No increased downtime
- **Cost Efficiency**: Local processing maintains free tier

## Conclusion

PPOCR integration offers significant accuracy improvements for recipe OCR while maintaining local processing. The implementation follows the existing architecture patterns with provider abstraction, circuit breaker integration, and gradual rollout strategy.

**Recommended Approach**: Start with Python subprocess integration for simplicity, then consider ONNX Runtime for pure Rust implementation if performance becomes critical.

**Timeline**: 4 weeks total implementation with phased rollout
**Risk Level**: Medium (technical complexity but well-understood patterns)
**Expected ROI**: 15-25% accuracy improvement with local processing benefits</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/docs/ppocr_implementation_study.md