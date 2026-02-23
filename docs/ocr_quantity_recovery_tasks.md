# OCR Quantity Recovery & Fraction Post-Processing: Implementation Tasks

This document outlines the step-by-step implementation plan for the OCR Quantity Recovery feature. The plan is broken down into small, incremental tasks. 

**CURRENT STATUS: Phase 1 (Anomaly Detection & Fallback UI) - 100% Complete**
- ✅ Task 1.1: Data Model Updates - COMPLETED
- ✅ Task 1.2: Anomaly Detection Logic - COMPLETED  
- ✅ Task 1.3: UI Highlighting for Missing Quantities - COMPLETED
- ✅ Task 1.4: Interactive Fallback Flow (Dialogue State & Localization) - COMPLETED
- ✅ Task 1.5: Interactive Fallback Flow (Handlers) - COMPLETED

**CURRENT STATUS: Phase 3 (Targeted Preprocessing & Re-OCR) - 100% Complete**
- ✅ Task 3.1: Image Cropping Logic - COMPLETED
- ✅ Task 3.2: Targeted Preprocessing Pipeline - COMPLETED
- ✅ Task 3.3: Constrained OCR Pass - COMPLETED
- ✅ Task 3.4: Integration & Orchestration - COMPLETED

**CRITICAL REQUIREMENT FOR EVERY TASK:**
After completing the code for each task, the following quality checks MUST be executed and pass before moving to the next task:
1. `cargo fmt --all -- --check` (Formatting)
2. `cargo clippy --all-targets --all-features -- -D warnings` (Linting - NO unwraps allowed)
3. `cargo test` (All tests must pass)
4. update tasks status

---

## Phase 1: Anomaly Detection & Fallback UI (High ROI, Low Effort)

### Task 1.1: Data Model Updates ✅ COMPLETED
- **Goal**: Update the core data structures to support quantity confirmation.
- **Action**: 
  - Add `requires_quantity_confirmation: bool` to the `MeasurementMatch` struct in `src/text_processing.rs`.
  - Update all existing instantiations of `MeasurementMatch` in the codebase and tests to set this to `false` by default.
- **Quality Gate**: Write/update unit tests, run `fmt`, `clippy`, `test`.

### Task 1.2: Anomaly Detection Logic ✅ COMPLETED
- **Goal**: Identify when an extracted ingredient has a missing or absurd quantity.
- **Action**: 
  - Update `MeasurementDetector::extract_ingredient_measurements` to evaluate the parsed quantity.
  - Flag `requires_quantity_confirmation = true` and set quantity to `"0"` if the quantity is empty, missing, or contains suspicious characters (e.g., letters mixed with numbers like "l/2").
- **Quality Gate**: Write unit tests specifically testing the anomaly detection with edge-case strings. Run `fmt`, `clippy`, `test`.

### Task 1.3: UI Highlighting for Missing Quantities ✅ COMPLETED
- **Goal**: Visually indicate to the user that a quantity needs correction.
- **Action**: 
  - Update `src/bot/ui_builder.rs` (specifically `create_ingredient_review_keyboard` or related display functions).
  - If `requires_quantity_confirmation` is true, format the display string with a warning emoji (e.g., `⚠️ 0 cup of flour`).
- **Quality Gate**: Update UI formatting tests. Run `fmt`, `clippy`, `test`.

### Task 1.4: Interactive Fallback Flow (Dialogue State & Localization) ✅ COMPLETED
- **Goal**: Prepare the bot's state machine to handle forced quantity corrections.
- **Action**: 
  - Add a new state to `RecipeDialogueState` (e.g., `AwaitingQuantityCorrection { ingredient_index: usize }`).
  - Add localization keys in `en/main.ftl` and `fr/main.ftl` for the prompt (e.g., *"We couldn't read the exact amount for {ingredient}. Please type the quantity:"*).
- **Quality Gate**: Write state transition tests and localization tests. Run `fmt`, `clippy`, `test`.

### Task 1.5: Interactive Fallback Flow (Handlers) ✅ COMPLETED
- **Goal**: Implement the logic to intercept the workflow and ask the user for the missing quantity.
- **Action**: 
  - Update the post-confirmation workflow in `src/bot/dialogue_manager.rs`. Before saving, check if any ingredient has `requires_quantity_confirmation == true`.
  - If true, transition to `AwaitingQuantityCorrection` and send the localized prompt.
  - Implement the message handler to receive the user's input, update the `MeasurementMatch` quantity, set the flag to `false`, and resume the workflow.
