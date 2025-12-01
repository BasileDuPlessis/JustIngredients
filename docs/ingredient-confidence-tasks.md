# Ingredient Confidence Scoring Implementation Tasks

## Overview

This document outlines the tasks to implement confidence scoring for ingredients after regex pattern matching. The system will evaluate how reliable each ingredient extraction is based on multiple factors, providing users with transparency about the quality of OCR results.

## Current State Analysis

**Existing Pipeline:**
1. OCR extracts text from images
2. Regex patterns match potential ingredients
3. Basic validation occurs
4. Results presented to users

**Target Pipeline:**
1. OCR extracts text from images
2. Regex patterns match potential ingredients
3. **NEW:** Confidence scoring evaluates match quality
4. **NEW:** Results categorized by confidence level
5. **NEW:** UI shows confidence indicators
6. Enhanced user experience with actionable feedback

---

## 🔧 Phase 1: Core Confidence Infrastructure

### Task 1.1: Create Confidence Data Structures
- [x] Define `IngredientConfidence` struct with overall_score, pattern_strength, measurement_validity, context_consistency, ocr_quality fields
- [x] Define `ConfidenceLevel` enum (High, Medium, Low, Invalid)
- [x] **Files**: `src/text_processing.rs`, `src/lib.rs`
- [x] **Validation**: Structs compile and can be instantiated

### Task 1.2: Extend MeasurementMatch with Confidence
- [x] Add optional confidence field to existing MeasurementMatch struct
- [x] **Files**: `src/text_processing.rs`
- [x] **Validation**: Existing code still works with confidence as optional

### Task 1.3: Create Confidence Calculator Module
- [x] Create new `src/confidence.rs` module
- [x] Implement `calculate_ingredient_confidence()` function
- [x] Implement `confidence_to_level()` function
- [x] **Validation**: Calculator produces reasonable confidence scores

---

## 🧠 Phase 2: Confidence Factor Implementation

### Task 2.1: Implement Pattern Strength Scoring
- [x] Evaluate how strongly the regex pattern matched
- [x] Consider exact matches, pattern scores, component completeness
- [x] **Files**: `src/confidence.rs`
- [x] **Validation**: Pattern strength correlates with match quality

### Task 2.2: Implement Measurement Validity Checking
- [x] Validate quantities and units make sense (e.g., reject "500kg salt")
- [x] Check unit compatibility with ingredient types
- [x] Implement quantity reasonableness bounds
- [x] **Files**: `src/confidence.rs`
- [x] **Validation**: Correctly identifies unreasonable measurements

### Task 2.3: Implement Context Consistency Analysis
- [x] Check if ingredient fits with recipe context
- [x] Consider recipe type alignment and ingredient category coherence
- [x] Avoid duplicate ingredients
- [x] **Files**: `src/confidence.rs`
- [x] **Validation**: Context consistency improves with recipe understanding

### Task 2.4: Integrate OCR Quality Factor
- [ ] Incorporate base OCR confidence into overall scoring
- [ ] Map OCR confidence to matched text segments
- [ ] **Files**: `src/confidence.rs`
- [ ] **Validation**: OCR quality properly factored into confidence scores

---

## ⚙️ Phase 3: Integration and Configuration

### Task 3.1: Update Ingredient Extraction Pipeline
- [ ] Integrate confidence calculation into existing extraction flow
- [ ] Modify `extract_ingredients_with_confidence()` function
- [ ] **Files**: `src/text_processing.rs`
- [ ] **Validation**: Confidence scores calculated for all ingredient matches

### Task 3.2: Create Confidence Configuration
- [ ] Create `config/confidence.json` with weights and thresholds
- [ ] Implement configuration loading
- [ ] Make confidence factors configurable
- [ ] **Validation**: Configuration loads correctly and affects scoring

### Task 3.3: Add Confidence to Database Schema
- [ ] Add confidence_score, confidence_factors, confidence_level columns
- [ ] Update database insert functions
- [ ] **Files**: `schema.sql`, `src/db.rs`
- [ ] **Validation**: Confidence data stored and retrieved correctly

---

## 🎨 Phase 4: User Interface Updates

### Task 4.1: Create Confidence Indicators
- [ ] Add visual confidence indicators (🟢🟡🔴⚠️) to ingredient display
- [ ] Implement `format_ingredient_with_confidence()` function
- [ ] **Files**: `src/bot/ui_builder.rs`
- [ ] **Validation**: Confidence indicators display correctly in Telegram

