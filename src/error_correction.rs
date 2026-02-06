//! # OCR Error Correction Module
//!
//! This module provides intelligent OCR error correction for recipe text.
//! It implements multiple correction strategies to improve OCR accuracy:
//!
//! - Character-level corrections for common OCR mistakes
//! - Word-level corrections and expansions
//! - Unit normalization (tbsp → tablespoon)
//! - Ingredient name fuzzy matching and correction
//! - Context-aware corrections based on recipe patterns

use regex::Regex;
use std::collections::HashMap;
use tracing::{debug, trace};

/// Configuration for OCR error correction
#[derive(Debug, Clone)]
pub struct ErrorCorrectionConfig {
    /// Whether to enable fuzzy matching for ingredient names
    pub enable_fuzzy_matching: bool,
    /// Maximum edit distance for fuzzy matching (1-3 recommended)
    pub max_edit_distance: usize,
    /// Whether to enable context-aware corrections
    pub enable_context_corrections: bool,
    /// Minimum confidence threshold for corrections (0.0-1.0)
    pub min_correction_confidence: f32,
}

impl Default for ErrorCorrectionConfig {
    fn default() -> Self {
        Self {
            enable_fuzzy_matching: true,
            max_edit_distance: 2,
            enable_context_corrections: true,
            min_correction_confidence: 0.7,
        }
    }
}

/// Main error correction engine
pub struct OcrErrorCorrector {
    config: ErrorCorrectionConfig,
    character_corrections: HashMap<String, String>,
    word_corrections: HashMap<String, String>,
    unit_expansions: HashMap<String, String>,
    ingredient_corrections: HashMap<String, String>,
    fuzzy_ingredient_words: Vec<String>,
    fuzzy_matcher: Option<FuzzyMatcher>,
}

impl OcrErrorCorrector {
    /// Create a new error corrector with default configuration
    pub fn new() -> Self {
        Self::with_config(ErrorCorrectionConfig::default())
    }

    /// Create a new error corrector with custom configuration
    pub fn with_config(config: ErrorCorrectionConfig) -> Self {
        let mut corrector = Self {
            config,
            character_corrections: HashMap::new(),
            word_corrections: HashMap::new(),
            unit_expansions: HashMap::new(),
            ingredient_corrections: HashMap::new(),
            fuzzy_ingredient_words: Vec::new(),
            fuzzy_matcher: None,
        };

        corrector.initialize_corrections();
        corrector
    }

    /// Initialize all correction dictionaries
    fn initialize_corrections(&mut self) {
        self.initialize_character_corrections();
        self.initialize_word_corrections();
        self.initialize_unit_expansions();
        self.initialize_ingredient_corrections();

        // Initialize fuzzy matching words (correct ingredient names)
        self.fuzzy_ingredient_words = vec![
            "flour".to_string(),
            "sugar".to_string(),
            "salt".to_string(),
            "butter".to_string(),
            "milk".to_string(),
            "eggs".to_string(),
            "egg".to_string(),
            "water".to_string(),
            "oil".to_string(),
            "vanilla".to_string(),
            "baking powder".to_string(),
            "baking soda".to_string(),
            "cinnamon".to_string(),
            "chocolate".to_string(),
            "cream".to_string(),
            "cheese".to_string(),
            // French ingredients
            "farine".to_string(),
            "sucre".to_string(),
            "sel".to_string(),
            "beurre".to_string(),
            "lait".to_string(),
            "œufs".to_string(),
            "eau".to_string(),
            "huile".to_string(),
            "vanille".to_string(),
            "levure".to_string(),
            "cannelle".to_string(),
            "chocolat".to_string(),
            "crème".to_string(),
            "fromage".to_string(),
        ];

        if self.config.enable_fuzzy_matching {
            self.fuzzy_matcher = Some(FuzzyMatcher::new(self.config.max_edit_distance));
        }
    }

