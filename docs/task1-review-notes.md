# Task 1: Review Current Single-Line Parsing - Notes

## Current Implementation Overview

The `extract_ingredient_measurements()` function in `src/text_processing.rs` processes ingredient text **line-by-line independently**. Here's how it works:

### Core Processing Logic

1. **Line-by-Line Processing**: Iterates through each line of input text using `text.lines().enumerate()`

2. **Regex Pattern Matching**: Uses a dynamically built regex pattern from `config/measurement_units.json` that supports:
   - Named capture groups: `quantity`, `measurement`, `ingredient`
   - Case-insensitive matching with `(?i)`
   - Quantity patterns: integers, decimals, fractions, Unicode fractions (½, ¼, etc.)
   - Measurement units: cups, grams, liters, tablespoons, etc. (English and French)

3. **Single-Line Ingredient Extraction**:
   - Finds measurement matches on each line
   - Extracts ingredient text from the remaining part of the **same line**
   - Stops extraction at: commas, next measurements, or end of line
   - Applies post-processing to clean ingredient names

### Key Regex Pattern Structure

```regex
(?i)(?P<quantity>\d+\s+\d+/\d+|\d+/\d+|\d*\.?\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units_pattern})(?:\s|$))?\s*
```

Where `{units_pattern}` is built from measurement units config with alternation and escaping.

### Current Limitations (Why Multi-Line Parsing is Needed)

- **No Cross-Line Continuation**: If an ingredient name wraps to the next line, only the first line's text is captured
- **Example Problem Case**:
  ```
  Input:
  1 cup old-fashioned rolled
  oats

  Current Output: "old-fashioned rolled" (missing "oats")
  Desired Output: "old-fashioned rolled oats"
  ```

### Measurement Units Configuration

Loaded from `config/measurement_units.json` with categories:
- `volume_units`: cup, cups, teaspoon, etc.
- `weight_units`: g, gram, kg, lb, etc.
- `volume_units_metric`: l, liter, ml, etc.
- `us_units`: fl oz, qt, gal, etc.
- `french_units`: litres, grammes, etc.

### Test Coverage

- **40 tests** in `tests/text_processing_tests.rs`
- All tests currently pass
- Covers various measurement formats, languages, and edge cases
- Includes multi-line text tests, but these test multiple separate ingredients, not wrapped ingredients

### Performance Characteristics

- Processes text efficiently with pre-compiled regex
- Records metrics via `observability::record_text_processing_metrics()`
- Handles large texts with reasonable performance

## Baseline Functionality Confirmed

✅ All 40 text processing tests pass  
✅ No regressions introduced  
✅ Current single-line parsing logic documented and understood  

This establishes the foundation for implementing multi-line ingredient parsing in subsequent tasks.