- **Quality Gate**: Write integration tests simulating the bot flow for quantity correction. Run `fmt`, `clippy`, `test`.

---

**CURRENT STATUS: Phase 2 (HOCR & Bounding Box Support) - 100% Complete**
- ✅ Task 2.1.1: Investigate HOCR Support in leptess - COMPLETED
- ✅ Task 2.1.2: Add HOCR Function Signature and Structure - COMPLETED
- ✅ Task 2.1.3: Implement Actual HOCR Generation - COMPLETED
- ✅ Task 2.1.4: HOCR Validation and Error Handling - COMPLETED
- ✅ Task 2.2: HOCR Parsing Data Models - COMPLETED
- ✅ Task 2.3: HOCR Parsing Logic - COMPLETED
- ✅ Task 2.4: Mapping Text to Bounding Boxes - COMPLETED

## Phase 2: HOCR & Bounding Box Support (Medium Effort)

### Task 2.1.1: Investigate HOCR Support in leptess ✅ COMPLETED
- **Goal**: Determine how to extract HOCR output from Tesseract via leptess.
- **Action**: 
  - Research leptess crate documentation and API for HOCR output capabilities.
  - Check if leptess supports HOCR format directly or if we need custom parsing.
  - Document findings on HOCR availability and implementation approach.
- **Findings**:
  - ✅ **HOCR Support Available**: leptess provides `get_hocr_text(page: c_int)` method that returns HTML with bounding box attributes
  - ✅ **Direct API Access**: No custom parsing needed - HOCR is natively supported via `LepTess::get_hocr_text(0)`
  - ✅ **Bounding Boxes Also Available**: `get_component_boxes(level, text_only)` provides structured bounding box data
  - ✅ **Multiple Output Formats**: leptess also supports TSV, ALTO XML, and other formats via `get_tsv_text()`, `get_alto_text()`
  - **Implementation Approach**: Use `get_hocr_text(0)` for spatial text data with HTML structure and bounding boxes
- **Quality Gate**: Research completed and documented. Ready to proceed to implementation.

### Task 2.1.2: Add HOCR Function Signature and Structure ✅ COMPLETED
- **Goal**: Create the basic function structure for HOCR extraction.
- **Action**: 
  - Add `extract_hocr_from_image` function signature to `src/ocr.rs`.
  - Implement basic function structure with proper error handling and circuit breaker integration.
  - Add placeholder HOCR generation (simple XML wrapping) for initial testing.
- **Implementation Details**:
  - ✅ Added public `extract_hocr_from_image` async function with proper signature
  - ✅ Integrated circuit breaker fault tolerance and timeout protection
  - ✅ Added placeholder HOCR XML generation with proper HTML structure
  - ✅ Added comprehensive error handling using existing `OcrError` variants
  - ✅ Added function to module exports and tests
- **Quality Gate**: Function compiles, tests pass, fmt and clippy clean. Run `fmt`, `clippy`, `test`.

### Task 2.1.3: Implement Actual HOCR Generation ✅ COMPLETED
- **Goal**: Generate real HOCR output from Tesseract OCR results.
- **Action**: 
  - Replace placeholder implementation with actual leptess HOCR extraction using leptess::get_hocr_text().
  - Configure Tesseract to output HOCR format if supported, or parse existing output.
  - Add proper text cleaning and formatting for HOCR XML structure.
- **Implementation Details**:
  - ✅ Replaced placeholder with `leptess::get_hocr_text(0)` call
  - ✅ Fixed mutable reference requirements for leptess API
  - ✅ Added proper error handling for HOCR extraction failures
  - ✅ Maintained existing circuit breaker and timeout protection
- **Quality Gate**: Function returns valid HOCR XML/HTML. Write tests with sample images. Run `fmt`, `clippy`, `test`.

### Task 2.1.4: HOCR Validation and Error Handling ✅ COMPLETED
- **Goal**: Ensure HOCR output is well-formed and handle edge cases.
- **Action**: 
  - Add XML validation for generated HOCR output.
  - Implement fallback behavior when HOCR generation fails.
  - Add comprehensive error handling for malformed or missing HOCR data.
