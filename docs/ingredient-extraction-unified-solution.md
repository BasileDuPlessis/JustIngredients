# Ingredient Name Extraction: Unified Multi-Word Extraction (Solution 1)

## Overview

**Problem**: Current regex alternation causes inconsistent ingredient name extraction:
- With measurement: `"2g flour"` → captures multi-word names correctly
- Without measurement: `"2 flour bread"` → captures only single word `"flour"`

**Solution**: Modify regex to use unified multi-word extraction by making measurement optional and capturing all remaining text as ingredient name.

**Regex Change**:
```rust
// Current (problematic alternation)
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units})|\s+(?P<ingredient>\w+))

// New (unified extraction)
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units}))?\s*(?P<ingredient>.*)
```

**Key Changes**:
- Remove alternation `|` between measurement and ingredient
- Make measurement optional with `?`
- Capture ingredient as `.*` (all remaining text)

**Expected Behavior**:
- `"2 crème fraîche"` → `quantity: "2", ingredient: "crème fraîche"`
- `"6 pommes de terre"` → `quantity: "6", ingredient: "pommes de terre"`
- `"2g de crème fraîche"` → `quantity: "2", measurement: "g", ingredient: "de crème fraîche"`

## Phase 1: Analysis and Design

#### Task 1.1: Analyze Current Regex Behavior
- [ ] Document current regex pattern and alternation logic
- [ ] Identify all test cases that will be affected by the change
- [ ] Create comprehensive test matrix with before/after behavior
- [ ] Assess impact on existing measurement detection accuracy

**Current Regex Analysis**:
```rust
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units})|\s+(?P<ingredient>\w+))
```

**Alternation Logic**:
- **Path A** (`\s*(?P<measurement>{units})`): Captures measurement, extracts ingredient from remaining text
- **Path B** (`\s+(?P<ingredient>\w+)`): Captures only one word as ingredient

**Test Cases to Validate**:
- `"2 crème fraîche"` (quantity + multi-word, no measurement)
- `"6 pommes de terre"` (quantity + multi-word, no measurement)
- `"2g de chocolat"` (quantity + measurement + preposition + name)
- `"500g chocolat noir"` (quantity + measurement + multi-word name)

#### Task 1.2: Design New Regex Pattern
- [ ] Define the new unified regex pattern with optional measurement
- [ ] Test regex against comprehensive examples
- [ ] Validate that measurement detection still works correctly
- [ ] Ensure ingredient capture includes all remaining text

**New Regex Pattern**:
```rust
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units}))?\s*(?P<ingredient>.*)
```

**Pattern Components**:
- `(?P<quantity>...)`: Quantity capture (unchanged)
- `(?:\s*(?P<measurement>{units}))?`: Optional measurement capture
- `\s*(?P<ingredient>.*)`: All remaining text as ingredient

**Validation Requirements**:
- ✅ Measurement detection accuracy maintained
- ✅ Multi-word ingredients captured completely
- ✅ No regression in existing functionality
- ✅ Handles edge cases (empty text, special characters)

#### Task 1.3: Plan Testing Strategy
- [ ] Identify all affected test files and functions
- [ ] Create test cases for new behavior
- [ ] Plan regression testing for existing functionality
- [ ] Define success criteria and edge case handling

**Test Files to Update**:
- `tests/text_processing_tests.rs`: Core regex tests
- `tests/integration_tests.rs`: End-to-end validation
- `tests/bot_tests.rs`: Bot integration tests

**New Test Cases Needed**:
- Multi-word ingredients without measurement
- French prepositions in ingredient names
- Mixed measurement and multi-word combinations
- Edge cases (empty strings, special characters)

### Phase 2: Core Implementation

#### Task 2.1: Update Regex Pattern
- [ ] Modify the regex pattern in `text_processing.rs`
- [ ] Update pattern compilation and error handling
- [ ] Ensure measurement units configuration remains compatible
- [ ] Test basic pattern compilation

**File Changes**:
- **File**: `src/text_processing.rs`
  - Update `MEASUREMENT_PATTERN` or equivalent regex definition
  - Change from alternation to optional measurement
  - Update any related pattern constants

**Code Location**:
```rust
// Current (approximate)
static MEASUREMENT_PATTERN: &str = r"(?i)(?P<quantity>...)(?:\s*(?P<measurement>{units})|\s+(?P<ingredient>\w+))";

// New
static MEASUREMENT_PATTERN: &str = r"(?i)(?P<quantity>...)(?:\s*(?P<measurement>{units}))?\s*(?P<ingredient>.*)";
```

#### Task 2.2: Update Ingredient Extraction Logic
- [ ] Modify ingredient name extraction to handle new regex output
- [ ] Update text processing functions to work with unified capture
- [ ] Handle cases where ingredient capture might include unwanted text
- [ ] Add post-processing to clean captured ingredient names

**Logic Changes**:
- **Before**: Ingredient extracted differently based on measurement presence
- **After**: Ingredient always captured as `.*` from remaining text
- **Post-processing**: Trim whitespace, handle empty captures

**Functions to Update**:
- `extract_ingredient_measurements()` in `text_processing.rs`
- Any ingredient name cleaning functions
- Measurement match construction logic

#### Task 2.3: Handle Edge Cases
- [ ] Implement logic to stop ingredient capture at next quantity
- [ ] Add validation for overly long ingredient captures
- [ ] Handle cases where `.*` captures too much text
- [ ] Add safeguards against infinite captures

