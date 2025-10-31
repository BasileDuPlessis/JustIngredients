# Callback Handler Refactoring Plan

## Current State Analysis

The `callback_handler.rs` file has grown to **2,261 lines** with **20+ functions** handling various callback operations. This monolithic structure creates several maintenance challenges:

### Issues Identified
- **Single Responsibility Violation**: One file handles recipe management, ingredient editing, UI workflows, and database operations
- **Complex Parameter Passing**: Large parameter structs (`ReviewIngredientsParams`, `SavedIngredientsParams`) with 8+ fields
- **Tight Coupling**: Direct dependencies on database, localization, and UI components throughout
- **Testing Difficulty**: Large functions with multiple responsibilities are hard to unit test
- **Code Navigation**: Developers must scroll through 2000+ lines to find related functionality

### Function Categories
1. **Main Dispatcher**: `callback_handler` - routes callbacks based on state/data
2. **State Handlers**: Handle callbacks for specific dialogue states
   - `handle_review_ingredients_callbacks`
   - `handle_editing_saved_ingredients_callbacks`
3. **Recipe Management**: CRUD operations for recipes
   - `handle_recipe_selection`
   - `handle_recipe_instance_selection`
   - `handle_recipe_action`
   - `handle_delete_recipe_confirmation`
   - `handle_recipe_statistics`
4. **UI Workflow**: Navigation and user flow management
   - `handle_back_to_recipes`
   - `handle_recipes_pagination`
   - `handle_workflow_button`
   - `handle_list_recipes`
5. **Ingredient Operations**: Edit/delete/add ingredients
   - `handle_edit_button`
   - `handle_delete_button`
   - `handle_confirm_button`
   - `handle_add_more_button`
   - `handle_cancel_review_button`
   - `handle_edit_saved_ingredient_button`
   - `handle_delete_saved_ingredient_button`
   - `handle_confirm_saved_ingredients_button`
   - `handle_cancel_saved_ingredients_button`
   - `handle_add_ingredient_button`
6. **Utility Functions**:
   - `handle_edit_ingredients_callback`
   - `callback_handler_with_cache`

## Proposed Modular Structure

### 1. Core Module: `callback_dispatcher.rs`
**Purpose**: Main entry point and routing logic
**Responsibilities**:
- `callback_handler()` - main dispatcher function
- Route callbacks to appropriate handlers based on state/data patterns
- Handle general callbacks that work in any state

**Benefits**:
- Clean separation of routing logic
- Easy to understand callback flow
- Minimal dependencies

### 2. Recipe Management Module: `recipe_callbacks.rs`
**Purpose**: Handle all recipe-related CRUD operations
**Functions to include**:
- `handle_recipe_selection()`
- `handle_recipe_instance_selection()`
- `handle_recipe_action()`
- `handle_delete_recipe_confirmation()`
- `handle_recipe_statistics()`

**Benefits**:
- Focused on recipe operations
- Easier to test recipe logic independently
- Clear database transaction boundaries

### 3. Ingredient Editing Module: `ingredient_callbacks.rs`
**Purpose**: Handle ingredient editing workflows
**Functions to include**:
- `handle_edit_button()`
- `handle_delete_button()`
- `handle_confirm_button()`
- `handle_add_more_button()`
- `handle_cancel_review_button()`
- `handle_edit_saved_ingredient_button()`
- `handle_delete_saved_ingredient_button()`
- `handle_confirm_saved_ingredients_button()`
- `handle_cancel_saved_ingredients_button()`
- `handle_add_ingredient_button()`
- `handle_edit_ingredients_callback()`

**Benefits**:
- Isolated ingredient editing logic
- Simplified parameter structs
- Better testability for editing workflows

### 4. UI Workflow Module: `workflow_callbacks.rs`
**Purpose**: Handle navigation and user flow
**Functions to include**:
- `handle_back_to_recipes()`
- `handle_recipes_pagination()`
- `handle_workflow_button()`
- `handle_list_recipes()`

**Benefits**:
- Clear separation of UI flow logic
- Easier to modify user experience
- Independent of business logic

