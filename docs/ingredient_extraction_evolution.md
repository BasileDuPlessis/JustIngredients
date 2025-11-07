# Ingredient Name Extraction Evolution

## Current Behavior Analysis

### Regex Pattern Structure
```rust
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units})|\s+(?P<ingredient>\w+))
```

The problematic alternation: `(?:\s*(?P<measurement>{units})|\s+(?P<ingredient>\w+))`

### Current Logic
- **Path A** (`\s*(?P<measurement>{units})`): When measurement unit exists
  - Captures measurement unit
  - Extracts ingredient from ALL text after the full match
- **Path B** (`\s+(?P<ingredient>\w+)`): When no measurement unit
  - Captures only ONE word (`\w+`) as ingredient name

### Problem Examples

| Input | Current Output | Expected Output | Issue |
|-------|---------------|-----------------|-------|
| `"2 crème fraîche"` | `quantity: "2", ingredient: "crème"` | `quantity: "2", ingredient: "crème fraîche"` | Single word only |
| `"2g de crème fraîche"` | `quantity: "2", measurement: "g", ingredient: "de crème fraîche"` | `quantity: "2", measurement: "g", ingredient: "de crème fraîche"` | Works correctly |
| `"6 pommes de terre"` | `quantity: "6", ingredient: "pommes"` | `quantity: "6", ingredient: "pommes de terre"` | Single word only |
| `"500g de chocolat noir"` | `quantity: "500", measurement: "g", ingredient: "de chocolat noir"` | `quantity: "500", measurement: "g", ingredient: "de chocolat noir"` | Works correctly |

## Desired Behavior

**Consistent multi-word ingredient name extraction regardless of measurement presence:**

- `"2 crème fraîche"` → `quantity: "2", ingredient: "crème fraîche"`
- `"6 pommes de terre"` → `quantity: "6", ingredient: "pommes de terre"`
- `"2g de crème fraîche"` → `quantity: "2", measurement: "g", ingredient: "de crème fraîche"`
- `"500g chocolat noir"` → `quantity: "500", measurement: "g", ingredient: "chocolat noir"`

## Proposed Solutions

### Solution 1: Unified Multi-Word Extraction
**Change the regex to always extract from remaining text:**

```rust
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units}))?\s*(?P<ingredient>.*)
```

- Remove the alternation
- Make measurement optional with `?`
- Capture ingredient as `.*` (all remaining text)
- **Pros**: Consistent behavior, handles multi-word names
- **Cons**: Captures everything after quantity, including unwanted text

### Solution 2: Smart Word Boundary Detection
**Use word boundaries and stop conditions:**

```rust
(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units}))?\s+(?P<ingredient>(?:\w+\s*)+?)(?=\s*(?:\d|\n|$))
```

- Optional measurement
- Multi-word ingredient with `(?:\w+\s*)+?` (non-greedy)
- Lookahead `(?=\s*(?:\d|\n|$))` to stop before next quantity or end
- **Pros**: More precise, stops at logical boundaries
- **Cons**: Complex regex, may miss edge cases

### Solution 3: Two-Pass Approach
**First pass: Detect measurement presence, second pass: Extract ingredient**

1. **Detection Pass**: `(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{units}))?`
2. **Extraction Pass**: Based on measurement presence, extract appropriate text
   - With measurement: text after measurement
   - Without measurement: text after quantity

**Pros**: Most flexible, can implement complex logic
**Cons**: Requires code changes, not just regex

### Solution 4: Context-Aware Extraction
**Use line-based context to determine ingredient boundaries:**

- Parse entire line
- Find quantity
- Extract ingredient as "everything after quantity until next quantity or end"
- Use heuristics to clean up (remove extra whitespace, punctuation)

**Pros**: Most robust, handles complex cases
**Cons**: Requires significant code changes

## Implementation Considerations

### Edge Cases to Handle

1. **Multiple ingredients on same line:**
   ```
   "2 cups flour, 1 cup sugar, 3 eggs"
   ```
   Should extract: `"flour"`, `"sugar"`, `"eggs"`

2. **French preposition handling:**
   ```
   "200g de chocolat noir"
   "2 cuillères à soupe de miel"
   ```
   Should preserve: `"de chocolat noir"`, `"à soupe de miel"`

3. **Mixed measurements:**
   ```
   "1.5 kg pommes de terre nouvelles"
   ```
   Should extract: `"kg"`, `"pommes de terre nouvelles"`

4. **Quantity-only with multi-word:**
   ```
   "6 pommes de terre"
   "4 cuillères à soupe"
   ```
   Should extract: `"pommes de terre"`, `"cuillères à soupe"`

### Performance Impact

- **Current**: Simple regex, fast
- **Solution 1**: Slightly slower (more text captured)
- **Solution 2**: Same performance (regex-only)
- **Solution 3**: Slower (two regex passes)
- **Solution 4**: Slowest (line parsing + heuristics)

### Backward Compatibility

- **Current behavior**: May break existing parsing for quantity-only ingredients
- **Migration**: Need to update tests and possibly database data
- **Testing**: Comprehensive test coverage required

## Recommended Approach

**Solution 3 (Two-Pass Approach)** provides the best balance of:

- ✅ Consistent multi-word ingredient extraction
- ✅ Maintains measurement detection accuracy
- ✅ Flexible enough for complex cases
- ✅ Preserves existing measurement logic
- ✅ Allows for future enhancements

### Implementation Steps

1. **Phase 1**: Implement detection pass to identify measurement presence
2. **Phase 2**: Implement context-aware ingredient extraction
3. **Phase 3**: Update tests and validate against real data
4. **Phase 4**: Deploy with monitoring for edge cases

### Code Structure Changes

```rust
// New method for unified ingredient extraction
fn extract_ingredient_name(line: &str, quantity_end: usize, has_measurement: bool, measurement_end: Option<usize>) -> String {
    // Implementation based on context
}

// Updated extraction logic
let ingredient_name = if has_measurement {
    // Extract from measurement_end to end or next quantity
    extract_ingredient_name(line, quantity_end, true, Some(measurement_end))
} else {
    // Extract from quantity_end to end or next quantity
    extract_ingredient_name(line, quantity_end, false, None)
};
```

This evolution will provide robust, consistent ingredient name extraction across all input formats.</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/ingredient_extraction_evolution.md