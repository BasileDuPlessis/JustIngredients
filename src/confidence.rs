//! Confidence scoring module for ingredient extraction
//!
//! This module provides functionality to calculate confidence scores for ingredient
//! extractions based on multiple factors including pattern strength, measurement validity,
//! context consistency, and OCR quality.

use crate::text_processing::{ConfidenceLevel, IngredientConfidence, MeasurementMatch};

/// Recipe types for context consistency analysis
#[derive(Debug, Clone, PartialEq)]
pub enum RecipeType {
    Dessert,
    MainCourse,
    Salad,
    Soup,
    Breakfast,
    Snack,
    Beverage,
    Unknown,
}

/// Ingredient categories for classification
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IngredientCategory {
    Protein,
    Vegetable,
    Fruit,
    Grain,
    Dairy,
    Spice,
    Herb,
    Oil,
    Sweetener,
    Baking,
    Condiment,
    Beverage,
    Unknown,
}

/// Context information for recipe consistency analysis
#[derive(Debug, Clone)]
pub struct RecipeContext {
    pub recipe_type: RecipeType,
    pub existing_ingredients: Vec<String>,
}

/// Calculate confidence score for an ingredient extraction
///
/// This function evaluates the quality of an ingredient match by analyzing multiple factors:
/// - Pattern strength: How well the regex pattern matched
/// - Measurement validity: Whether the quantity/unit combination is reasonable
/// - Context consistency: How well the ingredient fits the recipe context
/// - OCR quality: Base confidence from the OCR engine
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
/// * `extracted_text` - The full OCR-extracted text for context analysis
/// * `ocr_base_confidence` - Optional base confidence score from OCR engine (0.0-1.0)
/// * `recipe_context` - Optional recipe context for consistency analysis
///
/// # Returns
///
/// Returns an `IngredientConfidence` struct with overall score and individual factor scores
///
/// # Examples
///
/// ```
/// use just_ingredients::confidence::calculate_ingredient_confidence;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let measurement = MeasurementMatch {
///     quantity: "2".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 11,
///     confidence: None,
/// };
///
/// let confidence = calculate_ingredient_confidence(&measurement, "2 cups flour", Some(0.95), None);
/// assert!(confidence.overall_score > 0.7);
/// ```
pub fn calculate_ingredient_confidence(
    measurement: &MeasurementMatch,
    _extracted_text: &str,
    ocr_base_confidence: Option<f32>,
    recipe_context: Option<&RecipeContext>,
) -> IngredientConfidence {
    // Calculate individual confidence factors
    let pattern_strength = calculate_pattern_strength(measurement);
    let measurement_validity = calculate_measurement_validity(measurement);
    let context_consistency = calculate_context_consistency(measurement, recipe_context);
    let ocr_quality = ocr_base_confidence.unwrap_or(0.8); // Default to 0.8 if not provided

    // Calculate weighted overall score
    // Pattern strength: 30%, Measurement validity: 30%, Context: 20%, OCR quality: 20%
    let overall_score = (pattern_strength * 0.3)
        + (measurement_validity * 0.3)
        + (context_consistency * 0.2)
        + (ocr_quality * 0.2);

    IngredientConfidence {
        overall_score,
        pattern_strength,
        measurement_validity,
        context_consistency,
        ocr_quality,
    }
}

/// Convert a confidence score to a confidence level category
///
/// This function categorizes a numerical confidence score (0.0-1.0) into discrete levels
/// that are easier for users to understand and act upon.
///
/// # Confidence Level Thresholds
///
/// - **High** (> 0.8): Auto-accept recommended - extraction is highly reliable
/// - **Medium** (0.5-0.8): Review suggested - extraction is likely correct but should be verified
/// - **Low** (0.3-0.5): Manual correction needed - extraction is uncertain
/// - **Invalid** (< 0.3): Not a valid ingredient - extraction is likely incorrect
///
/// # Arguments
///
/// * `confidence` - The confidence struct containing the overall score
///
/// # Returns
///
/// Returns a `ConfidenceLevel` enum value
///
/// # Examples
///
/// ```
/// use just_ingredients::confidence::confidence_to_level;
/// use just_ingredients::text_processing::{ConfidenceLevel, IngredientConfidence};
///
/// let high_confidence = IngredientConfidence {
///     overall_score: 0.85,
///     pattern_strength: 0.9,
///     measurement_validity: 0.9,
///     context_consistency: 0.8,
///     ocr_quality: 0.8,
/// };
///
/// assert_eq!(confidence_to_level(&high_confidence), ConfidenceLevel::High);
/// ```
pub fn confidence_to_level(confidence: &IngredientConfidence) -> ConfidenceLevel {
    let score = confidence.overall_score;

    if score > 0.8 {
        ConfidenceLevel::High
    } else if score >= 0.5 {
        ConfidenceLevel::Medium
    } else if score >= 0.3 {
        ConfidenceLevel::Low
    } else {
        ConfidenceLevel::Invalid
    }
}