### Task 4.2: Implement Bulk Actions for Confidence Levels
- [ ] Allow users to accept/reject ingredients by confidence level
- [ ] Implement bulk accept high confidence handler
- [ ] **Files**: `src/bot/callbacks/workflow_callbacks.rs`
- [ ] **Validation**: Bulk actions work correctly for different confidence levels

### Task 4.3: Add Confidence-Based Grouping
- [ ] Group ingredients by confidence level in UI
- [ ] Implement `group_ingredients_by_confidence()` function
- [ ] **Files**: `src/bot/ui_builder.rs`
- [ ] **Validation**: Ingredients grouped correctly by confidence level

---

## 🧪 Phase 5: Testing and Validation

### Task 5.1: Create Confidence Unit Tests
- [ ] Test confidence calculation logic
- [ ] Test confidence level categorization
- [ ] **Files**: `tests/confidence_tests.rs`
- [ ] **Validation**: All confidence calculation tests pass

### Task 5.2: Update Integration Tests
- [ ] Test confidence integration with full pipeline
- [ ] Verify confidence scores for all ingredients
- [ ] **Files**: `tests/integration_tests.rs`
- [ ] **Validation**: Full pipeline integration works with confidence scoring

### Task 5.3: Add Performance Benchmarks
- [ ] Benchmark confidence calculation performance
- [ ] Ensure <1ms per ingredient calculation
- [ ] **Files**: `tests/performance_tests.rs`
- [ ] **Validation**: Confidence calculation meets performance requirements

---

## 🚀 Phase 6: Deployment and Monitoring

### Task 6.1: Implement Feature Flag
- [ ] Add confidence scoring feature flag
- [ ] Allow gradual rollout of confidence features
- [ ] **Files**: `src/config.rs`
- [ ] **Validation**: Feature can be enabled/disabled without breaking existing functionality

### Task 6.2: Add Confidence Metrics
- [ ] Monitor confidence system performance
- [ ] Track calculation time and confidence distribution
- [ ] **Files**: `src/observability/metrics.rs`
- [ ] **Validation**: Metrics collected and reported correctly

### Task 6.3: Create Rollback Plan
- [ ] Ability to disable confidence features if issues arise
- [ ] Implement rollback functionality
- [ ] **Files**: `src/config.rs`
- [ ] **Validation**: Rollback restores system to pre-confidence state

---

## 📊 Success Metrics

### Technical Metrics
- [ ] **Processing Time**: <1.2x increase in total processing time
- [ ] **Accuracy**: >85% correlation between confidence scores and human judgment
- [ ] **False Positive Rate**: <5% of high-confidence ingredients need correction
- [ ] **Performance**: <500μs average confidence calculation time

### User Experience Metrics
- [ ] **User Satisfaction**: >80% positive feedback on confidence indicators
- [ ] **Task Completion**: >90% of recipes completed without manual corrections for high-confidence ingredients
- [ ] **Error Reduction**: >50% reduction in user-reported ingredient extraction errors

### Quality Metrics
- [ ] **Test Coverage**: >90% test coverage for confidence module
- [ ] **Integration Tests**: All confidence integration tests passing
- [ ] **Performance Tests**: All performance benchmarks met

---

## 📋 Implementation Checklist

### Pre-Implementation
- [ ] Review existing ingredient extraction code
- [ ] Understand current MeasurementMatch structure
- [ ] Analyze existing regex patterns and their reliability

### Development Phases
- [x] Phase 1: Core confidence infrastructure (Tasks 1.1-1.3)
- [x] Phase 2: Confidence factor implementation (Tasks 2.1-2.4)
- [ ] Phase 3: Integration and configuration (Tasks 3.1-3.3)
- [ ] Phase 4: User interface updates (Tasks 4.1-4.3)
- [ ] Phase 5: Testing and validation (Tasks 5.1-5.3)
- [ ] Phase 6: Deployment and monitoring (Tasks 6.1-6.3)

### Post-Implementation
- [ ] Monitor performance metrics
- [ ] Collect user feedback
- [ ] Adjust confidence thresholds based on real usage
- [ ] Consider additional confidence factors

---

*Document Version: 2.0 - Clean Checkbox Version*
*Date: November 13, 2025*
*Total Tasks: 21*
*Estimated Timeline: 8-10 weeks*
*Status: Ready for Implementation*