    /// Initialize character-level corrections for common OCR mistakes
    fn initialize_character_corrections(&mut self) {
        let corrections = [
            // Fraction-specific corrections (more targeted)
            ("Ye", "1/2"), // Common 1/2 misread
            ("%", "1/4"),  // Common 1/4 misread
            ("Vz", "1/3"), // Common 1/3 misread
            ("V4", "1/4"), // Alternative 1/4 misread
            ("V2", "1/2"), // Alternative 1/2 misread
            ("l/2", "1/2"),
            ("l/3", "1/3"),
            ("l/4", "1/4"),
            ("O/2", "1/2"),
            ("O/3", "1/3"),
            ("O/4", "1/4"),
            // Specific character confusions in measurements
            ("tbIp", "tbsp"), // tablespoon misread
            ("tsp", "tsp"),   // teaspoon (already correct)
            // Limited character corrections to avoid false positives
            ("Ib", "lb"), // Capital i mistaken for lowercase L in "lb"
            ("Ibs", "lbs"),
        ];

        for (from, to) in corrections {
            self.character_corrections
                .insert(from.to_string(), to.to_string());
        }
    }

    /// Initialize word-level corrections for common OCR mistakes
    fn initialize_word_corrections(&mut self) {
        let corrections = [
            // Common word corrections
            ("teaspoon", "teaspoon"), // Already correct, but ensure consistency
            ("tablespoon", "tablespoon"),
            ("cup", "cup"),
            ("cups", "cups"),
            ("pound", "pound"),
            ("pounds", "pounds"),
            ("ounce", "ounce"),
            ("ounces", "ounces"),
            ("gram", "gram"),
            ("grams", "grams"),
            ("kilogram", "kilogram"),
            ("kilograms", "kilograms"),
            ("liter", "liter"),
            ("liters", "liters"),
            ("milliliter", "milliliter"),
            ("milliliters", "milliliters"),
            // Common OCR misreads
            ("teaspoo", "teaspoon"),
            ("tablespoo", "tablespoon"),
            ("tablspoon", "tablespoon"),
            ("tablsp", "tablespoon"),
            ("teasp", "teaspoon"),
            ("tsp", "teaspoon"),
            ("tbsp", "tablespoon"),
            ("tbs", "tablespoon"),
            ("cup", "cup"),
            ("cups", "cups"),
            ("Ib", "lb"), // Capital i mistaken for lowercase L
            ("Ibs", "lbs"),
            ("pound", "pound"),
            ("pounds", "pounds"),
            ("ounce", "ounce"),
            ("ounces", "ounces"),
            ("gram", "gram"),
            ("grams", "grams"),
            ("kilogram", "kilogram"),
            ("kilograms", "kilograms"),
            ("liter", "liter"),
            ("liters", "liters"),
            ("litre", "liter"),
            ("litres", "liters"),
            ("milliliter", "milliliter"),
            ("milliliters", "milliliters"),
            ("millilitre", "milliliter"),
            ("millilitres", "milliliters"),
            // Cooking terms
            ("chopped", "chopped"),
            ("diced", "diced"),
            ("minced", "minced"),
            ("sliced", "sliced"),
            ("grated", "grated"),
            ("ground", "ground"),
            ("fresh", "fresh"),
            ("large", "large"),
            ("medium", "medium"),
            ("small", "small"),
        ];

        for (from, to) in corrections {
            self.word_corrections
                .insert(from.to_lowercase(), to.to_string());
        }
    }

    /// Initialize unit expansions (abbreviations to full forms)
    fn initialize_unit_expansions(&mut self) {
        let expansions = [
            ("tsp", "teaspoon"),
            ("tsp.", "teaspoon"),
            ("tbsp", "tablespoon"),
            ("tbsp.", "tablespoon"),
            ("tbs", "tablespoon"),
            ("tbs.", "tablespoon"),
            ("c", "cup"),
            ("c.", "cup"),
            ("cup", "cup"),
            ("cups", "cups"),
            ("lb", "pound"),
            ("lbs", "pounds"),
            ("oz", "ounce"),
            ("oz.", "ounce"),
            ("g", "gram"),
            ("gm", "gram"),
            ("kg", "kilogram"),
            ("l", "liter"),
            ("ml", "milliliter"),
            ("fl oz", "fluid ounce"),
            ("fl. oz.", "fluid ounce"),
            ("pt", "pint"),
            ("qt", "quart"),
            ("gal", "gallon"),
        ];

        for (from, to) in expansions {
            self.unit_expansions
                .insert(from.to_lowercase(), to.to_string());
        }
    }