- **Implementation Details**:
  - ✅ Added `validate_hocr_output()` function with comprehensive XML/HTML validation
  - ✅ Implemented fallback HOCR generation in `generate_fallback_hocr()` when direct HOCR fails
  - ✅ Added validation checks for HTML structure, required elements, and content length
  - ✅ Enhanced `perform_hocr_extraction()` with primary/fallback logic and proper error handling
  - ✅ Added comprehensive test coverage for validation and fallback scenarios
- **Quality Gate**: All HOCR outputs are valid XML. Write edge case tests. Run `fmt`, `clippy`, `test`.

### Task 2.2: HOCR Parsing Data Models ✅ COMPLETED
- **Goal**: Create structures to hold bounding box data.
- **Action**: 
  - Create a `BBox` struct `{ x0: u32, y0: u32, x1: u32, y1: u32 }`.
  - Create a `HocrLine` struct `{ text: String, bbox: BBox }`.
- **Implementation Details**:
  - ✅ Added `BBox` struct with coordinates (x0, y0, x1, y1) and utility methods (width, height, area)
  - ✅ Added `HocrLine` struct combining text content with bounding box
  - ✅ Implemented constructors for both structs (`new()` and `from_coords()`)
  - ✅ Added proper derive macros (Debug, Clone, PartialEq, Eq, Serialize, Deserialize)
  - ✅ Added comprehensive test coverage for creation, methods, and serialization
  - ✅ Exported structs in `lib.rs` for external access
- **Quality Gate**: Write basic instantiation tests. Run `fmt`, `clippy`, `test`.

### Task 2.3: HOCR Parsing Logic ✅ COMPLETED
- **Goal**: Parse the HOCR output to extract lines and their bounding boxes.
- **Action**: 
  - Implement a lightweight parser (using regex or a fast XML parser) to extract `<span class="ocr_line" title="bbox x0 y0 x1 y1">...</span>` elements and their inner text.
- **Implementation Details**:
  - ✅ Added `parse_hocr_to_lines()` function with flexible regex pattern to handle various HOCR formats
  - ✅ Implemented `html_decode_text()` helper for HTML entity decoding and text cleaning
  - ✅ Added robust error handling for malformed coordinates and parsing failures
  - ✅ Exported function in `lib.rs` for external access
  - ✅ Added comprehensive test coverage including edge cases, HTML entities, and complex real-world HOCR structures
  - ✅ Regex pattern supports optional attributes between `class` and `title` for maximum compatibility
- **Quality Gate**: Write unit tests with mock HOCR strings to ensure accurate parsing of text and coordinates. Run `fmt`, `clippy`, `test`.

### Task 2.4: Mapping Text to Bounding Boxes ✅ COMPLETED
- **Goal**: Link the `MeasurementMatch` anomalies to their physical location on the image.
- **Action**: 
  - Implement logic to correlate a `MeasurementMatch` (which has a `line_number` or text content) with the corresponding `HocrLine` to retrieve its `BBox`.
- **Implementation Details**:
  - ✅ Added `map_measurement_to_bbox()` function that correlates measurements with HOCR lines by line number
  - ✅ Implemented 1-based to 0-based line number conversion with bounds checking
  - ✅ Added optional text validation in `validate_measurement_line_match()` for additional confidence
  - ✅ Added comprehensive error handling for out-of-bounds line numbers and validation mismatches
  - ✅ Exported function in `lib.rs` for external access
  - ✅ Added extensive test coverage including edge cases, bounds checking, and text validation
- **Quality Gate**: Write unit tests verifying the mapping logic. Run `fmt`, `clippy`, `test`.

---

## Phase 3: Targeted Preprocessing & Re-OCR (High Effort)

### Task 3.1: Image Cropping Logic ✅ COMPLETED
- **Goal**: Isolate the specific zone where the fraction/quantity should be.
- **Action**: 
  - Implement a function in `src/preprocessing/` that takes an image path and a `BBox`.
  - Calculate the target zone: the left 15-25% of the bounding box width, adding a 5-10 pixel padding.
  - Crop and save/return the new image zone.
