# OCR Module Size Reduction Strategy

## Version History
- **v1.1** (2026-03-13): Completed Tasks 3 and 4. Added progress tracking and implementation notes.
- **v1.0** (2026-03-13): Initial refactor strategy specification.

## Goal
Reduce the size and complexity of `src/ocr.rs` by splitting responsibilities into focused modules, while preserving behavior and test coverage.

## Why this matters
- Faster onboarding and easier code review.
- Smaller, focused units with clearer ownership.
- Lower regression risk through incremental extraction.
- Better long-term maintainability and testability.

## Target architecture
Keep `src/ocr.rs` as a thin public facade/re-export layer and move implementation into submodules:

- `src/ocr/validation.rs`
  - Path validation, format detection, size limits, memory estimation.
- `src/ocr/retry.rs`
  - Retry policy, exponential backoff, jitter utilities.
- `src/ocr/preprocessing_pipeline.rs`
  - Adaptive preprocessing and temporary file workflow.
- `src/ocr/extraction.rs`
  - Core OCR execution, timeout handling, instance usage.
- `src/ocr/confidence.rs`
  - `OcrConfidence`, flags, scoring functions.
- `src/ocr/hocr.rs`
  - HOCR extraction, validation, parsing, bbox mapping.
- `src/ocr/types.rs`
  - Shared structs (`BBox`, `HocrLine`, constrained OCR/adaptive preprocessing result types).

## Guardrails
- No behavior change unless explicitly planned.
- Keep public API stable (`just_ingredients::ocr::...` should continue to work).
- No `.unwrap()` introduction; keep current error handling standards.
- Every extraction step must keep tests green (`cargo test`, clippy, fmt).
- Integration tests must be executed explicitly during refactor checkpoints:
  - `cargo test --test integration_tests`
  - `cargo test --test observability_integration_tests`

## 1-hour micro-tasks with success criteria

### Task 1 (1h): Baseline and map responsibilities
Work:
- Inventory public functions/types in `src/ocr.rs`.
- Group each symbol by domain (validation, retry, preprocessing, extraction, confidence, HOCR, types).
- Capture current file LOC and number of public symbols.

Success criteria:
- A short mapping table exists below with every public symbol assigned to a target module.
- Baseline metrics are captured (LOC + public symbol count).
- No code changes yet.

#### Public Symbol Mapping Table

| Symbol | Type | Target Module | Notes |
|--------|------|---------------|-------|
| `BBox` | struct | `types.rs` | Bounding box coordinates |
| `HocrLine` | struct | `types.rs` | HOCR line with bbox |
| `AdaptivePreprocessingResult` | struct | `types.rs` | Preprocessing result |
| `ConstrainedOcrResult` | struct | `types.rs` | Constrained OCR result |
| `validate_image_path` | fn | `validation.rs` | Basic path validation |
| `validate_image_with_format_limits` | fn | `validation.rs` | Format-specific validation |
| `estimate_memory_usage` | fn | `validation.rs` | Memory estimation |
| `is_supported_image_format` | fn | `validation.rs` | Format support check |
| `calculate_retry_delay` | fn | `retry.rs` | Exponential backoff |
| `apply_adaptive_preprocessing` | fn | `preprocessing_pipeline.rs` | Adaptive preprocessing |
| `apply_image_preprocessing` | fn | `preprocessing_pipeline.rs` | Temp file preprocessing |
| `extract_text_from_image` | fn | `extraction.rs` | Main OCR entry point |
| `perform_ocr_extraction` | fn | `extraction.rs` | Core extraction logic |
| `perform_constrained_ocr` | fn | `extraction.rs` | Constrained OCR |
| `OcrConfidence` | struct | `confidence.rs` | Confidence scores |
| `ConfidenceFlag` | enum | `confidence.rs` | Confidence flags |
| `calculate_ocr_confidence_with_tesseract` | fn | `confidence.rs` | Confidence calculation |
| `calculate_text_quality_score` | fn | `confidence.rs` | Text quality scoring |
| `calculate_pattern_recognition_score` | fn | `confidence.rs` | Pattern scoring |
| `calculate_processing_time_score` | fn | `confidence.rs` | Time scoring |
| `should_flag_for_review` | fn | `confidence.rs` | Review flag check |
| `get_confidence_description` | fn | `confidence.rs` | Confidence description |
| `extract_hocr_from_image` | fn | `hocr.rs` | HOCR extraction |
| `perform_hocr_extraction` | fn | `hocr.rs` | HOCR processing |
| `generate_fallback_hocr` | fn | `hocr.rs` | Fallback HOCR |
| `validate_hocr_output` | fn | `hocr.rs` | HOCR validation |
| `parse_hocr_to_lines` | fn | `hocr.rs` | HOCR parsing |
| `html_decode_text` | fn | `hocr.rs` | HTML decoding |
| `map_measurement_to_bbox` | fn | `hocr.rs` | Bbox mapping |
| `validate_measurement_line_match` | fn | `hocr.rs` | Line validation |

