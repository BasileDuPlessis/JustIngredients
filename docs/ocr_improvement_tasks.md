# OCR Accuracy Improvement Tasks

## Overview
This document outlines a simplified, phased approach to improving Tesseract OCR accuracy for the Just Ingredients bot. The strategy prioritizes **image processing improvements first** (quick wins with immediate impact) before moving to **model training and fine-tuning** (longer-term investments).

## Phase 1: Image Processing Foundation (Weeks 1-8)

### Task 1.1: Create ImageScaler Structure (Week 1, Days 1-2)
**Objective**: Create the basic ImageScaler struct and core functionality.

**Requirements:**
- Create new `preprocessing.rs` module
- Implement `ImageScaler` struct with target height configuration
- Add basic scaling method with cubic interpolation
- Include proper error handling

**Implementation Steps:**
1. Create `src/preprocessing.rs` with module documentation
2. Implement `ImageScaler` struct with `target_char_height` field
3. Add constructor methods (`new()`, `with_target_height()`)
4. Implement basic `scale()` method using cubic interpolation
5. Add input validation for target height (20-35 pixels)

**Success Criteria:**
- [x] `preprocessing.rs` module compiles without errors
- [x] `ImageScaler` struct can be created with default and custom heights
- [x] Basic scaling method processes images without crashing
- [x] Input validation prevents invalid target heights
- [x] Unit tests pass for basic functionality

**Testing:**
- Test struct creation and configuration
- Test basic scaling with small test images
- Verify cubic interpolation is applied

---

### Task 1.2: Implement OCR-Optimized Scaling Logic (Week 1, Days 3-4)
**Objective**: Add intelligent scaling logic optimized for OCR text recognition.

**Requirements:**
- Implement text height estimation algorithm
- Add scale factor calculation based on estimated text size
- Include safeguards against excessive scaling
- Optimize for recipe image characteristics

**Implementation Steps:**
1. Implement `estimate_text_height()` heuristic method
2. Add `scale_for_ocr()` method that calculates optimal scaling
3. Include scaling limits to prevent excessive image sizes
4. Add logging for scaling operations and performance metrics
5. Optimize for typical recipe image dimensions

**Success Criteria:**
- [ ] Text height estimation provides reasonable values (10-150 pixels)
- [ ] Scale factor calculation works for various image sizes
- [ ] Scaling limits prevent creation of excessively large images
- [ ] Performance logging shows scaling duration
- [ ] Optimized for images 100x100 to 2000x2000 pixels

**Testing:**
- Test with images of various sizes and text densities
- Verify scaling decisions (upscale small text, preserve large text)
- Measure scaling performance and memory usage

---

### Task 1.3: Integrate Scaling into OCR Pipeline (Week 1, Day 5)
**Objective**: Connect the ImageScaler to the existing OCR processing pipeline.

**Requirements:**
- Modify `perform_ocr_extraction()` to use image preprocessing
- Create temporary file handling for processed images
- Maintain backward compatibility with existing error handling
- Preserve original image format when possible

**Implementation Steps:**
1. Add `apply_image_preprocessing()` function to `ocr.rs`
2. Modify `perform_ocr_extraction()` to call preprocessing before OCR
3. Implement temporary file creation and cleanup
4. Handle preprocessing errors gracefully
5. Ensure Tesseract receives properly formatted images

**Success Criteria:**
- [ ] OCR pipeline calls preprocessing before Tesseract
- [ ] Temporary files are created and cleaned up properly
- [ ] Preprocessing errors are converted to appropriate OCR errors
- [ ] Original OCR functionality remains intact
- [ ] No performance regression in OCR processing

**Testing:**
- Test OCR processing with and without scaling enabled
- Verify temporary file cleanup works correctly
- Test error handling for preprocessing failures
- Compare OCR accuracy and performance

---

### Task 1.4: End-to-End Testing and Optimization (Week 1, Day 5)
**Objective**: Test the complete scaling integration and optimize performance.

