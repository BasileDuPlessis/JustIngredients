# Duplicate Recipe Names - Implementation Plan

## Overview
This document outlines the incremental implementation plan to gracefully handle duplicate recipe names in the JustIngredients Telegram bot. When users create multiple recipes with the same name, the system will provide clear disambiguation UI instead of losing access to recipes.

## Phase 1: Database & Core Infrastructure

### Task 1.1: Add Database Function for Recipe Retrieval by Name
- **Goal**: Create `get_recipes_by_name()` function that returns all recipes with a specific name for a user
- **Implementation**: Add new database function in `src/db.rs`
- **Query**: `SELECT id, telegram_id, content, recipe_name, created_at FROM recipes WHERE telegram_id = $1 AND recipe_name = $2 ORDER BY created_at DESC`
- **Return Type**: `Result<Vec<Recipe>>`

### Task 1.2: Add Helper Function to Check for Duplicates
- **Goal**: Create utility function to determine if a recipe name has duplicates
- **Implementation**: Add `has_duplicate_recipes()` function
- **Logic**: Query count of recipes with same name, return true if > 1

## Phase 2: Recipe Selection Logic

### Task 2.1: Update Recipe Selection Handler
- **Goal**: Modify `handle_recipe_selection()` to handle both single and multiple recipes
- **Logic**:
  - Query all recipes with the selected name
  - If only 1 recipe → show recipe details directly
  - If multiple recipes → show disambiguation UI

### Task 2.2: Create Recipe Disambiguation UI
- **Goal**: Build UI to show multiple recipes with same name
- **Features**:
  - Show recipe name as title
  - List each recipe with creation date/time
  - Show preview of first few ingredients (optional)
  - Use inline keyboard for selection

### Task 2.3: Add Recipe Details Display
- **Goal**: Implement actual recipe viewing (replaces "coming soon" placeholder)
- **Features**:
  - Show recipe name, creation date
  - Display all ingredients in formatted list
  - Add action buttons (edit, delete, back to list)

## Phase 3: UI Components & Localization

### Task 3.1: Update UI Builder Functions
- **Goal**: Extend `ui_builder.rs` with new keyboard functions
- **New Functions**:
  - `create_recipe_instances_keyboard()` - for selecting specific recipe instance
  - `create_recipe_details_keyboard()` - for recipe actions (edit/delete/back)

### Task 3.2: Add Localization Messages
- **Goal**: Add new strings to both English and French localization files
- **New Messages**:
  - `multiple-recipes-found = Found {$count} recipes with this name:`
  - `select-specific-recipe = Select which recipe to view:`
  - `recipe-created = Created: {$date}`
  - `recipe-details-title = 📖 Recipe Details`
  - `recipe-actions = What would you like to do?`
  - `edit-recipe-name = Rename Recipe`
  - `delete-recipe = Delete Recipe`
  - `back-to-recipes = Back to Recipes`

### Task 3.3: Add Recipe Instance Distinction
- **Goal**: Create clear labels for distinguishing duplicate recipes
- **Format**: "Created: {date} • {first_few_ingredients_preview}"
- **Example**: "Created: Dec 15, 2024 • flour, sugar, eggs..."

## Phase 4: Callback Handling

### Task 4.1: Add New Callback Handlers
- **Goal**: Extend callback handler for new recipe interactions
- **New Handlers**:
  - `handle_recipe_instance_selection()` - when user picks specific recipe from duplicates
  - `handle_recipe_details_actions()` - for edit/delete/back actions on recipe details

### Task 4.2: Update Callback Routing
- **Goal**: Add routing logic for new callback data patterns
- **Patterns**:
  - `recipe_instance:{recipe_id}` - for selecting specific recipe instance
  - `recipe_action:{action}:{recipe_id}` - for recipe actions

## Phase 5: Recipe Management Features

### Task 5.1: Implement Recipe Deletion
- **Goal**: Allow users to delete individual recipes
- **Flow**: Confirmation dialog → delete recipe and associated ingredients → return to list
- **Cascade**: Delete ingredients when recipe is deleted

### Task 5.2: Implement Recipe Renaming
- **Goal**: Allow users to rename recipes to resolve duplicates
- **Flow**: Show current name → prompt for new name → validate → update → refresh list
- **Validation**: Same rules as initial recipe naming

### Task 5.3: Add Recipe Statistics
- **Goal**: Show recipe metadata (creation date, ingredient count)
- **Display**: Include in recipe details view

## Phase 6: Testing & Validation

### Task 6.1: Unit Tests
- **Goal**: Test new database functions and logic
- **Coverage**: Duplicate detection, recipe retrieval by name, validation

### Task 6.2: Integration Tests
- **Goal**: Test full user flows with duplicates
- **Scenarios**:
  - Single recipe selection (existing behavior)
  - Multiple recipe disambiguation
  - Recipe details viewing
  - Recipe deletion and renaming

### Task 6.3: Localization Testing
- **Goal**: Ensure all new messages work in both languages
- **Validation**: Test message formatting and pluralization

## Implementation Order Recommendation

1. **Start with Phase 1** - Database functions (low risk, foundational)
2. **Phase 3** - Localization (can be done early, needed for UI)
3. **Phase 2** - Core selection logic (main functionality)
4. **Phase 4** - Callback handling (depends on Phase 2)
5. **Phase 5** - Management features (enhancement)
6. **Phase 6** - Testing (validation)

## Risk Mitigation

- **Incremental**: Each phase can be implemented and tested independently
- **Backward Compatible**: Existing single-recipe behavior unchanged
- **Graceful Degradation**: If new features fail, falls back to current behavior
- **Database Safe**: No schema changes required, only new queries

## Success Criteria

- ✅ Users can create multiple recipes with same name
- ✅ Recipe list shows unique names (current behavior preserved)
- ✅ Selecting unique name shows recipe directly
- ✅ Selecting duplicate name shows disambiguation UI
- ✅ Users can view, rename, and delete individual recipes
- ✅ All functionality works in both English and French
- ✅ Comprehensive test coverage for new features

## Current Behavior Analysis

### What happens now with duplicate names:
- Users can create multiple recipes with identical names ✅
- Recipe list shows unique names only (uses `DISTINCT`) ✅
- Selecting a recipe name shows "Recipe details coming soon!" ❌
- No way to access individual recipes with duplicate names ❌

### Proposed Behavior:
- Maintain current creation and listing behavior ✅
- When selecting a unique recipe name → show details directly ✅
- When selecting a duplicate recipe name → show disambiguation UI ✅
- Allow viewing, editing, and deleting individual recipes ✅

This approach maintains the current user experience for single recipes while gracefully handling the duplicate case with clear, actionable UI.</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/DUPLICATE_RECIPES_IMPLEMENTATION.md