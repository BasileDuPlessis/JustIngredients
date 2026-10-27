# OCR Processing Feature Specification

> **Superseded.** Photos are read by the Gemini vision API (`src/extraction/`, default
> `EXTRACTION_BACKEND=gemini`); see [LLM Extraction Migration](tasks/llm-extraction-migration.md)
> and [Telegram Bot](telegram-bot.md#photo-flow). This page describes the Tesseract pipeline,
> kept only as the fallback selected with `EXTRACTION_BACKEND=tesseract`. It is not maintained
> and will be deleted in migration Task 5.5.

## Overview
The OCR (Optical Character Recognition) processing feature extracts text from images sent by users through the Telegram bot. It supports multiple image formats, handles ingredient extraction from recipe images, and provides confidence scoring for extracted text.

## Requirements

### Functional Requirements
- **Image Format Support**: PNG, JPEG, BMP, TIFF
- **Text Extraction**: Extract readable text from images using Tesseract OCR
- **Text Cleaning**: Remove OCR artifacts (leading dashes, dots) to improve parsing accuracy
- **Language Support**: English and French text recognition
- **Ingredient Detection**: Parse measurements, units, and ingredient names
- **Confidence Scoring**: Provide reliability metrics for extracted text
- **Error Handling**: Graceful failure with user-friendly messages

### Non-Functional Requirements
- **Performance**: Process images within 30 seconds
- **Accuracy**: Minimum 80% text extraction accuracy, >10% parsing improvement with cleaning
- **Reliability**: Circuit breaker protection against OCR failures
- **Resource Usage**: Memory estimation before processing
- **Scalability**: Instance pooling for concurrent requests

### Success Criteria
- [x] All image formats supported and validated
- [x] Text extraction works for recipe images
- [x] OCR text cleaning removes artifacts and improves parsing accuracy
- [x] Ingredient parsing extracts measurements and names
- [x] Confidence scores provided for all extractions
- [x] Circuit breaker prevents system overload
- [x] Performance meets 30-second SLA

## Architecture

### Component Design
```
OCR Processing Layer
├── Validation Module      # Format, size, path validation
├── Preprocessing Module   # Adaptive pipeline (quality-based strategy selection)
├── Extraction Module      # Core OCR execution + text cleaning
├── Confidence Module      # Scoring and quality assessment
├── HOCR Module           # Advanced text positioning
├── Retry Module          # Exponential backoff with jitter
└── Types Module          # Shared data structures

Preprocessing Layer (src/preprocessing/)
├── scaling.rs            # DPI scaling for OCR optimization
├── filtering.rs          # Noise reduction (Gaussian blur)
├── thresholding.rs       # Adaptive binarization
├── deskewing.rs          # Rotation correction
├── cropping.rs           # Border crop
├── targeted.rs           # Region-specific preprocessing
├── quality.rs            # Image quality assessment (High/Medium/Low)
└── types.rs              # Shared preprocessing data structures
```

### Data Flow
1. **Input Validation**: Check format, size, path security
2. **Preprocessing**: Scale, enhance image quality
3. **OCR Extraction**: Run Tesseract with language settings
4. **Text Cleaning**: Remove leading dashes/dots and OCR artifacts
5. **Text Processing**: Parse ingredients, measurements, units
6. **Confidence Scoring**: Calculate reliability metrics
7. **Result Formatting**: Structure for bot response

### Integration Points
- **Telegram Bot**: Receives images, sends results
- **Database**: Stores OCR history and extracted data
- **Circuit Breaker**: Prevents cascade failures
- **Instance Manager**: Provides OCR engine instances

## API Design

### Public Functions
```rust
// Main entry point (async, returns extracted text + confidence score)
pub async fn extract_text_from_image(
    path: &str,
    config: &OcrConfig,
    instance_manager: &OcrInstanceManager,
    circuit_breaker: &CircuitBreaker,
) -> Result<(String, f32), OcrError>

// Advanced extraction with confidence
pub fn perform_ocr_extraction(image: &DynamicImage, config: &OcrConfig) -> Result<OcrResult, OcrError>

// Text cleaning for OCR artifacts — returns (cleaned_text, artifacts_removed_count)
pub fn clean_ocr_artifacts(text: &str) -> (String, u32)

// Constrained OCR for measurements
pub fn perform_constrained_ocr(image: &DynamicImage, config: &OcrConfig) -> Result<ConstrainedOcrResult, OcrError>

// HOCR extraction for positioning
pub fn extract_hocr_from_image(path: &str, config: &OcrConfig) -> Result<String, OcrError>

// Retry with exponential backoff + jitter
pub fn calculate_retry_delay(attempt: u32, base_ms: u64, max_ms: u64) -> Duration
```

### Error Handling
- **ValidationError**: Invalid input format/size
- **TimeoutError**: Processing exceeded time limits
- **InitializationError**: OCR engine setup failures
- **ExtractionError**: OCR processing failures

### Configuration
```rust
pub struct OcrConfig {
    pub max_file_size: u64,
    pub operation_timeout_secs: u64,
    pub base_retry_delay_ms: u64,
    pub max_retry_delay_ms: u64,
    pub format_limits: FormatLimits,
    pub recovery: RecoveryConfig,
}
```

## Implementation Notes

### Key Decisions
- **Tesseract Backend**: Industry-standard OCR engine
- **Text Cleaning**: Remove leading dashes/dots to improve parsing accuracy by >10%
- **Instance Pooling**: Reuse OCR instances for performance
- **Circuit Breaker**: Fail-fast protection against OCR issues
- **Confidence Scoring**: Multi-factor reliability assessment
- **Language Support**: English/French for ingredient recognition

### Constraints
- **Memory Limits**: Pre-calculate usage before processing
- **Time Limits**: 30-second maximum processing time
- **File Size Limits**: Format-specific size restrictions
- **Thread Safety**: Shared state protected with mutexes

### Dependencies
- **leptess**: Tesseract Rust bindings
- **image**: Format detection and processing
- **tokio**: Async processing support
- **tracing**: Logging and monitoring

## Testing Strategy

### Unit Tests
- Individual module functionality
- Error condition handling
- Configuration validation

### Integration Tests
- End-to-end OCR processing
- Database storage of results
- Circuit breaker behavior

### Performance Tests
- Processing time validation
- Memory usage monitoring
- Concurrent request handling

## Monitoring & Observability

### Metrics
- OCR success/failure rates
- Text cleaning effectiveness (artifacts removed)
- Processing time distribution
- Memory usage patterns
- Circuit breaker state

### Logging
- Processing start/completion
- Error conditions with context
- Performance metrics
- Instance usage statistics

## Future Enhancements

### Potential Improvements
- **Multi-language expansion**: Additional language support beyond EN/FR
- **Batch processing**: Multiple image processing in a single request
- **Real-time feedback**: Processing progress updates during long extraction
- **Model optimization**: Custom OCR model training for recipe-specific text