/// Calculate pattern strength score based on match quality
///
/// This function evaluates how well the regex pattern matched the text by considering:
/// - Presence of measurement unit (higher confidence)
/// - Presence of ingredient name (required for valid match)
/// - Quantity format validity (numbers, fractions)
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_pattern_strength(measurement: &MeasurementMatch) -> f32 {
    let mut score: f32 = 0.5; // Base score

    // Strong indicators of good pattern match
    if measurement.measurement.is_some() {
        score += 0.3; // Has explicit measurement unit
    }

    if !measurement.ingredient_name.is_empty() {
        score += 0.2; // Has ingredient name
    } else {
        return 0.1; // No ingredient name is a critical failure
    }

    // Penalize very short ingredient names (likely OCR errors)
    if measurement.ingredient_name.len() < 3 {
        score -= 0.4; // Very short names significantly reduce pattern strength
    }

    // Check quantity format validity
    if is_valid_quantity_format(&measurement.quantity) {
        score += 0.0; // Already covered by base score
    } else {
        score -= 0.2; // Invalid quantity format
    }

    score.clamp(0.0, 1.0)
}

/// Calculate measurement validity score
///
/// This function checks if the quantity and unit combination is reasonable:
/// - Quantity should be within reasonable bounds
/// - Unit should be appropriate for the ingredient type (future enhancement)
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_measurement_validity(measurement: &MeasurementMatch) -> f32 {
    let mut score: f32 = 0.7; // Base score for having a measurement

    // Parse quantity and check reasonableness
    if let Ok(qty) = measurement.quantity.parse::<f32>() {
        // Check if quantity is in a reasonable range
        if qty <= 0.0 {
            return 0.1; // Zero or negative is invalid
        } else if qty > 10000.0 {
            score -= 0.4; // Very large quantities are suspicious
        } else if qty > 1000.0 {
            score -= 0.2; // Large quantities reduce confidence
        } else if qty < 0.01 {
            score -= 0.3; // Very small quantities are suspicious
        }
    } else {
        // Try parsing fractions
        if measurement.quantity.contains('/') {
            let parts: Vec<&str> = measurement.quantity.split('/').collect();
            if parts.len() == 2 {
                if let (Ok(num), Ok(denom)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                    if denom > 0.0 && num / denom > 0.0 && num / denom <= 10000.0 {
                        score += 0.1; // Valid fraction
                    } else {
                        score -= 0.3; // Invalid fraction
                    }
                } else {
                    score -= 0.3; // Cannot parse fraction
                }
            } else {
                score -= 0.3; // Malformed fraction
            }
        } else {
            score -= 0.2; // Cannot parse quantity
        }
    }

    // If no measurement unit, it's a quantity-only ingredient (still valid but lower confidence)
    if measurement.measurement.is_none() {
        score -= 0.1;
    }

    score.clamp(0.0, 1.0)
}