    /// Initialize ingredient name corrections
    fn initialize_ingredient_corrections(&mut self) {
        let corrections = [
            // Common ingredient corrections
            ("flour", "flour"),
            ("sugar", "sugar"),
            ("salt", "salt"),
            ("butter", "butter"),
            ("milk", "milk"),
            ("eggs", "eggs"),
            ("egg", "egg"),
            ("water", "water"),
            ("oil", "oil"),
            ("vanilla", "vanilla"),
            ("baking powder", "baking powder"),
            ("baking soda", "baking soda"),
            ("cinnamon", "cinnamon"),
            ("chocolate", "chocolate"),
            ("cream", "cream"),
            ("cheese", "cheese"),
            // Common OCR misreads for ingredients
            ("fiour", "flour"),
            ("suger", "sugar"),
            ("sait", "salt"),
            ("buter", "butter"),
            ("mik", "milk"),
            ("egs", "eggs"),
            ("eg", "egg"),
            ("wter", "water"),
            ("oi", "oil"),
            ("vanila", "vanilla"),
            ("bakng powder", "baking powder"),
            ("bakng soda", "baking soda"),
            ("cinamon", "cinnamon"),
            ("choclate", "chocolate"),
            ("crem", "cream"),
            ("chese", "cheese"),
            // French ingredients (common in recipes)
            ("farine", "farine"),
            ("sucre", "sucre"),
            ("sel", "sel"),
            ("beurre", "beurre"),
            ("lait", "lait"),
            ("oeufs", "œufs"),
            ("œufs", "œufs"),
            ("eau", "eau"),
            ("huile", "huile"),
            ("vanille", "vanille"),
            ("levure", "levure"),
            ("cannelle", "cannelle"),
            ("chocolat", "chocolat"),
            ("crème", "crème"),
            ("fromage", "fromage"),
        ];

        for (from, to) in corrections {
            self.ingredient_corrections
                .insert(from.to_lowercase(), to.to_string());
        }
    }

    /// Apply all error corrections to the input text
    pub fn correct_text(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        debug!("Starting OCR error correction on text: '{}'", text);

        // Apply corrections in order of specificity (most specific first)
        corrected = self.apply_character_corrections(&corrected);
        corrected = self.apply_word_corrections(&corrected);
        corrected = self.apply_unit_expansions(&corrected);
        corrected = self.apply_ingredient_corrections(&corrected);

        if self.config.enable_context_corrections {
            corrected = self.apply_context_corrections(&corrected);
        }

        if self.config.enable_fuzzy_matching {
            if let Some(fuzzy) = &self.fuzzy_matcher {
                corrected =
                    fuzzy.correct_with_fuzzy_matching(&corrected, &self.fuzzy_ingredient_words);
            }
        }

        debug!(
            "OCR error correction completed: '{}' -> '{}'",
            text, corrected
        );
        corrected
    }

    /// Apply character-level corrections
    fn apply_character_corrections(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        for (from, to) in &self.character_corrections {
            // Use word boundaries for multi-character corrections to avoid false positives
            let pattern = if from.len() == 1 {
                regex::escape(from)
            } else {
                format!(r"\b{}\b", regex::escape(from))
            };

            if let Ok(regex) = Regex::new(&pattern) {
                let before = corrected.clone();
                corrected = regex.replace_all(&corrected, to).to_string();
                if before != corrected {
                    trace!(
                        "Character correction: '{}' -> '{}' in '{}'",
                        from,
                        to,
                        before
                    );
                }
            }
        }

        corrected
    }

    /// Apply word-level corrections
    fn apply_word_corrections(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        for (from, to) in &self.word_corrections {
            let pattern = format!(r"\b{}\b", regex::escape(from));

            if let Ok(regex) = Regex::new(&pattern) {
                let before = corrected.clone();
                corrected = regex.replace_all(&corrected, to).to_string();
                if before != corrected {
                    trace!("Word correction: '{}' -> '{}' in '{}'", from, to, before);
                }
            }
        }

        corrected
    }