**Requirements:**
- Test with real recipe images
- Measure accuracy improvements
- Optimize scaling parameters
- Document performance characteristics

**Implementation Steps:**
1. Collect sample recipe images for testing
2. Run OCR accuracy tests with scaling enabled/disabled
3. Measure processing time and memory usage
4. Fine-tune scaling parameters based on results
5. Add comprehensive logging and metrics

**Success Criteria:**
- [ ] Scaling improves OCR accuracy by 5-15% on test images
- [ ] Processing time increase < 200ms per image
- [ ] Memory usage remains reasonable (< 50MB per image)
- [ ] No image quality degradation from scaling
- [ ] Integration doesn't break existing OCR functionality

**Testing:**
- Test with 10-20 diverse recipe images
- Compare OCR results with/without scaling
- Measure performance impact
- Validate on different image formats (PNG, JPEG)

---

### Task 1.2: Otsu Thresholding (Week 2)
**Objective**: Convert images to binary (black/white) for better text segmentation.

**Requirements:**
- Implement Otsu's thresholding algorithm
- Apply to grayscale images before OCR
- Handle both uniform and variable lighting conditions

**Implementation Steps:**
1. Add `apply_otsu_threshold()` function to preprocessing module
2. Convert images to grayscale first
3. Apply Otsu thresholding
4. Integrate into OCR pipeline after scaling

**Success Criteria:**
- [ ] Thresholding produces clean binary images
- [ ] Text remains readable after thresholding
- [ ] Works on images with varying lighting conditions
- [ ] Processing time increase < 100ms per image
- [ ] Improves OCR accuracy by 5-10% on test images

**Testing:**
- Test with 20 images (bright, dark, uneven lighting)
- Compare OCR accuracy before/after thresholding
- Visual inspection of thresholded images

---

### Task 1.3: Gaussian Blur Noise Reduction (Week 3)
**Objective**: Reduce image noise that interferes with OCR.

**Requirements:**
- Apply Gaussian blur with σ = 1.0-1.5
- Reduce salt-and-pepper noise while preserving text edges
- Optimize blur parameters for recipe images

**Implementation Steps:**
1. Add `reduce_noise()` function with Gaussian blur
2. Apply before thresholding in preprocessing pipeline
3. Test different sigma values (1.0, 1.2, 1.5)
4. Choose optimal parameters based on accuracy

**Success Criteria:**
- [ ] Noise is visibly reduced in processed images
- [ ] Text edges remain sharp
- [ ] No blurring of fine text details
- [ ] Improves OCR accuracy by 3-5% on noisy images
- [ ] Processing time increase < 50ms per image

**Testing:**
- Test with 15 noisy recipe images (scans, photos)
- Compare OCR accuracy before/after noise reduction
- Visual inspection for text clarity preservation

---

### Task 1.4: Morphological Operations (Week 4)
**Objective**: Clean up binary images using morphological operations.

**Requirements:**
- Implement erosion/dilation operations
- Remove small noise particles
- Fill small gaps in text
- Use 3x3 kernel for basic operations

**Implementation Steps:**
1. Add morphological operations functions
2. Apply opening operation (erosion + dilation) to remove noise
3. Apply closing operation (dilation + erosion) to fill gaps
4. Integrate after thresholding in pipeline

**Success Criteria:**
- [ ] Small noise particles are removed
- [ ] Text characters remain intact
- [ ] Small gaps in characters are filled
- [ ] Improves OCR accuracy by 2-4% on test images
- [ ] Processing time increase < 30ms per image

**Testing:**
- Test with 15 thresholded images
- Compare OCR accuracy before/after morphological operations
- Visual inspection for text integrity

---

### Task 1.5: Quality Assessment (Week 5)
**Objective**: Assess image quality and adapt preprocessing accordingly.

**Requirements:**
- Implement basic quality metrics (contrast, brightness, sharpness)
- Create adaptive preprocessing pipeline
- Skip or modify preprocessing for high-quality images