### 5. State Handler Module: `state_callbacks.rs`
**Purpose**: Handle dialogue state-specific routing
**Functions to include**:
- `handle_review_ingredients_callbacks()`
- `handle_editing_saved_ingredients_callbacks()`

**Benefits**:
- Clean state-based routing
- Easy to add new dialogue states
- Reduced complexity in main dispatcher

### 6. Shared Types Module: `callback_types.rs`
**Purpose**: Common types and parameter structs
**Contents**:
- Simplified parameter structs (if still needed)
- Common callback data structures
- Shared enums/types

**Benefits**:
- Reduced code duplication
- Type safety across modules
- Easier refactoring of shared structures

## Implementation Strategy

### Phase 1: Extract Independent Modules (Low Risk)
1. **Create `recipe_callbacks.rs`**
   - Extract recipe management functions
   - Update imports and dependencies
   - Test recipe operations independently

2. **Create `workflow_callbacks.rs`**
   - Extract UI workflow functions
   - Minimal dependencies on other modules
   - Test navigation flows

3. **Create `callback_types.rs`**
   - Extract shared parameter structs
   - Define common interfaces
   - Update all modules to use shared types

### Phase 2: Extract State-Dependent Modules (Medium Risk)
4. **Create `state_callbacks.rs`**
   - Extract state routing functions
   - Depends on ingredient callbacks
   - Test state transitions

5. **Create `ingredient_callbacks.rs`**
   - Extract ingredient editing functions
   - Most complex due to parameter structs
   - Comprehensive testing required

### Phase 3: Core Refactor (High Risk)
6. **Create `callback_dispatcher.rs`**
   - Extract main dispatcher logic
   - Integrate all modules
   - Update main module imports

7. **Update `mod.rs`**
   - Add new module declarations
   - Update public exports
   - Ensure backward compatibility

## Parameter Struct Simplification

### Current Issues
- `ReviewIngredientsParams`: 9 fields, complex lifetime management
- `SavedIngredientsParams`: 8 fields, similar complexity

### Proposed Solutions
1. **Context Struct**: Create a `CallbackContext` struct containing common dependencies
2. **Data Structs**: Separate data-only structs from context-dependent ones
3. **Builder Pattern**: Use builders for complex parameter construction
4. **Result Types**: Return structured results instead of passing mutable references

## Testing Strategy

### Unit Tests
- Test each module independently
- Mock external dependencies (database, bot, dialogue)
- Focus on business logic without side effects

### Integration Tests
- Test module interactions
- End-to-end callback flows
- Database integration tests

### Migration Testing
- Ensure no breaking changes in public API
- All existing functionality preserved
- Performance benchmarks to ensure no degradation

## Benefits of Refactoring

1. **Maintainability**: Smaller, focused modules are easier to understand and modify
2. **Testability**: Individual modules can be tested in isolation
3. **Reusability**: Common functionality can be shared across modules
4. **Performance**: Better code organization may improve compile times
5. **Developer Experience**: Easier navigation and reduced cognitive load
6. **Scalability**: New features can be added to appropriate modules without affecting others

## Risk Mitigation

1. **Incremental Migration**: Extract modules one at a time with full testing
2. **Backward Compatibility**: Maintain existing public API during transition
3. **Comprehensive Testing**: Ensure all functionality works after each phase
4. **Code Reviews**: Review each extracted module for correctness
5. **Performance Monitoring**: Track for any performance regressions

## Success Criteria

- [ ] All 93 existing tests pass
- [ ] Code compiles without warnings
- [ ] No functionality regressions
- [ ] Each module under 500 lines
- [ ] Clear separation of concerns
- [ ] Improved test coverage for extracted modules
- [ ] Documentation updated for new structure

## Timeline Estimate

- **Phase 1**: 2-3 days (independent modules)
- **Phase 2**: 3-4 days (state-dependent modules)
- **Phase 3**: 2-3 days (core integration)
- **Testing & Polish**: 2-3 days
- **Total**: 9-13 days for complete refactoring

## Next Steps

1. Create the first independent module (`recipe_callbacks.rs`)
2. Set up proper testing for the extracted module
3. Continue with remaining modules following the phased approach
4. Update documentation and code comments throughout the process</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/callback_handler_refactoring_plan.md