**Edge Case Handling**:
- **Multiple ingredients on line**: `"2 cups flour, 1 cup sugar"`
  - Should extract `"flour"` not `"flour, 1 cup sugar"`
- **Long ingredient names**: Add reasonable length limits
- **Special characters**: Handle punctuation, unicode characters
- **Empty captures**: Fallback behavior when ingredient is empty

### Phase 3: Integration and Testing

#### Task 3.1: Update Unit Tests
- [ ] Update existing regex tests in `text_processing_tests.rs`
- [ ] Add new test cases for multi-word ingredients
- [ ] Test measurement detection accuracy
- [ ] Validate ingredient extraction consistency

**Test Updates Needed**:
```rust
// New test cases
#[test]
fn test_multi_word_ingredients_without_measurement() {
    // "2 crème fraîche" -> ingredient: "crème fraîche"
    // "6 pommes de terre" -> ingredient: "pommes de terre"
}

#[test]
fn test_unified_extraction_consistency() {
    // Ensure same extraction logic for with/without measurement
}
```

#### Task 3.2: Update Integration Tests
- [ ] Update bot integration tests that depend on ingredient extraction
- [ ] Test end-to-end ingredient processing workflows
- [ ] Validate that recipe creation still works correctly
- [ ] Test with real-world ingredient examples

**Integration Test Updates**:
- `tests/integration_tests.rs`: Mixed recipe processing tests
- `tests/bot_flow_tests.rs`: Bot dialogue tests
- Database integration tests for ingredient storage

#### Task 3.3: Add Regression Tests
- [ ] Ensure existing measurement detection still works
- [ ] Test backward compatibility with existing data
- [ ] Validate that no existing functionality breaks
- [ ] Performance testing for regex changes

**Regression Validation**:
- ✅ All existing tests pass
- ✅ Measurement accuracy maintained
- ✅ No performance degradation
- ✅ Database compatibility preserved

### Phase 4: Validation and Deployment

#### Task 4.1: Manual Testing
- [ ] Test with various ingredient formats (French, English)
- [ ] Validate multi-word ingredient extraction
- [ ] Test edge cases (empty, special characters, long text)
- [ ] Verify bot responses show correct ingredient names

**Manual Test Scenarios**:
- Send photo with `"2 crème fraîche"`
- Send photo with `"6 pommes de terre"`
- Send photo with `"500g chocolat noir"`
- Test editing workflow with multi-word ingredients

#### Task 4.2: Code Quality Checks
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Run `cargo fmt --all -- --check`
- [ ] Execute full test suite: `cargo test`
- [ ] Verify no performance regressions

**Quality Gates**:
- ✅ All 93 tests pass
- ✅ No clippy warnings
- ✅ Code formatting correct
- ✅ Performance within acceptable limits

#### Task 4.3: Deployment Preparation
- [ ] Update deployment scripts if needed
- [ ] Test in staging environment
- [ ] Monitor ingredient extraction accuracy post-deployment
- [ ] Prepare rollback plan if issues arise

**Deployment Checklist**:
- [ ] Database migration compatibility verified
- [ ] Environment variables checked
- [ ] Monitoring alerts configured
- [ ] Rollback procedure documented

## Success Criteria

### Functional Requirements
- [ ] Multi-word ingredients extracted consistently regardless of measurement presence
- [ ] Measurement detection accuracy maintained (no regressions)
- [ ] Ingredient names include all relevant words (no truncation)
- [ ] Handles French prepositions and complex ingredient names

### User Experience Requirements
- [ ] Bot displays complete ingredient names in editing prompts
- [ ] Recipe summaries show full ingredient information
- [ ] No confusion from truncated ingredient names
- [ ] Consistent behavior across all input formats

### Technical Requirements
- [ ] Regex performance acceptable (no significant slowdown)
- [ ] Backward compatibility maintained
- [ ] Comprehensive test coverage for new behavior
- [ ] Code passes all quality checks

## Risk Assessment

### Low Risk
- [ ] Regex compilation failures (caught at build time)
- [ ] Minor performance impact (acceptable for improved accuracy)

### Medium Risk
- [ ] Over-capture of ingredient text (needs proper boundaries)
- [ ] Edge cases with complex ingredient lists (requires testing)

### High Risk
- [ ] Breaking existing ingredient extraction (comprehensive testing required)
- [ ] Performance degradation on large texts (benchmarking needed)

### Mitigation Strategies
- [ ] Comprehensive test suite with real-world examples
- [ ] Gradual rollout with monitoring and rollback capability
- [ ] Performance benchmarking before and after changes
- [ ] Feature flag implementation for safe deployment

## Dependencies

### External Dependencies
- [ ] Regex crate (for pattern compilation)
- [ ] Measurement units configuration (JSON file)

### Internal Dependencies
- [ ] `text_processing.rs`: Core extraction logic
- [ ] `MeasurementMatch` struct: Data structure compatibility
- [ ] Test files: Comprehensive test coverage
- [ ] Localization: Error messages and UI text

## Timeline Estimate

- **Phase 1**: 2-3 hours (analysis and design)
- **Phase 2**: 3-4 hours (core implementation)
- **Phase 3**: 4-5 hours (testing and integration)
- **Phase 4**: 2-3 hours (validation and deployment)

**Total Estimate**: 11-15 hours

## Notes

- This solution provides the most consistent behavior but may capture more text than needed
- Consider implementing boundary detection to stop at next quantity
- May require additional post-processing to clean captured ingredient names
- Comprehensive testing essential due to regex behavior changes
- Monitor performance impact on ingredient extraction operations</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/docs/ingredient-extraction-unified-solution.md