**Implementation Steps:**
1. Add `assess_image_quality()` function
2. Calculate basic metrics (contrast ratio, brightness)
3. Create conditional preprocessing logic
4. Apply full pipeline only when needed

**Success Criteria:**
- [ ] Quality assessment runs without errors
- [ ] Correctly identifies high/low quality images
- [ ] Adaptive pipeline improves processing efficiency
- [ ] Overall accuracy improves by 5-8% on mixed quality images
- [ ] High-quality images process faster

**Testing:**
- Test with 30 images of varying quality
- Measure processing time for different quality levels
- Compare accuracy improvements

---

### Task 1.6: Deskewing (Week 6-7)
**Objective**: Correct text rotation for better OCR accuracy.

**Requirements:**
- Detect text line orientation
- Rotate images to horizontal text (±2-3° tolerance)
- Handle small rotations common in photos

**Implementation Steps:**
1. Implement projection profile analysis for skew detection
2. Add rotation correction function
3. Integrate into preprocessing pipeline
4. Test with rotated recipe images

**Success Criteria:**
- [ ] Correctly detects skew in rotated images
- [ ] Rotates images to within 1° of horizontal
- [ ] Improves OCR accuracy by 10-15% on rotated images
- [ ] Processing time increase < 150ms per image
- [ ] Doesn't over-correct straight images

**Testing:**
- Test with 20 rotated images (1° to 10° rotation)
- Compare OCR accuracy before/after deskewing
- Verify straight images remain unchanged

---

### Task 1.7: CLAHE Contrast Enhancement (Week 8)
**Objective**: Improve contrast in low-contrast recipe images.

**Requirements:**
- Implement CLAHE (Contrast Limited Adaptive Histogram Equalization)
- Enhance local contrast while avoiding noise amplification
- Use appropriate clip limit and tile size

**Implementation Steps:**
1. Add CLAHE implementation
2. Apply to images with low contrast
3. Integrate into adaptive preprocessing pipeline
4. Test on various contrast levels

**Success Criteria:**
- [ ] Low-contrast images show improved text visibility
- [ ] No excessive noise amplification
- [ ] Improves OCR accuracy by 5-10% on low-contrast images
- [ ] Processing time increase < 100ms per image
- [ ] High-contrast images remain unaffected

**Testing:**
- Test with 25 low-contrast recipe images
- Compare OCR accuracy before/after CLAHE
- Visual inspection for contrast improvement

---

## Phase 2: Model Configuration & Optimization (Weeks 9-12)

### Task 2.1: PSM Mode Optimization (Week 9)
**Objective**: Configure optimal page segmentation modes for recipe content.

**Requirements:**
- Test different PSM modes (3, 4, 6, 8, 13)
- Identify best mode for ingredient lists vs full recipes
- Implement adaptive PSM selection

**Implementation Steps:**
1. Create PSM testing framework
2. Test each mode with recipe images
3. Analyze accuracy results
4. Implement PSM selection logic

**Success Criteria:**
- [ ] PSM 6 works best for ingredient lists
- [ ] PSM 3 works best for full recipe pages
- [ ] Accuracy improves by 5-15% with optimal PSM
- [ ] No processing time degradation

**Testing:**
- Test all PSM modes with 50 diverse recipe images
- Measure accuracy and processing time for each mode
- Establish optimal mode selection rules

---

### Task 2.2: Language Model Optimization (Week 10)
**Objective**: Optimize language model configuration for recipes.

**Requirements:**
- Switch to `tessdata_best` models
- Confirm `eng+fra` configuration works optimally
- Test accuracy improvements with better models

**Implementation Steps:**
1. Update model configuration to use best models
2. Verify eng+fra language combination
3. Test accuracy improvements
4. Monitor for performance impact

**Success Criteria:**
- [ ] Successfully loads tessdata_best models
- [ ] eng+fra configuration works without errors
- [ ] Accuracy improves by 3-8% with better models
- [ ] Memory usage remains acceptable
- [ ] Loading time acceptable

