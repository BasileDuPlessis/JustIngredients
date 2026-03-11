# Task Breakdown: Split `ocr.rs` into Sub-modules

## Context

`src/ocr.rs` is 2,300 lines and contains 7 distinct concerns. The plan is to convert it into a
module directory (`src/ocr/`) with focused sub-modules, keeping `mod.rs` as a thin re-exporter.

### Proposed module layout

```
src/ocr/
  mod.rs              ← thin re-exports only
  types.rs            ← BBox, HocrLine
  validation.rs       ← validate_image_path, validate_image_with_format_limits,
                         estimate_memory_usage, is_supported_image_format
  confidence.rs       ← OcrConfidence, ConfidenceFlag, calculate_* helpers,
                         should_flag_for_review, get_confidence_description
  hocr.rs             ← extract_hocr_from_image, parse_hocr_to_lines,
                         perform_hocr_extraction, html_decode_text, etc.
  constrained.rs      ← perform_constrained_ocr, ConstrainedOcrResult,
                         validate_measurement_line_match
  preprocessing_pipeline.rs ← AdaptivePreprocessingResult, apply_adaptive_preprocessing,
                               apply_image_preprocessing
  extraction.rs       ← extract_text_from_image, perform_ocr_extraction,
                         calculate_retry_delay
```

---

## Tasks

- [ ] **Task 1: Convert `ocr.rs` to a module directory** (30 min)
  - Rename `src/ocr.rs` → `src/ocr/mod.rs` (`mv src/ocr.rs src/ocr/mod.rs` after `mkdir src/ocr`)
  - Verify the project still compiles with no changes to content yet
  - Success criteria: `cargo build` succeeds, all existing tests still pass
  - How to test: `cargo test`

- [ ] **Task 2: Extract `types.rs`** (30 min)
  - Move `BBox` and `HocrLine` structs + their `impl` blocks (lines 45–99) into `src/ocr/types.rs`
  - Add `pub mod types;` and re-export (`pub use types::*;`) in `mod.rs`
  - Remove the moved code from `mod.rs`
  - Success criteria: `BBox` and `HocrLine` are usable from the same public path as before
  - How to test: `cargo test`

- [ ] **Task 3: Extract `validation.rs`** (45 min)
  - Move `validate_image_path`, `validate_image_with_format_limits`, `estimate_memory_usage`,
    `is_supported_image_format` (lines 103–530) into `src/ocr/validation.rs`
  - Update `mod.rs` to declare and re-export
  - Fix any `crate::` path references inside the moved functions
  - Success criteria: All validation-related tests pass; no regressions
  - How to test: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`

- [ ] **Task 4: Extract `confidence.rs`** (45 min)
  - Move `OcrConfidence`, `ConfidenceFlag`, `calculate_ocr_confidence_with_tesseract`,
    `calculate_text_quality_score`, `calculate_pattern_recognition_score`,
    `calculate_processing_time_score`, `should_flag_for_review`, `get_confidence_description`
    (lines 1538–1790) into `src/ocr/confidence.rs`
  - Update `mod.rs` to declare and re-export
  - Success criteria: Confidence types are usable from the same public path as before
  - How to test: `cargo test`

- [ ] **Task 5: Extract `hocr.rs`** (45 min)
  - Move `extract_hocr_from_image`, `perform_hocr_extraction`, `generate_fallback_hocr`,
    `validate_hocr_output`, `parse_hocr_to_lines`, `html_decode_text`, `map_measurement_to_bbox`
    (lines 1790–2150) into `src/ocr/hocr.rs`
  - Update `mod.rs` to declare and re-export
  - Resolve any cross-module imports (e.g. references to `BBox`, `HocrLine` from `types`)
  - Success criteria: HOCR-related tests pass; public API unchanged
  - How to test: `cargo test`

- [ ] **Task 6: Extract `constrained.rs`** (30 min)
  - Move `ConstrainedOcrResult`, `perform_constrained_ocr`, `validate_measurement_line_match`
    (lines 2151–2300) into `src/ocr/constrained.rs`
  - Update `mod.rs` to declare and re-export
  - Success criteria: `cargo build` succeeds; constrained OCR functions remain accessible
  - How to test: `cargo test`

- [ ] **Task 7: Extract `preprocessing_pipeline.rs`** (45 min)
  - Move `AdaptivePreprocessingResult`, `apply_adaptive_preprocessing`, `apply_image_preprocessing`
    (lines 903–1200) into `src/ocr/preprocessing_pipeline.rs`
  - Update `mod.rs` to declare and re-export
  - Verify interaction with the existing `preprocessing/` module is preserved
  - Success criteria: Preprocessing-related tests still pass
  - How to test: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`

- [ ] **Task 8: Extract `extraction.rs` and slim down `mod.rs`** (45 min)
  - Move `extract_text_from_image`, `perform_ocr_extraction`, `calculate_retry_delay`
    (lines 738–903 and 1198–1537) into `src/ocr/extraction.rs`
  - `mod.rs` should now contain only `mod` declarations, `use` re-exports, and the top-level
    doc comment — nothing else
  - Success criteria: `mod.rs` is under ~50 lines; all public symbols still accessible from `crate::ocr`
  - How to test: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`