/// Infer recipe type from existing ingredients
pub fn infer_recipe_type(ingredients: &[String]) -> RecipeType {
    if ingredients.is_empty() {
        return RecipeType::Unknown;
    }

    let mut category_counts = std::collections::HashMap::new();

    // Count categories across all ingredients
    for ingredient in ingredients {
        let category = classify_ingredient_category(ingredient);
        *category_counts.entry(category).or_insert(0) += 1;
    }

    let total_ingredients = ingredients.len() as f32;

    // Calculate category percentages
    let sweetener_pct = *category_counts.get(&IngredientCategory::Sweetener).unwrap_or(&0) as f32 / total_ingredients;
    let dairy_pct = *category_counts.get(&IngredientCategory::Dairy).unwrap_or(&0) as f32 / total_ingredients;
    let baking_pct = *category_counts.get(&IngredientCategory::Baking).unwrap_or(&0) as f32 / total_ingredients;
    let vegetable_pct = *category_counts.get(&IngredientCategory::Vegetable).unwrap_or(&0) as f32 / total_ingredients;
    let protein_pct = *category_counts.get(&IngredientCategory::Protein).unwrap_or(&0) as f32 / total_ingredients;

    // Heuristic rules for recipe type classification
    if sweetener_pct > 0.3 || (dairy_pct > 0.2 && baking_pct > 0.1) {
        RecipeType::Dessert
    } else if vegetable_pct > 0.5 {
        RecipeType::Salad
    } else if protein_pct > 0.3 {
        RecipeType::MainCourse
    } else if ingredients.iter().any(|i| i.to_lowercase().contains("soup") || i.to_lowercase().contains("broth")) {
        RecipeType::Soup
    } else if ingredients.iter().any(|i| i.to_lowercase().contains("coffee") || i.to_lowercase().contains("tea")) {
        RecipeType::Beverage
    } else if ingredients.iter().any(|i| i.to_lowercase().contains("cereal") || i.to_lowercase().contains("oat")) {
        RecipeType::Breakfast
    } else {
        RecipeType::Unknown
    }
}

/// Classify an ingredient into a category
pub fn classify_ingredient_category(name: &str) -> IngredientCategory {
    // Load categories from config (in a real implementation, this would be cached)
    // For now, use hardcoded mappings for common ingredients
    let name_lower = name.to_lowercase();

    // Check each category
    if matches_category(&name_lower, &[
        "chicken", "beef", "pork", "fish", "salmon", "tuna", "shrimp", "eggs", "tofu", "tempeh",
        "lentils", "beans", "chickpeas", "turkey", "lamb", "duck", "crab", "lobster", "scallops",
        "bacon", "sausage", "ham", "ground beef", "steak", "ribs", "meatballs", "poultry"
    ]) {
        IngredientCategory::Protein
    } else if matches_category(&name_lower, &[
        "carrot", "broccoli", "spinach", "lettuce", "tomato", "cucumber", "bell pepper", "onion",
        "garlic", "potato", "sweet potato", "zucchini", "eggplant", "mushroom", "celery", "kale",
        "cabbage", "cauliflower", "brussels sprouts", "asparagus", "green beans", "peas", "corn",
        "beets", "radish", "turnip", "parsnip", "squash", "pumpkin", "artichoke", "fennel"
    ]) {
        IngredientCategory::Vegetable
    } else if matches_category(&name_lower, &[
        "apple", "banana", "orange", "lemon", "lime", "strawberry", "blueberry", "raspberry",
        "blackberry", "grape", "pineapple", "mango", "peach", "pear", "plum", "cherry", "kiwi",
        "watermelon", "cantaloupe", "honeydew", "grapefruit", "pomegranate", "avocado"
    ]) {
        IngredientCategory::Fruit
    } else if matches_category(&name_lower, &[
        "rice", "pasta", "bread", "flour", "oats", "quinoa", "barley", "wheat", "couscous",
        "noodles", "spaghetti", "macaroni", "lasagna", "tortilla", "cracker", "cereal", "bran"
    ]) {
        IngredientCategory::Grain
    } else if matches_category(&name_lower, &[
        "milk", "cheese", "yogurt", "butter", "cream", "sour cream", "cottage cheese",
        "mozzarella", "cheddar", "parmesan", "feta", "goat cheese", "ricotta", "cream cheese"
    ]) {
        IngredientCategory::Dairy
    } else if matches_category(&name_lower, &[
        "salt", "pepper", "cumin", "paprika", "cinnamon", "nutmeg", "ginger", "curry powder",
        "chili powder", "garam masala", "turmeric", "coriander", "cardamom", "cloves", "saffron",
        "cayenne", "mustard"
    ]) {
        IngredientCategory::Spice
    } else if matches_category(&name_lower, &[
        "basil", "parsley", "cilantro", "mint", "dill", "chives", "tarragon", "sage", "oregano",
        "thyme", "rosemary", "lavender", "bay leaf", "bay leaves"
    ]) {
        IngredientCategory::Herb
    } else if matches_category(&name_lower, &[
        "olive oil", "vegetable oil", "canola oil", "coconut oil", "sesame oil", "peanut oil",
        "avocado oil", "sunflower oil", "grapeseed oil", "butter", "ghee"
    ]) {
        IngredientCategory::Oil
    } else if matches_category(&name_lower, &[
        "sugar", "brown sugar", "honey", "maple syrup", "agave", "stevia", "sucralose",
        "aspartame", "corn syrup", "molasses", "powdered sugar"
    ]) {
        IngredientCategory::Sweetener
    } else if matches_category(&name_lower, &[
        "baking powder", "baking soda", "yeast", "vanilla extract", "almond extract",
        "cocoa powder", "chocolate chips", "sprinkles", "food coloring", "gelatin"
    ]) {
        IngredientCategory::Baking
    } else if matches_category(&name_lower, &[
        "ketchup", "mustard", "mayonnaise", "soy sauce", "hot sauce", "vinegar", "balsamic",
        "worcestershire", "barbecue sauce", "teriyaki", "salsa", "pesto", "hummus"
    ]) {
        IngredientCategory::Condiment
    } else if matches_category(&name_lower, &[
        "water", "coffee", "tea", "juice", "soda", "wine", "beer", "milk", "cocoa", "broth", "stock"
    ]) {
        IngredientCategory::Beverage
    } else {
        IngredientCategory::Unknown
    }
}

