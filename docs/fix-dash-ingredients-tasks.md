# Fix Ingredient Matching for Lines Starting with Dashes

## Problem Description
The OCR processing fails to correctly match ingredients that start with dash characters (-). Lines like "- 2 cups flour" are not recognized because the regex pattern expects quantities to appear at the beginning of lines, but dashes precede the quantities.

## Root Cause Analysis
- The `MeasurementDetector::extract_ingredient_measurements()` function processes text line-by-line without preprocessing
- The regex pattern `(?i)(?P<quantity>...)` expects quantities at line start (after optional whitespace)
- Lines starting with dashes like "- 2 cups flour" don't match because the dash comes before the quantity
- **Existing behavior**: Bot already merges lines when next line doesn't start with a quantity, which may prevent dash-prefixed lines from being processed individually
- Only dash characters (-) are handled as list markers

## Incremental Fix Tasks

### Phase 1: Core Preprocessing Logic
- [x] **Task 1.1**: Add line preprocessing function to strip leading bullet points
  - Create `strip_leading_bullets(line: &str) -> &str` function
  - Handle dash characters (-) as list markers
  - Preserve whitespace after dashes for proper parsing
  - Add unit tests for dash stripping logic

- [x] **Task 1.2**: Integrate preprocessing into `extract_ingredient_measurements()`
  - Modify the main processing loop to call preprocessing on each line
  - Ensure line numbers and positions are correctly maintained
  - Add logging for preprocessed lines
  - **IMPORTANT**: Account for existing line merging behavior - bot already merges lines when next line doesn't start with a quantity, which may prevent dash-prefixed lines from being processed individually

### Phase 2: Enhanced Pattern Matching
- [x] **Task 2.1**: Update regex pattern to be more flexible with leading content
  - Added `.*?` at the beginning of regex pattern to allow quantities anywhere in the line
  - Updated position calculations to use quantity capture group start position
  - Updated `is_measurement_line()` to check quantity position instead of full match
  - Added comprehensive test `test_quantities_anywhere_in_line()` to verify functionality
  - All existing tests still pass


### Phase 3: Testing and Validation
- [x] **Task 3.1**: Add comprehensive unit tests
  - Updated existing tests to focus on dash (-) handling only
  - Test mixed content (some lines with dash, some without)
  - Test edge cases (empty lines, lines with only dash)
  - Updated `test_strip_leading_bullets` and `test_extract_ingredient_measurements_with_bullets`

- [x] **Task 3.2**: Integration test with `recipe_with_dashes.png`
  - Created `test_dash_prefixed_ingredients_integration()` test
  - Test extracts text from `recipe_with_dashes.png` using OCR
  - Verifies ingredients are correctly parsed from OCR output
  - Confirms dash-prefixed ingredients are handled properly
  - Test passes and validates end-to-end functionality

- [x] **Task 3.3**: Add regression tests
  - Ensure existing functionality still works
  - Test with other test images to prevent breaking changes

### Phase 4: Edge Cases and Polish
- [x] **Task 4.1**: Handle false positives
  - Avoid stripping dashes that are part of ingredient names (e.g., "low-sodium")
  - Only strip leading bullets at line start

- [ ] **Task 4.2**: Performance considerations
  - Ensure preprocessing doesn't significantly impact performance
  - Profile the changes with large texts

- [ ] **Task 4.3**: Documentation updates
  - Update code comments explaining bullet point handling
  - Add examples in documentation

## Success Criteria
- [ ] All ingredients in `recipe_with_dashes.png` are correctly extracted
- [ ] No regression in existing functionality
- [ ] All tests pass (existing + new)
- [ ] Performance impact is minimal (<5% degradation)

## Files to Modify
- `src/text_processing.rs`: Core logic changes
- `tests/text_processing_tests.rs`: Unit tests
- `tests/integration_tests.rs`: Integration tests

## Testing Commands
```bash
# Run all tests
cargo test

# Run specific text processing tests
cargo test text_processing

# Test with the specific image (manual verification)
cargo run --bin test_ocr recipe_with_dashes.png
```</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/fix-dash-ingredients-tasks.md