    /// Apply unit expansions
    fn apply_unit_expansions(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        for (from, to) in &self.unit_expansions {
            // Case-insensitive matching
            let pattern = format!(r"(?i)\b{}\b", regex::escape(from));

            if let Ok(regex) = Regex::new(&pattern) {
                let before = corrected.clone();
                corrected = regex.replace_all(&corrected, to).to_string();
                if before != corrected {
                    trace!("Unit expansion: '{}' -> '{}' in '{}'", from, to, before);
                }
            }
        }

        corrected
    }

    /// Apply ingredient name corrections
    fn apply_ingredient_corrections(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        for (from, to) in &self.ingredient_corrections {
            // Case-insensitive matching
            let pattern = format!(r"(?i)\b{}\b", regex::escape(from));

            if let Ok(regex) = Regex::new(&pattern) {
                let before = corrected.clone();
                corrected = regex.replace_all(&corrected, to).to_string();
                if before != corrected {
                    trace!(
                        "Ingredient correction: '{}' -> '{}' in '{}'",
                        from,
                        to,
                        before
                    );
                }
            }
        }

        corrected
    }

    /// Apply context-aware corrections based on recipe patterns
    fn apply_context_corrections(&self, text: &str) -> String {
        let mut corrected = text.to_string();

        // Context-aware corrections for common recipe patterns
        let context_corrections = [
            // Correct "tsp" to "teaspoon" only when followed by ingredient
            (r"\btsp\b(?=\s+[a-zA-Z])", "teaspoon"),
            (r"\btbsp\b(?=\s+[a-zA-Z])", "tablespoon"),
            // Correct fractions in measurements
            (r"(\d+)\s*/\s*(\d+)", "$1/$2"), // Remove spaces around fraction slashes
            // Correct common ingredient misspellings in context
            (r"\bflour\b", "flour"),
            (r"\bsuger\b", "sugar"),
            (r"\bsalt\b", "salt"),
        ];

        for (pattern, replacement) in context_corrections {
            if let Ok(regex) = Regex::new(pattern) {
                let before = corrected.clone();
                corrected = regex.replace_all(&corrected, replacement).to_string();
                if before != corrected {
                    trace!(
                        "Context correction: '{}' -> '{}' in '{}'",
                        pattern,
                        replacement,
                        before
                    );
                }
            }
        }

        corrected
    }
}

impl Default for OcrErrorCorrector {
    fn default() -> Self {
        Self::new()
    }
}

/// Fuzzy string matching for ingredient names
struct FuzzyMatcher {
    max_distance: usize,
}