/// Check if an ingredient name matches any in a category list
fn matches_category(name: &str, category_items: &[&str]) -> bool {
    category_items.iter().any(|&item| name.contains(item))
}

/// Check if a category is aligned with a recipe type
fn is_category_aligned(category: IngredientCategory, recipe_type: RecipeType) -> bool {
    match recipe_type {
        RecipeType::Dessert => matches!(
            category,
            IngredientCategory::Sweetener
                | IngredientCategory::Dairy
                | IngredientCategory::Baking
                | IngredientCategory::Fruit
                | IngredientCategory::Grain
        ),
        RecipeType::Salad => matches!(
            category,
            IngredientCategory::Vegetable
                | IngredientCategory::Fruit
                | IngredientCategory::Oil
                | IngredientCategory::Condiment
                | IngredientCategory::Herb
        ),
        RecipeType::Soup => matches!(
            category,
            IngredientCategory::Vegetable
                | IngredientCategory::Protein
                | IngredientCategory::Grain
                | IngredientCategory::Dairy
                | IngredientCategory::Spice
                | IngredientCategory::Herb
        ),
        RecipeType::MainCourse => matches!(
            category,
            IngredientCategory::Protein
                | IngredientCategory::Vegetable
                | IngredientCategory::Grain
                | IngredientCategory::Oil
                | IngredientCategory::Spice
                | IngredientCategory::Herb
                | IngredientCategory::Condiment
        ),
        RecipeType::Breakfast => matches!(
            category,
            IngredientCategory::Grain
                | IngredientCategory::Dairy
                | IngredientCategory::Fruit
                | IngredientCategory::Protein
                | IngredientCategory::Sweetener
        ),
        RecipeType::Beverage => matches!(
            category,
            IngredientCategory::Beverage
                | IngredientCategory::Fruit
                | IngredientCategory::Sweetener
                | IngredientCategory::Spice
                | IngredientCategory::Dairy
        ),
        RecipeType::Snack => matches!(
            category,
            IngredientCategory::Grain
                | IngredientCategory::Fruit
                | IngredientCategory::Dairy
                | IngredientCategory::Sweetener
                | IngredientCategory::Protein
        ),
        RecipeType::Unknown => true, // Allow anything for unknown recipe types
    }
}