- **Implementation Details**:
  - ✅ Added `CroppedImageResult` struct to `src/preprocessing/types.rs` with image, original bbox, cropped region, and processing time
  - ✅ Created `src/preprocessing/cropping.rs` module with `crop_measurement_region()` function
  - ✅ Implemented left 20% width cropping with 7-pixel padding for quantity isolation
  - ✅ Added bounds checking to prevent cropping outside image dimensions
  - ✅ Added comprehensive test coverage with edge cases (bounds, invalid images, small bboxes)
  - ✅ Exported function and types in module system
- **Quality Gate**: Write tests using a dummy image to verify the cropped dimensions are correct. Run `fmt`, `clippy`, `test`.

### Task 3.2: Targeted Preprocessing Pipeline ✅ COMPLETED
- **Goal**: Enhance the cropped zone specifically for small fraction recognition.
- **Action**: 
  - Implement a specialized preprocessing function for the cropped zone: upscale (2x/3x), convert to grayscale, and apply aggressive binarization (Otsu/adaptive thresholding).
- **Implementation Details**:
  - ✅ Added `TargetedPreprocessingResult` struct to `src/preprocessing/types.rs` with image, dimensions, scale factor, threshold, and processing time
  - ✅ Created `src/preprocessing/targeted.rs` module with `preprocess_measurement_region()` function
  - ✅ Implemented 2.5x upscaling using high-quality Catmull-Rom interpolation
  - ✅ Added custom Otsu thresholding algorithm for optimal binarization
  - ✅ Comprehensive test coverage including dimension verification, Otsu calculation, and edge cases
  - ✅ Exported function and types in module system
- **Quality Gate**: Write tests verifying the output image format and dimensions. Run `fmt`, `clippy`, `test`.

### Task 3.3: Constrained OCR Pass ✅ COMPLETED
- **Goal**: Run Tesseract optimized strictly for numbers and fractions.
- **Action**: 
  - Implement a function in `src/ocr.rs` to run OCR on the preprocessed cropped image.
  - Configure Tesseract with `PSM 8` (Single Word) or `PSM 7` (Single Line).
  - Apply a strict character whitelist: `0123456789/½⅓⅔¼¾⅕⅖⅚⅙⅛⅜⅝⅞. `
- **Implementation Details**:
  - ✅ Added `ConstrainedOcrResult` struct to `src/ocr.rs` with text, confidence, PSM mode, whitelist, and processing time
  - ✅ Implemented `perform_constrained_ocr()` async function taking DynamicImage and returning optimized OCR results
  - ✅ Configured Tesseract with PSM 8 (Single Word) for isolated quantity recognition
  - ✅ Applied strict character whitelist containing only numbers, fractions, and decimal points
  - ✅ Added proper error handling and timeout protection matching existing OCR patterns
  - ✅ Comprehensive test coverage including structure validation and Tesseract availability handling
  - ✅ Exported function and result type in module system
- **Quality Gate**: Write tests using a sample cropped fraction image to verify accurate extraction. Run `fmt`, `clippy`, `test`.

### Task 3.4: Integration & Orchestration ✅ COMPLETED
- **Goal**: Tie the automated recovery pipeline together.
- **Action**: 
  - Update the main OCR pipeline:
    1. Run standard OCR.
    2. Detect anomalies.
    3. If anomalies exist, extract HOCR, map to BBox, crop, preprocess, and run constrained OCR.
    4. If constrained OCR yields a valid quantity, update the `MeasurementMatch` and set `requires_quantity_confirmation = false`.
    5. If it still fails, leave `requires_quantity_confirmation = true` (falling back to Phase 1 UI).
- **Implementation Details**:
  - ✅ Added `attempt_automated_quantity_recovery()` async function implementing complete recovery pipeline
  - ✅ Created `process_ingredients_with_recovery()` async function that integrates anomaly detection with automated recovery
  - ✅ Updated `download_and_process_image()` to use the new recovery-enabled processing function
  - ✅ Added validation functions `is_valid_recovered_quantity()` and `is_valid_fraction()` for recovered text
  - ✅ Implemented proper error handling and logging throughout the recovery pipeline
  - ✅ Added comprehensive integration with existing OCR components (HOCR, cropping, preprocessing, constrained OCR)
  - ✅ Maintained backward compatibility - measurements that can't be recovered automatically still require manual confirmation
  - ✅ Added detailed logging and metrics for recovery success/failure tracking
- **Quality Gate**: Write comprehensive end-to-end integration tests simulating both successful automated recovery and fallback to UI. Run `fmt`, `clippy`, `test`.