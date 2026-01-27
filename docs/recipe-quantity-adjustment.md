# Recipe Quantity Adjustment Feature

## Overview

The Recipe Quantity Adjustment feature allows users to scale all ingredient quantities in a selected recipe by applying a multiplication coefficient. This functionality is essential for adapting recipes to different serving sizes, dietary needs, or personal preferences without manually recalculating each ingredient.

## User Requirements

Users want the ability to:
- Select an existing recipe from their saved recipes
- Apply a uniform multiplication factor to all ingredient quantities
- Preview the adjusted quantities before confirming changes
- Save the adjusted recipe as a new version or overwrite the existing one
- **Automatic coherency preservation**: For unsplittable ingredients (like eggs), have the system find the closest coefficient that maintains whole number quantities

## User Flow

### Step-by-Step Process

1. **Recipe Selection**
   - User navigates to their recipe list
   - Selects a recipe to adjust quantities for

2. **Adjustment Initiation**
   - User selects "Adjust Quantities" or similar option from recipe actions
   - System displays current ingredient quantities

3. **Coefficient Input**
   - User enters a multiplication coefficient (e.g., 1.5, 0.75, 2.0)
   - Input validation ensures positive numeric values
   - Optional: Provide common presets (half, double, triple)

4. **Preview Changes**
   - System calculates and displays adjusted quantities
   - Shows original vs. adjusted amounts for comparison
   - **Automatic Coherency Adjustment**: For unsplittable ingredients, suggests closest coefficient that results in whole numbers
   - Highlights any ingredients that may need special attention

5. **Confirmation and Saving**
   - User reviews adjustments
   - Chooses to save as new recipe or update existing one
   - System persists changes to database

## Technical Considerations

### Data Handling
- Maintain precision for fractional quantities
- Handle different unit types appropriately
- Preserve ingredient names and notes
- Track adjustment history if needed
- **Coherency Preservation**: For unsplittable ingredients (e.g., eggs, whole fruits), automatically find the closest coefficient that results in whole numbers to maintain recipe practicality

### Validation Rules
- Coefficient must be positive number (> 0)
- Prevent extremely large coefficients that could cause overflow
- Validate that adjusted quantities remain meaningful
- **Coherency Check**: For recipes containing unsplittable ingredients, suggest or automatically adjust coefficient to nearest value that produces whole numbers

### User Interface
- Intuitive coefficient input (numeric keyboard in Telegram)
- Clear display of before/after quantities
- Easy cancellation and retry options

## Benefits

- **Convenience**: Quickly scale recipes without manual calculation
- **Flexibility**: Adapt recipes for different group sizes
- **Precision**: Maintains accuracy of ingredient ratios
- **Time-saving**: Eliminates error-prone manual adjustments

## Edge Cases

- Recipes with zero quantities
- Mixed measurement units
- Fractional coefficients
- Very large or very small coefficients
- Recipes with complex ingredient formats
- **Unsplittable Ingredients**: Recipes containing items like eggs, whole fruits, or vegetables that cannot be meaningfully split - system should find closest coefficient preserving whole number quantities

## Future Enhancements

- Batch adjustment for multiple recipes
- Smart suggestions based on common serving adjustments
- Integration with nutritional information scaling
- Undo/redo functionality for adjustments

## Implementation Tasks

### Phase 1: Core Algorithm Development

1. **Implement Quantity Adjustment Algorithm**
   - Create a function to multiply ingredient quantities by a coefficient
   - Handle different quantity types (integers, decimals, fractions)
   - Preserve quantity precision and formatting

2. **Develop Coherency Preservation Logic**
   - Identify unsplittable ingredients (eggs, whole fruits, etc.)
   - Implement algorithm to find closest coefficient resulting in whole numbers
   - Create ingredient classification system for splittable vs unsplittable items

3. **Add Quantity Validation**
   - Validate coefficient input (positive numbers only)
   - Prevent overflow with extremely large coefficients
   - Ensure adjusted quantities remain meaningful

### Phase 2: Database and Data Layer

4. **Database Schema Updates**
   - Review current ingredient storage structure
   - Add fields for tracking adjustment history if needed
   - Ensure quantity fields support precision requirements

5. **Backend Service Implementation**
   - Create `RecipeAdjustmentService` with quantity scaling methods
   - Implement coherency checking and coefficient suggestion
   - Add methods for saving adjusted recipes (new version vs overwrite)

### Phase 3: User Interface and Dialogue

6. **Telegram Bot UI Components**
   - Add "Adjust Quantities" button to recipe view
   - Create coefficient input interface with numeric keyboard
   - Implement preview display showing before/after quantities

7. **Dialogue State Management**
   - Extend dialogue system to handle quantity adjustment flow
   - Add states for coefficient input, preview, and confirmation
   - Implement navigation between adjustment steps

8. **Localization Updates**
   - Add UI strings for adjustment workflow in English and French
   - Include messages for coherency suggestions and validation errors

### Phase 4: Integration and Testing

9. **Bot Handler Integration**
   - Integrate adjustment commands into existing bot message handlers
   - Add callback handlers for adjustment UI interactions
   - Update command help and usage instructions

10. **Comprehensive Testing**
    - Unit tests for quantity adjustment algorithms
    - Integration tests for coherency preservation logic
    - End-to-end tests for complete adjustment workflow
    - Test edge cases (unsplittable ingredients, mixed units, etc.)

11. **User Experience Refinement**
    - Test adjustment flow with real users
    - Refine UI based on feedback
    - Optimize performance for large recipes

### Phase 5: Documentation and Deployment

12. **Update User Documentation**
    - Add feature documentation to bot help system
    - Update README with adjustment feature details
    - Create user guide for quantity adjustment

13. **Monitoring and Metrics**
    - Add observability for adjustment operations
    - Track usage patterns and success rates
    - Monitor for edge case handling

14. **Production Deployment**
    - Deploy feature to staging environment for testing
    - Gradual rollout with feature flags if needed
    - Monitor for issues and performance impact