**Baseline Metrics:**
- **LOC**: 2300 lines (initial)
- **Current LOC**: 2104 lines (after Tasks 3-4, -196 lines total)
- **Public symbols**: 28 (20 functions + 8 types)

### Task 2 (1h): Create module skeletons without moving logic
Work:
- Create folder `src/ocr/` and empty module files.
- Add `src/ocr/mod.rs` wiring with `pub(crate)` and `pub use` stubs.
- Keep existing compile behavior (no functional move yet).

Success criteria:
- Project compiles (`cargo check`).
- New module tree exists with clear names.
- No test regressions introduced by scaffolding.

### Task 3 (1h): ✅ COMPLETED - Extract shared types
Work:
- Move `BBox`, `HocrLine`, `AdaptivePreprocessingResult`, `ConstrainedOcrResult` into `types.rs`.
- Re-export from `mod.rs` (and optionally facade) to preserve old imports.

Success criteria:
- ✅ All previous type imports still compile.
- ✅ `cargo test` passes.
- ✅ `src/ocr.rs` shrinks measurably (types removed).

**Implementation Notes:**
- Created `src/ocr/types.rs` with all four shared types
- Added `mod types;` and `pub use self::types::*;` to `src/ocr.rs`
- Reduced `src/ocr.rs` from 2300 to 2223 lines (77 lines removed)
- All 135 tests pass, no breaking changes to public API

### Task 4 (1h): ✅ COMPLETED - Extract retry utilities
Work:
- Move `calculate_retry_delay` into `retry.rs`.
- Keep function signature stable.

Success criteria:
- ✅ Existing callers unchanged except module path wiring.
- ✅ Retry behavior remains identical (same bounds/jitter logic).
- ✅ `cargo test` passes.

**Implementation Notes:**
- Created `src/ocr/retry.rs` with the complete `calculate_retry_delay` function and documentation
- Added `mod retry;` and `pub use self::retry::*;` to `src/ocr.rs`
- Function remains accessible via same public API path
- Reduced `src/ocr.rs` from 2223 to 2104 lines (119 lines removed, including extensive documentation)
- Exponential backoff and jitter logic preserved exactly

### Task 5 (1h): Extract confidence scoring
Work:
- Move confidence-related types/functions into `confidence.rs`:
  - `OcrConfidence`, `ConfidenceFlag`
  - scoring helpers and review helpers
- Re-export to keep API compatibility.

Success criteria:
- No external call-site breakage.
- Confidence-related tests still pass.
- `src/ocr.rs` reduced by another significant block.

### Task 6 (1h): Extract validation helpers
Work:
- Move image/path/format validation and memory estimation into `validation.rs`.
- Keep error strings and thresholds unchanged.

Success criteria:
- Validation behavior unchanged in tests.
- Functions remain reachable via existing public API.
- `cargo test` passes.

### Task 7 (1h): Extract HOCR helpers
Work:
- Move HOCR extraction and parsing flow into `hocr.rs`:
  - extraction entry point and fallback generation
  - HOCR validation, parse-to-lines, bbox mapping helpers

Success criteria:
- HOCR tests continue to pass.
- No behavioral change in fallback path.
- Facade exports stay backward-compatible.

### Task 8 (1h): Extract preprocessing pipeline
Work:
- Move adaptive preprocessing and temp-file image preparation into `preprocessing_pipeline.rs`.
- Keep same timeout and error-mapping behavior.

Success criteria:
- OCR extraction still uses preprocessed images exactly as before.
- `cargo test` passes.
- Preprocessing logs remain present.

### Task 9 (1h): Extract core extraction orchestration
Work:
- Move `perform_ocr_extraction` and main extraction orchestration internals into `extraction.rs`.
- Keep `extract_text_from_image` as public facade in `mod.rs` or re-exported API.

Success criteria:
- Public API remains stable.
- Circuit breaker + metrics flow unchanged.
- Integration tests pass (`cargo test --test integration_tests` and `cargo test --test observability_integration_tests`).

### Task 10 (1h): Final cleanup and quality gate
Work:
- Reduce `src/ocr.rs` to facade/re-exports + minimal docs.
- Remove dead code/imports.
- Run full quality gate.

Success criteria:
- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test` passes.
- `src/ocr.rs` is significantly smaller (target: <= 25-35% of original LOC).

## Recommended execution order
1. Task 1
2. Task 2
3. Task 3
4. Task 4
5. Task 5
6. Task 6
7. Task 7
8. Task 8
9. Task 9
10. Task 10

## Definition of done
- Behavior unchanged and all tests pass.
- Public OCR API remains backward-compatible.
- `src/ocr.rs` becomes a thin facade instead of a multi-responsibility implementation file.
- Team can assign ownership by module domain.

## Integration verification checklist
Run this at least after Tasks 6, 9, and 10:

1. `cargo test --test integration_tests`
2. `cargo test --test observability_integration_tests`
3. `cargo test` (full suite)