**Testing:**
- Compare accuracy with tessdata_fast vs tessdata_best
- Test with bilingual recipe content
- Measure memory and loading time impact

---

### Task 2.3: Custom Word Lists (Week 11)
**Objective**: Create and integrate custom word lists for recipe terminology.

**Requirements:**
- Compile list of common ingredients (100-200 words)
- Include measurement units and cooking terms
- Create user_words file for Tesseract
- Test accuracy improvements

**Implementation Steps:**
1. Research and compile ingredient word list
2. Create measurement unit word list
3. Generate user_words.txt file
4. Configure Tesseract to use custom word list
5. Test accuracy improvements

**Success Criteria:**
- [ ] Word list contains 150+ relevant terms
- [ ] Tesseract successfully loads custom word list
- [ ] Accuracy improves by 2-5% on recipe content
- [ ] No false positives from custom words
- [ ] Processing time remains stable

**Testing:**
- Test OCR accuracy with/without custom word list
- Verify common ingredients are recognized better
- Check for any accuracy degradation on non-recipe content

---

### Task 2.4: Character Whitelist (Week 12)
**Objective**: Restrict character set to recipe-relevant characters.

**Requirements:**
- Create whitelist with letters, numbers, fractions, common symbols
- Include accented characters for French recipes
- Test impact on accuracy and false positives

**Implementation Steps:**
1. Define comprehensive character whitelist
2. Configure Tesseract to use whitelist
3. Test on recipe vs non-recipe content
4. Measure accuracy and error reduction

**Success Criteria:**
- [ ] Whitelist includes all necessary characters
- [ ] Reduces false character recognition by 20-30%
- [ ] Improves accuracy on recipe content by 3-7%
- [ ] Doesn't break valid recipe text recognition

**Testing:**
- Compare OCR output with/without whitelist
- Test on recipe images and general text images
- Measure reduction in garbage characters

---

## Phase 3: Advanced Features & Training (Weeks 13-20)

### Task 3.1: Fraction-Specific Improvements (Week 13-14)
**Objective**: Enhance fraction detection and recognition.

**Requirements:**
- Improve Unicode fraction recognition
- Better ASCII fraction parsing
- Context-aware fraction correction
- Mixed number handling

**Implementation Steps:**
1. Enhance fraction detection patterns
2. Improve Unicode fraction recognition
3. Add mixed number parsing
4. Integrate into post-processing pipeline

**Success Criteria:**
- [ ] Unicode fractions (¼, ½, ¾) recognized accurately (>95%)
- [ ] ASCII fractions (1/2, 1/4) parsed correctly (>98%)
- [ ] Mixed numbers (1½, 2¼) handled properly
- [ ] Overall fraction accuracy >90%

**Testing:**
- Test with 100+ images containing fractions
- Measure fraction recognition accuracy
- Compare with baseline performance

---

### Task 3.2: Confidence Scoring (Week 15)
**Objective**: Implement confidence-based result validation.

**Requirements:**
- Extract confidence scores from Tesseract
- Flag low-confidence results for review
- Implement confidence thresholds
- Log confidence statistics

**Implementation Steps:**
1. Add confidence score extraction
2. Implement confidence threshold logic
3. Add confidence-based filtering
4. Integrate confidence tracking into metrics

**Success Criteria:**
- [ ] Confidence scores extracted successfully
- [ ] Low-confidence results flagged appropriately
- [ ] Confidence correlates with accuracy (>80% correlation)
- [ ] No false rejections of valid results

**Testing:**
- Collect confidence scores from 200+ OCR results
- Correlate confidence with manual accuracy assessment
- Tune confidence thresholds for optimal filtering

---

### Task 3.3: Error Correction System (Week 16-17)
**Objective**: Implement intelligent OCR error correction.

**Requirements:**
- Create correction rules for common OCR mistakes
- Context-aware corrections for recipes
- Fuzzy matching for ingredient names
- Unit correction (tbsp → tablespoon)

