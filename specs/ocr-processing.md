# OCR Processing Feature Specification

## Overview
The OCR (Optical Character Recognition) processing feature extracts text from images sent by users through the Telegram bot. It supports multiple image formats, handles ingredient extraction from recipe images, and provides confidence scoring for extracted text.

## Requirements

### Functional Requirements
- **Image Format Support**: PNG, JPEG, BMP, TIFF
- **Text Extraction**: Extract readable text from images using Tesseract OCR
- **Language Support**: English and French text recognition
- **Ingredient Detection**: Parse measurements, units, and ingredient names
- **Confidence Scoring**: Provide reliability metrics for extracted text
- **Error Handling**: Graceful failure with user-friendly messages

### Non-Functional Requirements
- **Performance**: Process images within 30 seconds
- **Accuracy**: Minimum 80% text extraction accuracy
- **Reliability**: Circuit breaker protection against OCR failures
- **Resource Usage**: Memory estimation before processing
- **Scalability**: Instance pooling for concurrent requests

### Success Criteria
- [ ] All image formats supported and validated
- [ ] Text extraction works for recipe images
- [ ] Ingredient parsing extracts measurements and names
- [ ] Confidence scores provided for all extractions
- [ ] Circuit breaker prevents system overload
- [ ] Performance meets 30-second SLA

## Architecture

### Component Design
```
OCR Processing Layer
├── Validation Module      # Format, size, path validation
├── Preprocessing Module   # Image enhancement, scaling
├── Extraction Module      # Core OCR execution
├── Confidence Module      # Scoring and quality assessment
├── HOCR Module           # Advanced text positioning
└── Types Module          # Shared data structures
```

### Data Flow
1. **Input Validation**: Check format, size, path security
2. **Preprocessing**: Scale, enhance image quality
3. **OCR Extraction**: Run Tesseract with language settings
4. **Text Processing**: Parse ingredients, measurements, units
5. **Confidence Scoring**: Calculate reliability metrics
6. **Result Formatting**: Structure for bot response

### Integration Points
- **Telegram Bot**: Receives images, sends results
- **Database**: Stores OCR history and extracted data
- **Circuit Breaker**: Prevents cascade failures
- **Instance Manager**: Provides OCR engine instances

## API Design

### Public Functions
```rust
// Main entry point
pub fn extract_text_from_image(path: &str, config: &OcrConfig) -> Result<String, OcrError>

// Advanced extraction with confidence
pub fn perform_ocr_extraction(image: &DynamicImage, config: &OcrConfig) -> Result<OcrResult, OcrError>

// Constrained OCR for measurements
pub fn perform_constrained_ocr(image: &DynamicImage, config: &OcrConfig) -> Result<ConstrainedOcrResult, OcrError>

// HOCR extraction for positioning
pub fn extract_hocr_from_image(path: &str, config: &OcrConfig) -> Result<String, OcrError>
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
- **Multi-language expansion**: Additional language support
- **Advanced preprocessing**: Better image quality enhancement
- **Batch processing**: Multiple image processing
- **Real-time feedback**: Processing progress updates
- **Model optimization**: Custom OCR model training