/// Calculate category coherence score with existing ingredients
fn calculate_category_coherence(
    new_category: IngredientCategory,
    existing_ingredients: &[String],
) -> f32 {
    if existing_ingredients.is_empty() {
        return 1.0; // No existing ingredients, so fully coherent
    }

    let mut category_counts = std::collections::HashMap::new();

    // Count existing categories
    for ingredient in existing_ingredients {
        let category = classify_ingredient_category(ingredient);
        *category_counts.entry(category).or_insert(0) += 1;
    }

    let total_existing = existing_ingredients.len() as f32;
    let new_category_count = *category_counts.get(&new_category).unwrap_or(&0) as f32;

    // Coherence score based on how common this category is in the recipe
    let category_prevalence = new_category_count / total_existing;

    // Higher score for categories that are already well-represented
    if category_prevalence > 0.3 {
        1.0 // Very coherent - this category dominates the recipe
    } else if category_prevalence > 0.1 {
        0.8 // Moderately coherent
    } else if category_prevalence > 0.0 {
        0.6 // Somewhat coherent - category exists but is minor
    } else {
        0.4 // Low coherence - new category for this recipe
    }
}

/// Check if an ingredient is a duplicate of existing ingredients
fn is_duplicate(new_ingredient: &str, existing_ingredients: &[String]) -> bool {
    let new_lower = new_ingredient.to_lowercase();

    for existing in existing_ingredients {
        let existing_lower = existing.to_lowercase();

        // Exact match
        if new_lower == existing_lower {
            return true;
        }

        // Fuzzy match - check if one contains the other (for variations like "chicken breast" vs "chicken")
        if new_lower.contains(&existing_lower) || existing_lower.contains(&new_lower) {
            return true;
        }

        // Check for common variations
        if are_similar_ingredients(&new_lower, &existing_lower) {
            return true;
        }
    }

    false
}

/// Check if two ingredient names are similar (accounting for common variations)
fn are_similar_ingredients(name1: &str, name2: &str) -> bool {
    // Remove common words and check similarity
    let clean1 = remove_common_words(name1);
    let clean2 = remove_common_words(name2);

    // If they're very short after cleaning, check exact match
    if clean1.len() <= 3 && clean2.len() <= 3 {
        return clean1 == clean2;
    }

    // Check if one is a substring of the other
    clean1.contains(&clean2) || clean2.contains(&clean1)
}

/// Remove common words that don't affect ingredient identity
fn remove_common_words(name: &str) -> String {
    let common_words = ["fresh", "dried", "ground", "chopped", "minced", "sliced", "diced", "grated"];
    let mut result = name.to_string();

    for word in &common_words {
        result = result.replace(&format!(" {} ", word), " ");
        result = result.replace(&format!(" {}", word), "");
        result = result.replace(&format!("{} ", word), "");
    }

    result.trim().to_string()
}

/// Calculate context consistency score for an ingredient for an ingredient
///
/// This function evaluates how well an ingredient fits within the recipe context by:
/// - Checking recipe type alignment (e.g., sugar fits desserts better than savory dishes)
/// - Analyzing ingredient category coherence with existing ingredients
/// - Detecting potential duplicate ingredients
///
/// # Arguments
///
/// * `measurement` - The measurement match to evaluate
/// * `recipe_context` - Optional recipe context information
///
/// # Returns
///
/// Returns a score between 0.0 and 1.0
fn calculate_context_consistency(
    measurement: &MeasurementMatch,
    recipe_context: Option<&RecipeContext>,
) -> f32 {
    // If no recipe context provided, fall back to basic name validation
    let Some(context) = recipe_context else {
        return calculate_basic_name_consistency(measurement);
    };

    let ingredient_name = &measurement.ingredient_name;
    let category = classify_ingredient_category(ingredient_name);

    let mut score = 1.0;

    // Recipe type alignment (40% weight)
    let alignment_score = if is_category_aligned(category.clone(), context.recipe_type.clone()) {
        1.0
    } else {
        0.3 // Penalty for misalignment
    };
    score *= 0.4 * alignment_score + 0.6; // Blend with base score

    // Category coherence with existing ingredients (40% weight)
    let coherence_score = calculate_category_coherence(category, &context.existing_ingredients);
    score *= 0.4 * coherence_score + 0.6;

    // Duplicate detection (20% weight)
    let duplicate_penalty = if is_duplicate(ingredient_name, &context.existing_ingredients) {
        0.3 // Heavy penalty for duplicates
    } else {
        1.0
    };
    score *= 0.2 * duplicate_penalty + 0.8;

    score.clamp(0.0, 1.0)
}

