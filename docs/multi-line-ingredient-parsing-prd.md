# Multi-Line Ingredient Parsing Feature

## Overview

This PRD describes the implementation of a new feature that enables the JustIngredients Telegram bot to correctly parse and extract ingredients that span multiple lines in OCR-processed text. This addresses a common issue where ingredient names are split across lines due to text wrapping or formatting in the original recipe images.

## Problem Statement

Currently, the bot processes each line of OCR text independently, which causes issues when ingredient names are split across multiple lines. For example:

```
1 cup old-fashioned rolled
oats
```

The current implementation would only extract "old-fashioned rolled" as the ingredient name, missing "oats" which appears on the next line.

## Solution

Implement multi-line ingredient parsing that intelligently combines text from consecutive lines when an ingredient name appears to continue beyond a single line.

### Key Rules

1. **Continuation Detection**: If a line contains a measurement (quantity + unit) but the ingredient name appears incomplete (doesn't end with proper punctuation), continue reading subsequent lines.

2. **Termination Conditions**: Stop reading additional lines when:
   - A new measurement is detected on a subsequent line
   - The current ingredient ends with punctuation indicating completion (period, closing parenthesis)
   - An empty line is encountered

3. **Line Classification**: Lines are classified as:
   - **Measurement lines**: Start with a quantity/unit pattern
   - **Continuation lines**: Don't start with a quantity but contain ingredient text
   - **New ingredient lines**: Start with a new quantity/unit pattern

## User Stories

### Primary User Story
As a user sending recipe images to the bot, I want ingredients that wrap across multiple lines to be correctly parsed as complete ingredient names, so that I get accurate recipe extraction without missing parts of ingredient names.

### Secondary User Stories
- As a user, I want the bot to handle various formatting styles (bullet points, numbered lists, paragraph form) without breaking ingredient parsing.
- As a user, I want consistent behavior regardless of how the OCR engine breaks text into lines.

## Functional Requirements

### FR1: Multi-Line Ingredient Detection
**Given** an ingredient that spans multiple lines
**When** the bot processes the OCR text
**Then** it should combine all relevant lines to form the complete ingredient name

### FR2: Smart Continuation Logic
**Given** a line ending without punctuation
**When** the next line doesn't start with a measurement
**Then** the next line should be considered a continuation of the current ingredient

### FR3: Proper Termination
**Given** an ingredient that ends with punctuation (., ))
**When** processing continuation lines
**Then** stop reading additional lines for that ingredient

### FR4: New Ingredient Detection
**Given** a line that starts with a new measurement
**When** processing continuation lines
**Then** treat it as the start of a new ingredient and stop the current one

## Technical Requirements

### TR1: Backward Compatibility
The feature must maintain full backward compatibility with existing single-line ingredient parsing.

### TR2: Performance
Multi-line parsing should not significantly impact processing performance for recipes with only single-line ingredients.

### TR3: Accuracy
The parsing logic should correctly handle:
- Ingredients with commas in descriptions (e.g., "unsalted butter, cold and cubed")
- Ingredients ending with notes (e.g., "flour (all-purpose)")
- Mixed single-line and multi-line ingredients in the same recipe

### TR4: Error Handling
Gracefully handle edge cases such as:
- Empty lines between ingredients
- Lines with only punctuation
- Very long ingredient names spanning many lines

## Implementation Details

### Core Algorithm

1. **Parse each line for measurements** using existing regex patterns
2. **For each measurement found**, extract the initial ingredient text from the same line
3. **Check for continuation**:
   - If ingredient text appears incomplete (no ending punctuation)
   - And next line doesn't start with a measurement
   - Then read additional lines until termination condition is met
4. **Combine text** from all relevant lines with appropriate spacing
5. **Apply post-processing** (prefix removal, normalization) to the complete ingredient name

### Code Structure

- `extract_multi_line_ingredient()`: Main function for multi-line parsing
- `extract_ingredient_from_line()`: Enhanced to handle commas intelligently
- Modified main processing loop to use multi-line extraction
- Updated position calculation for multi-line matches

### Test Cases

#### Test Case 1: Basic Multi-Line
```
Input:
1 cup old-fashioned rolled
oats

Expected Output:
- Quantity: "1", Measurement: "cup", Ingredient: "old-fashioned rolled oats"
```

#### Test Case 2: Multi-Line with Notes
```
Input:
8 tablespoons unsalted butter, cold and
cubed (See note.)

Expected Output:
- Quantity: "8", Measurement: "tablespoons", Ingredient: "unsalted butter, cold and cubed (See note.)"
```

#### Test Case 3: Mixed Single and Multi-Line
```
Input:
2 cups flour
1 cup old-fashioned rolled
oats
3 eggs

Expected Output:
- "2 cups flour"
- "1 cup old-fashioned rolled oats"
- "3 eggs"
```

## Acceptance Criteria

### AC1: Feature Implementation
- [ ] Multi-line ingredient parsing is implemented
- [ ] All existing tests pass
- [ ] New comprehensive test suite covers multi-line scenarios

### AC2: Quality Assurance
- [ ] Performance impact is minimal (< 10% increase for single-line recipes)
- [ ] No regressions in existing functionality
- [ ] Edge cases are handled gracefully

### AC3: User Experience
- [ ] Users see complete ingredient names in bot responses
- [ ] No breaking changes to existing bot behavior
- [ ] Improved accuracy for complex recipe layouts

## Success Metrics

- **Accuracy**: >95% of multi-line ingredients correctly parsed
- **Performance**: <5% performance degradation for typical recipes
- **User Satisfaction**: Positive feedback on improved parsing accuracy

## Risk Assessment

### High Risk
- **Complex parsing logic** could introduce bugs in edge cases
- **Performance impact** if not implemented efficiently

### Mitigation
- Comprehensive test coverage including edge cases
- Performance benchmarking before and after implementation
- Gradual rollout with monitoring

## Future Enhancements

- Support for more complex formatting (tables, columns)
- Machine learning-based continuation detection
- User feedback loop for continuous improvement

## Dependencies

- Existing OCR and text processing infrastructure
- Regex pattern matching for measurements
- Ingredient post-processing pipeline

## Timeline

- **Week 1**: Design and implement core multi-line parsing logic
- **Week 2**: Testing, bug fixes, and performance optimization
- **Week 3**: Integration testing and documentation
- **Week 4**: Production deployment and monitoring