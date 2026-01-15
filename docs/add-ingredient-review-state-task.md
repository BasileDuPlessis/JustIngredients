# Add Ingredient Feature for ReviewIngredients State

## Overview
Implement the "Add Ingredient" functionality for the `ReviewIngredients` dialogue state to allow users to add ingredients to OCR results before saving them to the database.

## Problem Description
Currently, the "Add Ingredient" button only works when editing existing saved recipes (`EditingSavedIngredients` state). When users are reviewing OCR-extracted ingredients from a new image (`ReviewIngredients` state), the "Add Ingredient" button appears but clicking it does nothing.

## Current Behavior
- "Add Ingredient" button appears in both `ReviewIngredients` and `EditingSavedIngredients` states
- Button only functions in `EditingSavedIngredients` state
- Clicking the button in `ReviewIngredients` state silently fails (no action)

## Desired Behavior
- "Add Ingredient" button should work in both states
- In `ReviewIngredients` state: Allow adding ingredients to the current OCR results before saving
- In `EditingSavedIngredients` state: Allow adding ingredients to existing saved recipes (current behavior)

## Implementation Requirements

### 1. State Management
- [ ] Add new dialogue state: `AddingIngredientToReview` (similar to `AddingIngredientToSavedRecipe`)
- [ ] Update state transition logic in `handle_add_ingredient_button`

### 2. Callback Handler Updates
**File: `src/bot/callbacks/review_callbacks.rs`**
- [ ] Add handling for `"add_ingredient"` callback in `ReviewIngredients` state
- [ ] Implement `handle_add_ingredient_button` function for review state

### 3. Message Handler Updates
**File: `src/bot/message_handler.rs`**
- [ ] Add handling for `AddingIngredientToReview` state in message processing
- [ ] Parse and validate user input for new ingredients
- [ ] Update ingredient list and return to review state

### 4. Dialogue Manager Updates
**File: `src/bot/dialogue_manager.rs`**
- [ ] Add `handle_add_ingredient_input` function for review state (similar to saved recipe version)
- [ ] Implement ingredient parsing and validation
- [ ] Handle state transitions back to `ReviewIngredients`

### 5. UI/UX Considerations
- [ ] Ensure consistent user experience between both states
- [ ] Maintain proper keyboard layouts and messaging
- [ ] Handle cancellation and error cases gracefully

## Technical Details

### State Structure
```rust
AddingIngredientToReview {
    recipe_name: String,
    ingredients: Vec<MeasurementMatch>,
    language_code: Option<String>,
    message_id: Option<i32>,
    extracted_text: String,
    recipe_name_from_caption: Option<String>,
}
```

### Callback Flow
1. User clicks "Add Ingredient" in `ReviewIngredients` state
2. Bot sends prompt message: "Send me the new ingredient (e.g., '2 cups flour' or '3 eggs')"
3. Dialogue transitions to `AddingIngredientToReview` state
4. User sends text message with new ingredient
5. Bot parses ingredient using `parse_ingredient_from_text`
6. If valid: Add to ingredients list and return to `ReviewIngredients` state
7. If invalid: Show error and stay in input state

### Parsing Logic
- Use existing `parse_ingredient_from_text` function
- Validate ingredient format
- Handle quantity parsing (fractions, decimals)
- Support both English and French units
- Add to current `ingredients` vector

## Files to Modify
1. [ ] `src/bot/callbacks/review_callbacks.rs` - Add callback handling
2. [ ] `src/bot/message_handler.rs` - Add message processing for new state
3. [ ] `src/bot/dialogue_manager.rs` - Add input handling logic
4. [ ] `src/dialogue.rs` - Add new dialogue state variant
5. [ ] `src/bot/ui_builder.rs` - Ensure button appears in correct contexts

## Testing Requirements
- [ ] Unit tests for new callback handler
- [ ] Integration tests for complete add ingredient flow
- [ ] Test parsing of various ingredient formats
- [ ] Test error handling for invalid input
- [ ] Test state transitions and UI updates

## Acceptance Criteria
- [ ] "Add Ingredient" button works in `ReviewIngredients` state
- [ ] Users can add ingredients to OCR results before saving
- [ ] Input validation works correctly
- [ ] Error messages are user-friendly and localized
- [ ] State transitions work properly
- [ ] UI remains consistent between both states
- [ ] All existing functionality continues to work
- [ ] Tests pass for new functionality

## Risk Assessment
- **Low Risk**: Reuses existing parsing and validation logic
- **Medium Risk**: State management complexity - ensure proper transitions
- **Low Risk**: UI consistency - follows existing patterns

## Dependencies
- [ ] Existing ingredient parsing functions
- [ ] Current dialogue state management
- [ ] Localization system for messages
- [ ] UI component consistency

## Estimated Effort
- [ ] Implementation: 4-6 hours
- [ ] Testing: 2-3 hours
- [ ] Code review: 1 hour
- [ ] Total: 7-10 hours</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/add-ingredient-review-state-task.md