/// Fallback function for basic name consistency when no recipe context is available
fn calculate_basic_name_consistency(measurement: &MeasurementMatch) -> f32 {
    let mut score: f32 = 0.7; // Base score

    let name = &measurement.ingredient_name;

    // Check ingredient name length
    if name.is_empty() {
        return 0.0; // No name is invalid
    } else if name.len() < 2 {
        score -= 0.3; // Very short names are suspicious
    } else if name.len() > 50 {
        score -= 0.2; // Very long names might be OCR errors
    }

    // Check for excessive numbers in ingredient name (might be OCR error)
    let digit_count = name.chars().filter(|c| c.is_numeric()).count();
    if digit_count > 2 {
        score -= 0.3; // Too many numbers in ingredient name
    }

    // Check for common ingredient name patterns (lowercase letters, spaces, hyphens)
    let valid_chars = name
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace() || *c == '-' || *c == '\'')
        .count();
    let char_ratio = valid_chars as f32 / name.len() as f32;

    if char_ratio < 0.7 {
        score -= 0.2; // Too many special characters
    }

    score.clamp(0.0, 1.0)
}

/// Check if quantity format is valid (number, decimal, or fraction)
fn is_valid_quantity_format(quantity: &str) -> bool {
    // Check if it's a simple number or decimal
    if quantity.parse::<f32>().is_ok() {
        return true;
    }

    // Check if it's a fraction (e.g., "1/2")
    if quantity.contains('/') {
        let parts: Vec<&str> = quantity.split('/').collect();
        if parts.len() == 2 {
            return parts[0].parse::<f32>().is_ok() && parts[1].parse::<f32>().is_ok();
        }
    }

    // Check for Unicode fractions (½, ⅓, ¼, etc.)
    let unicode_fractions = ['½', '⅓', '⅔', '¼', '¾', '⅕', '⅖', '⅗', '⅘', '⅙', '⅚', '⅛', '⅜', '⅝', '⅞'];
    if quantity.chars().any(|c| unicode_fractions.contains(&c)) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_match(
        quantity: &str,
        measurement: Option<&str>,
        ingredient_name: &str,
    ) -> MeasurementMatch {
        MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: measurement.map(|s| s.to_string()),
            ingredient_name: ingredient_name.to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 10,
            confidence: None,
        }
    }

    #[test]
    fn test_calculate_confidence_high_quality() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let confidence = calculate_ingredient_confidence(&measurement, "2 cups flour", Some(0.95), None);

        assert!(confidence.overall_score > 0.7, "High quality match should have high confidence");
        assert!(confidence.pattern_strength > 0.8);
        assert!(confidence.measurement_validity > 0.6);
        assert_eq!(confidence.ocr_quality, 0.95);
    }

    #[test]
    fn test_calculate_confidence_medium_quality() {
        let measurement = create_test_match("500", None, "eggs");
        let confidence = calculate_ingredient_confidence(&measurement, "500 eggs", Some(0.7), None);

        assert!(confidence.overall_score >= 0.4 && confidence.overall_score <= 0.8);
    }

    #[test]
    fn test_calculate_confidence_low_quality() {
        let measurement = create_test_match("0", Some("cups"), "a");
        let confidence = calculate_ingredient_confidence(&measurement, "0 cups a", Some(0.5), None);

        assert!(confidence.overall_score < 0.5, "Low quality match should have low confidence, got {}", confidence.overall_score);
    }

    #[test]
    fn test_confidence_to_level_high() {
        let confidence = IngredientConfidence {
            overall_score: 0.85,
            pattern_strength: 0.9,
            measurement_validity: 0.9,
            context_consistency: 0.8,
            ocr_quality: 0.8,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::High);
    }

    #[test]
    fn test_confidence_to_level_medium() {
        let confidence = IngredientConfidence {
            overall_score: 0.65,
            pattern_strength: 0.7,
            measurement_validity: 0.7,
            context_consistency: 0.6,
            ocr_quality: 0.6,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Medium);
    }

    #[test]
    fn test_confidence_to_level_low() {
        let confidence = IngredientConfidence {
            overall_score: 0.4,
            pattern_strength: 0.5,
            measurement_validity: 0.4,
            context_consistency: 0.3,
            ocr_quality: 0.4,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Low);
    }

    #[test]
    fn test_confidence_to_level_invalid() {
        let confidence = IngredientConfidence {
            overall_score: 0.2,
            pattern_strength: 0.3,
            measurement_validity: 0.2,
            context_consistency: 0.1,
            ocr_quality: 0.2,
        };

        assert_eq!(confidence_to_level(&confidence), ConfidenceLevel::Invalid);
    }

    #[test]
    fn test_pattern_strength_with_unit() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength > 0.8);
    }

    #[test]
    fn test_pattern_strength_without_unit() {
        let measurement = create_test_match("6", None, "eggs");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength >= 0.5 && strength < 0.9);
    }

    #[test]
    fn test_pattern_strength_no_ingredient() {
        let measurement = create_test_match("2", Some("cups"), "");
        let strength = calculate_pattern_strength(&measurement);
        assert!(strength < 0.2);
    }

    #[test]
    fn test_measurement_validity_normal() {
        let measurement = create_test_match("2", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity > 0.6);
    }

    #[test]
    fn test_measurement_validity_zero() {
        let measurement = create_test_match("0", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.2);
    }

    #[test]
    fn test_measurement_validity_negative() {
        let measurement = create_test_match("-5", Some("cups"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.2);
    }

    #[test]
    fn test_measurement_validity_very_large() {
        let measurement = create_test_match("50000", Some("grams"), "flour");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity < 0.5);
    }

    #[test]
    fn test_measurement_validity_fraction() {
        let measurement = create_test_match("1/2", Some("cup"), "sugar");
        let validity = calculate_measurement_validity(&measurement);
        assert!(validity > 0.6);
    }

    #[test]
    fn test_context_consistency_normal() {
        let measurement = create_test_match("2", Some("cups"), "all-purpose flour");
        let consistency = calculate_context_consistency(&measurement, "2 cups all-purpose flour");
        assert!(consistency > 0.6);
    }

    #[test]
    fn test_context_consistency_short_name() {
        let measurement = create_test_match("2", Some("cups"), "a");
        let consistency = calculate_context_consistency(&measurement, "2 cups a");
        assert!(consistency < 0.5);
    }

    #[test]
    fn test_context_consistency_too_many_numbers() {
        let measurement = create_test_match("2", Some("cups"), "flour123456");
        let consistency = calculate_context_consistency(&measurement, "2 cups flour123456");
        assert!(consistency < 0.5);
    }

    #[test]
    fn test_is_valid_quantity_format() {
        assert!(is_valid_quantity_format("2"));
        assert!(is_valid_quantity_format("2.5"));
        assert!(is_valid_quantity_format("1/2"));
        assert!(is_valid_quantity_format("½"));
        assert!(!is_valid_quantity_format("abc"));
        assert!(!is_valid_quantity_format(""));
    }

    #[test]
    fn test_confidence_boundaries() {
        // Test that confidence scores are always between 0 and 1
        let test_cases = vec![
            create_test_match("2", Some("cups"), "flour"),
            create_test_match("0", Some("cups"), ""),
            create_test_match("99999", None, "x"),
            create_test_match("1/2", Some("tsp"), "salt"),
        ];

        for measurement in test_cases {
            let confidence = calculate_ingredient_confidence(&measurement, "", Some(0.8), None);
            assert!(confidence.overall_score >= 0.0 && confidence.overall_score <= 1.0);
            assert!(confidence.pattern_strength >= 0.0 && confidence.pattern_strength <= 1.0);
            assert!(confidence.measurement_validity >= 0.0 && confidence.measurement_validity <= 1.0);
            assert!(confidence.context_consistency >= 0.0 && confidence.context_consistency <= 1.0);
        }
    }
}