impl FuzzyMatcher {
    fn new(max_distance: usize) -> Self {
        Self { max_distance }
    }

    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        let len1 = s1_chars.len();
        let len2 = s2_chars.len();

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        #[allow(clippy::needless_range_loop)]
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                    0
                } else {
                    1
                };

                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[len1][len2]
    }

    /// Find the best fuzzy match for a word
    fn find_best_match(&self, word: &str, candidates: &[String]) -> Option<String> {
        let word_lower = word.to_lowercase();
        let mut best_match: Option<String> = None;
        let mut best_distance = self.max_distance + 1;

        for candidate in candidates {
            let distance = self.levenshtein_distance(&word_lower, &candidate.to_lowercase());
            if distance < best_distance && distance <= self.max_distance && distance > 0 {
                best_distance = distance;
                best_match = Some(candidate.clone());
            }
        }

        best_match
    }

    /// Apply fuzzy matching corrections to text
    fn correct_with_fuzzy_matching(&self, text: &str, correct_words: &[String]) -> String {
        let mut corrected = text.to_string();

        // Words to skip for fuzzy matching (units, numbers, etc.)
        let skip_words = [
            "cup",
            "cups",
            "tablespoon",
            "tablespoons",
            "teaspoon",
            "teaspoons",
            "pound",
            "pounds",
            "ounce",
            "ounces",
            "gram",
            "grams",
            "kilogram",
            "kilograms",
            "liter",
            "liters",
            "milliliter",
            "milliliters",
            "pint",
            "pints",
            "quart",
            "quarts",
            "gallon",
            "gallons",
            "tbsp",
            "tsp",
            "lb",
            "lbs",
            "oz",
            "g",
            "kg",
            "l",
            "ml",
            "fl",
            "fluid",
            "oz",
            "and",
            "or",
            "the",
            "a",
            "an",
            "of",
            "with",
            "for",
            "to",
            "large",
            "medium",
            "small",
            "fresh",
            "ground",
            "chopped",
            "diced",
            "minced",
            "sliced",
        ];

        // Split text into words and process each word
        let words: Vec<&str> = text.split_whitespace().collect();

        for word in words {
            // Remove punctuation for matching
            let clean_word = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();

            // Skip short words, numbers, and measurement-related words
            if clean_word.len() < 3
                || clean_word.chars().all(|c| c.is_numeric())
                || skip_words.contains(&clean_word.as_str())
            {
                continue;
            }

            if let Some(correction) = self.find_best_match(&clean_word, correct_words) {
                if clean_word != correction.to_lowercase() {
                    // Replace the word in the text
                    let pattern = format!(r"\b{}\b", regex::escape(word));
                    if let Ok(regex) = Regex::new(&pattern) {
                        let before = corrected.clone();
                        corrected = regex.replace_all(&corrected, &correction).to_string();
                        if before != corrected {
                            trace!(
                                "Fuzzy correction: '{}' -> '{}' in '{}'",
                                word,
                                correction,
                                before
                            );
                        }
                    }
                }
            }
        }

        corrected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_corrections() {
        let corrector = OcrErrorCorrector::new();

        assert_eq!(
            corrector.apply_character_corrections("Ye cup flour"),
            "1/2 cup flour"
        );
        assert_eq!(
            corrector.apply_character_corrections("% cup sugar"),
            "1/4 cup sugar"
        );
        assert_eq!(
            corrector.apply_character_corrections("l/2 tsp salt"),
            "1/2 tsp salt"
        );
    }

    #[test]
    fn test_unit_expansions() {
        let corrector = OcrErrorCorrector::new();

        assert_eq!(
            corrector.apply_unit_expansions("2 tbsp butter"),
            "2 tablespoon butter"
        );
        assert_eq!(
            corrector.apply_unit_expansions("1 tsp vanilla"),
            "1 teaspoon vanilla"
        );
        assert_eq!(
            corrector.apply_unit_expansions("3 cups flour"),
            "3 cups flour"
        );
    }

    #[test]
    fn test_ingredient_corrections() {
        let corrector = OcrErrorCorrector::new();

        assert_eq!(
            corrector.apply_ingredient_corrections("2 cups fiour"),
            "2 cups flour"
        );
        assert_eq!(
            corrector.apply_ingredient_corrections("1 cup suger"),
            "1 cup sugar"
        );
        assert_eq!(
            corrector.apply_ingredient_corrections("1/2 tsp sait"),
            "1/2 tsp salt"
        );
    }

    #[test]
    fn test_full_correction_pipeline() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: false, // Disable fuzzy matching for this test
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        let input = "2 tbsp fiour\n1 tsp suger\nYe cup sait";
        let expected = "2 tablespoon flour\n1 teaspoon sugar\n1/2 cup salt";

        let result = corrector.correct_text(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_fuzzy_matching() {
        let config = ErrorCorrectionConfig {
            enable_fuzzy_matching: true,
            max_edit_distance: 1,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        // Test fuzzy matching for ingredients
        let result = corrector.correct_text("2 cups flur"); // "flur" should match "flour"
        assert!(result.contains("flour"));
    }

    #[test]
    fn test_context_corrections() {
        let config = ErrorCorrectionConfig {
            enable_context_corrections: true,
            ..Default::default()
        };
        let corrector = OcrErrorCorrector::with_config(config);

        let result = corrector.correct_text("2 1 / 2 cups flour");
        assert_eq!(result, "2 1/2 cups flour");
    }
}