**Implementation Steps:**
1. Analyze common OCR errors in recipe context
2. Create correction mapping system
3. Implement fuzzy string matching
4. Integrate corrections into post-processing

**Success Criteria:**
- [ ] Corrects 60-80% of common OCR errors
- [ ] Improves ingredient recognition by 10-15%
- [ ] Unit recognition improves by 15-20%
- [ ] No incorrect corrections introduced

**Testing:**
- Collect 500+ OCR errors from test images
- Measure correction accuracy
- Test on held-out validation set

---

### Task 3.4: Training Data Collection (Week 18-19)
**Objective**: Collect and prepare training data for model fine-tuning.

**Requirements:**
- Collect 500+ diverse recipe images
- Create ground truth annotations
- Prepare data in Tesseract training format
- Validate annotation quality

**Implementation Steps:**
1. Set up data collection pipeline
2. Annotate images with accurate text
3. Convert to Tesseract training format
4. Validate annotation quality and consistency

**Success Criteria:**
- [ ] 500+ annotated recipe images collected
- [ ] Ground truth accuracy >99%
- [ ] Training data format correct
- [ ] Diverse image types represented (photos, scans, handwritten)

**Testing:**
- Spot-check 10% of annotations for accuracy
- Validate training data format
- Ensure diversity in collected images

---

### Task 3.5: Model Fine-tuning (Week 20)
**Objective**: Fine-tune Tesseract model on recipe-specific data.

**Requirements:**
- Set up Tesseract training environment
- Fine-tune existing eng+fra model
- Evaluate improvements
- Deploy fine-tuned model

**Implementation Steps:**
1. Set up training environment and tools
2. Fine-tune model with collected data
3. Evaluate accuracy improvements
4. Deploy and test fine-tuned model

**Success Criteria:**
- [ ] Training completes successfully
- [ ] Accuracy improves by 15-25% on recipe content
- [ ] No degradation on general text
- [ ] Model loads and runs without errors

**Testing:**
- Compare fine-tuned model vs baseline on test set
- Measure accuracy improvements by image type
- Validate model stability and performance

---

## Success Metrics & Validation

### Overall Accuracy Targets:
- **Phase 1 End**: 20-30% accuracy improvement on processed images
- **Phase 2 End**: 35-45% accuracy improvement on recipe content
- **Phase 3 End**: 50-60% accuracy improvement with custom model

### Performance Requirements:
- **Processing Time**: < 2 seconds per image (including preprocessing)
- **Memory Usage**: < 200MB per image processing
- **Error Rate**: < 5% complete failures
- **Fraction Accuracy**: > 90% for all fraction types

### Quality Assurance:
- **Regression Testing**: Accuracy doesn't decrease on existing test cases
- **Edge Case Handling**: Works on diverse image types and qualities
- **Scalability**: Performance maintained under load
- **Maintainability**: Code is well-documented and testable

## Risk Mitigation

### Technical Risks:
- **Performance Impact**: Monitor and optimize each preprocessing step
- **Accuracy Regression**: Comprehensive testing before deployment
- **Memory Issues**: Implement streaming processing for large images
- **Training Complexity**: Start with small datasets, scale gradually

### Operational Risks:
- **Downtime**: Implement feature flags for gradual rollout
- **User Impact**: A/B testing for user-facing changes
- **Monitoring**: Comprehensive metrics and alerting
- **Rollback**: Ability to revert changes quickly

## Implementation Notes

### Development Environment:
- Use feature flags for gradual rollout
- Implement comprehensive logging and metrics
- Create automated tests for each component
- Document all configuration parameters

### Testing Strategy:
- Unit tests for individual functions
- Integration tests for preprocessing pipeline
- Accuracy benchmarks with diverse image sets
- Performance regression tests

### Deployment Strategy:
- Gradual rollout with feature flags
- A/B testing for accuracy improvements
- Monitoring dashboards for key metrics
- Quick rollback capabilities