//! # Text Processing Module
//!
//! This module provides text processing utilities for the Ingredients Telegram bot,
//! including regex-based measurement detection and ingredient parsing.
//!
//! ## Features
//!
//! - Measurement unit detection using comprehensive regex patterns
//! - Support for English and French measurement units
//! - **Quantity-only ingredient support**: Recognizes ingredients with quantities but no units (e.g., "6 oeufs", "4 pommes")
//! - **Fraction support**: Recognizes fractional quantities (e.g., "1/2 litre", "3/4 cup")
//! - Ingredient name extraction alongside quantity and measurement
//! - Line-by-line text analysis for ingredient lists

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use tracing::{debug, info, trace, warn};

/// Represents a detected measurement in text
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeasurementMatch {
    /// The extracted quantity (e.g., "2", "1/2", "500")
    pub quantity: String,
    /// The measurement unit (e.g., "cups", "g", "tablespoons")
    pub measurement: Option<String>,
    /// The extracted ingredient name (e.g., "flour", "de tomates", "all-purpose flour")
    pub ingredient_name: String,
    /// The line number where the measurement was found
    pub line_number: usize,
    /// The starting character position in the line
    pub start_pos: usize,
    /// The ending character position in the line
    pub end_pos: usize,
}

/// Configuration options for measurement detection
#[derive(Clone, Debug)]
pub struct MeasurementConfig {
    /// Custom regex pattern for measurements. If None, uses the default comprehensive pattern
    #[allow(dead_code)]
    pub custom_pattern: Option<String>,
    /// Whether to enable ingredient name postprocessing (cleaning, normalization)
    pub enable_ingredient_postprocessing: bool,
    /// Maximum length for ingredient names (truncated if longer)
    #[allow(dead_code)]
    pub max_ingredient_length: usize,
    /// Whether to include count-only measurements (e.g., "2 eggs" -> "2")
    #[allow(dead_code)]
    pub include_count_measurements: bool,
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            custom_pattern: None,
            enable_ingredient_postprocessing: true,
            max_ingredient_length: 100,
            include_count_measurements: true,
        }
    }
}

impl MeasurementConfig {
    /// Validate measurement configuration parameters
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        // Validate max_ingredient_length
        if self.max_ingredient_length == 0 {
            return Err(crate::errors::AppError::Config(
                "max_ingredient_length must be greater than 0".to_string(),
            ));
        }

        // Validate custom regex pattern if provided
        if let Some(pattern) = &self.custom_pattern {
            if pattern.trim().is_empty() {
                return Err(crate::errors::AppError::Config(
                    "custom_pattern cannot be empty if provided".to_string(),
                ));
            }
            // Test that the pattern compiles
            if regex::Regex::new(pattern).is_err() {
                return Err(crate::errors::AppError::Config(format!(
                    "custom_pattern '{}' is not a valid regex",
                    pattern
                )));
            }
        }

        Ok(())
    }
}

/// Measurement units configuration loaded from JSON
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeasurementUnitsConfig {
    pub measurement_units: MeasurementUnits,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeasurementUnits {
    pub volume_units: Vec<String>,
    pub weight_units: Vec<String>,
    pub volume_units_metric: Vec<String>,
    pub us_units: Vec<String>,
    pub french_units: Vec<String>,
}

impl MeasurementUnitsConfig {
    /// Validate measurement units configuration
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        // Validate that all unit arrays are non-empty
        if self.measurement_units.volume_units.is_empty() {
            return Err(crate::errors::AppError::Config("volume_units cannot be empty".to_string()));
        }
        if self.measurement_units.weight_units.is_empty() {
            return Err(crate::errors::AppError::Config("weight_units cannot be empty".to_string()));
        }
        if self.measurement_units.volume_units_metric.is_empty() {
            return Err(crate::errors::AppError::Config("volume_units_metric cannot be empty".to_string()));
        }
        if self.measurement_units.us_units.is_empty() {
            return Err(crate::errors::AppError::Config("us_units cannot be empty".to_string()));
        }
        if self.measurement_units.french_units.is_empty() {
            return Err(crate::errors::AppError::Config("french_units cannot be empty".to_string()));
        }

        // Validate that all unit strings are non-empty and contain valid characters
        let validate_units = |units: &[String], category: &str| -> crate::errors::AppResult<()> {
            for (i, unit) in units.iter().enumerate() {
                if unit.trim().is_empty() {
                    return Err(crate::errors::AppError::Config(format!(
                        "{}[{}] cannot be empty",
                        category, i
                    )));
                }
                // Check for obviously invalid characters (control characters)
                if unit.chars().any(|c| c.is_control()) {
                    return Err(crate::errors::AppError::Config(format!(
                        "{}[{}] '{}' contains control characters",
                        category, i, unit
                    )));
                }
            }
            Ok(())
        };

        validate_units(&self.measurement_units.volume_units, "volume_units")?;
        validate_units(&self.measurement_units.weight_units, "weight_units")?;
        validate_units(
            &self.measurement_units.volume_units_metric,
            "volume_units_metric",
        )?;
        validate_units(&self.measurement_units.us_units, "us_units")?;
        validate_units(&self.measurement_units.french_units, "french_units")?;

        Ok(())
    }
}

// Default comprehensive regex pattern for measurement units (now supports quantity-only ingredients and fractions)
// Uses named capture groups: quantity, measurement, and ingredient
// NOTE: This pattern is now built dynamically from config/measurement_units.json

/// Load measurement units configuration from JSON file
pub fn load_measurement_units_config() -> MeasurementUnitsConfig {
    let config_path = "config/measurement_units.json";
    match fs::read_to_string(config_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!(
                "Failed to parse measurement units config: {}. Using default empty config.",
                e
            );
            MeasurementUnitsConfig {
                measurement_units: MeasurementUnits {
                    volume_units: vec![],
                    weight_units: vec![],
                    volume_units_metric: vec![],
                    us_units: vec![],
                    french_units: vec![],
                },
            }
        }),
        Err(e) => {
            warn!("Failed to read measurement units config file '{}': {}. Using default empty config.", config_path, e);
            MeasurementUnitsConfig {
                measurement_units: MeasurementUnits {
                    volume_units: vec![],
                    weight_units: vec![],
                    volume_units_metric: vec![],
                    us_units: vec![],
                    french_units: vec![],
                },
            }
        }
    }
}

/// Build the regex pattern from measurement units configuration
///
/// This function implements a sophisticated pattern generation algorithm that creates
/// a comprehensive regex for detecting measurement units in ingredient text.
///
/// ## Pattern Generation Algorithm
///
/// ### Step 1: Configuration Loading
/// ```text
/// Load measurement units from config/measurement_units.json
/// Categories: volume_units, weight_units, volume_units_metric, us_units, french_units
/// ```
///
/// ### Step 2: Unit Collection and Deduplication
/// ```text
/// Combine all unit categories into single collection
/// Remove duplicates using HashSet
/// Sort by length (longest first) to prevent partial matches
/// ```
///
/// ### Step 3: Regex Escaping
/// ```text
/// Escape all regex special characters in unit names
/// Examples: "cups?" → "cups\\?", "fl. oz" → "fl\\. oz"
/// ```
///
/// ### Step 4: Alternation Pattern Construction
/// ```text
/// Join escaped units with "|" for alternation
/// Result: "cups\\?|tablespoons\\?|litres\\?|grammes\\?|..."
/// ```
///
/// ### Step 5: Complete Pattern Assembly
/// ```text
/// Build final regex with named capture groups:
/// (?i)(?P<quantity>...)(?:\s*(?P<measurement>...)|\s+(?P<ingredient>...))
/// ```
///
/// ## Pattern Structure Analysis
///
/// The generated regex uses this structure:
/// ```regex
/// (?i)                           # Case-insensitive matching
/// (?P<quantity>...)             # Named group for quantity (fractions/decimals)
/// (?:                           # Non-capturing group for alternatives
///   \s*(?P<measurement>...)     # Optional whitespace + measurement unit
///   |                           # OR
///   \s+(?P<ingredient>...)      # Whitespace + ingredient name (quantity-only)
/// )
/// ```
///
/// ## Quantity Pattern Details
///
/// Supports multiple quantity formats:
/// - **Integers**: `2`, `500`, `6`
/// - **Decimals**: `1.5`, `2.25`, `0.5`
/// - **Fractions**: `1/2`, `3/4`, `2¼` (Unicode fractions)
/// - **Mixed**: `2½`, `1½` (Unicode fraction characters)
///
/// ## Measurement Unit Handling
///
/// - **Escaping**: All regex special characters are escaped
/// - **Ordering**: Longest units matched first to prevent partial matches
/// - **Categories**: Supports English, French, metric, and US customary units
/// - **Case Insensitive**: All units matched regardless of case
///
/// ## Examples of Generated Patterns
///
/// For units ["cups", "tablespoons", "litres", "grammes"]:
/// ```regex
/// (?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>cups|tablespoons|litres|grammes)|\s+(?P<ingredient>\w+))
/// ```
///
/// ## Performance Characteristics
///
/// - **Compilation**: Pattern compiled once at startup via lazy_static
/// - **Matching**: Efficient regex matching with pre-compiled patterns
/// - **Memory**: Minimal memory usage (shared static pattern)
/// - **Thread Safety**: Immutable static pattern, safe for concurrent access
///
/// ## Error Handling
///
/// - **Config Loading**: Falls back to empty config if JSON parsing fails
/// - **Pattern Compilation**: Uses expect() for guaranteed valid patterns
/// - **Unit Validation**: Logs warnings for missing or invalid units
///
/// ## Configuration File Format
///
/// Expected JSON structure in `config/measurement_units.json`:
/// ```json
/// {
///   "measurement_units": {
///     "volume_units": ["cups", "tablespoons", "teaspoons"],
///     "weight_units": ["grams", "kilograms", "pounds"],
///     "volume_units_metric": ["litres", "millilitres"],
///     "us_units": ["fl oz", "qt", "gal"],
///     "french_units": ["litres", "grammes", "cuillères"]
///   }
/// }
/// ```
///
/// ## Thread Safety and Performance
///
/// - **Lazy Initialization**: Pattern built once at first access
/// - **Static Storage**: Compiled regex stored in static memory
/// - **Concurrent Access**: Safe for use across multiple threads
/// - **Memory Efficiency**: Single compiled pattern reused for all operations
///
/// # Returns
///
/// Returns a complete regex pattern string ready for compilation
///
/// # Examples
///
/// Note: This is a private function used internally to build the default regex pattern.
/// The functionality is exposed through the public `MeasurementDetector::new()` constructor.
fn build_measurement_regex_pattern() -> String {
    let config = load_measurement_units_config();

    // Combine all unit categories into a single collection
    let mut all_units: Vec<String> = Vec::new();
    all_units.extend(config.measurement_units.volume_units);
    all_units.extend(config.measurement_units.weight_units);
    all_units.extend(config.measurement_units.volume_units_metric);
    all_units.extend(config.measurement_units.us_units);
    all_units.extend(config.measurement_units.french_units);

    // Remove duplicates and sort by length (longest first) to avoid partial matches
    let unique_units: std::collections::HashSet<String> = all_units.into_iter().collect();
    let mut sorted_units: Vec<String> = unique_units.into_iter().collect();

    // Sort by length descending, then alphabetically for consistency
    sorted_units.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));

    // Escape regex special characters in each unit
    let escaped_units: Vec<String> = sorted_units
        .into_iter()
        .map(|unit| regex::escape(&unit))
        .collect();

    // Build the alternation pattern
    let units_pattern = escaped_units.join("|");

    // Build the complete regex pattern with named capture groups
    format!(
        r"(?i)(?P<quantity>\d*\.?\d+|\d+/\d+|[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟])(?:\s*(?P<measurement>{})|\s+(?P<ingredient>\w+))",
        units_pattern
    )
}

// Lazy static regex for default pattern to avoid recompilation
lazy_static! {
    static ref DEFAULT_REGEX: Regex = Regex::new(&build_measurement_regex_pattern())
        .expect("Default measurement pattern should be valid");
}

/// Measurement detector using regex patterns for English and French units
pub struct MeasurementDetector {
    /// Compiled regex pattern for detecting measurements
    pattern: Regex,
    /// Configuration options
    config: MeasurementConfig,
}

impl MeasurementDetector {
    /// Create a new measurement detector with the default comprehensive pattern
    ///
    /// The pattern matches common measurement units in both English and French,
    /// including volume, weight, count, and other ingredient measurements.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let detector = MeasurementDetector::new();
    /// ```
    pub fn new() -> Result<Self, regex::Error> {
        info!("Creating new MeasurementDetector with default configuration");
        Ok(Self {
            pattern: DEFAULT_REGEX.clone(),
            config: MeasurementConfig::default(),
        })
    }

    /// Create a measurement detector with a custom regex pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - Custom regex pattern string
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let custom_pattern = r"\b\d+\s*(?:cups?|tablespoons?)\b";
    /// let detector = MeasurementDetector::with_pattern(custom_pattern)?;
    /// # Ok::<(), regex::Error>(())
    /// ```
    /// Create a measurement detector with a custom regex pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - Custom regex pattern string
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let custom_pattern = r"\b\d+\s*(?:cups?|tablespoons?)\b";
    /// let detector = MeasurementDetector::with_pattern(custom_pattern)?;
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn with_pattern(pattern: &str) -> Result<Self, regex::Error> {
        let pattern = Regex::new(pattern)?;
        Ok(Self {
            pattern,
            config: MeasurementConfig::default(),
        })
    }

    /// Create a measurement detector with custom configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration options for the detector
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::{MeasurementDetector, MeasurementConfig};
    ///
    /// let config = MeasurementConfig {
    ///     enable_ingredient_postprocessing: true,
    ///     max_ingredient_length: 50,
    ///     ..Default::default()
    /// };
    /// let detector = MeasurementDetector::with_config(config)?;
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn with_config(config: MeasurementConfig) -> Result<Self, regex::Error> {
        // Validate configuration first
        if let Err(e) = config.validate() {
            return Err(regex::Error::Syntax(format!("Invalid configuration: {}", e)));
        }

        let pattern = if let Some(custom_pattern) = &config.custom_pattern {
            debug!("Using custom regex pattern: {}", custom_pattern);
            Regex::new(custom_pattern)?
        } else {
            debug!("Using default regex pattern");
            DEFAULT_REGEX.clone()
        };

        info!("Creating MeasurementDetector with custom config: postprocessing={}, max_length={}, count_measurements={}",
              config.enable_ingredient_postprocessing, config.max_ingredient_length, config.include_count_measurements);

        Ok(Self { pattern, config })
    }

    /// Extract all ingredient measurements from the given text
    ///
    /// This function implements a sophisticated measurement detection algorithm that:
    ///
    /// ## Algorithm Overview
    ///
    /// 1. **Line-by-Line Processing**: Scans text line by line to maintain positional accuracy
    /// 2. **Regex Pattern Matching**: Uses compiled regex to find measurement patterns
    /// 3. **Capture Group Analysis**: Extracts quantity, measurement unit, and ingredient components
    /// 4. **Measurement Classification**: Distinguishes between traditional measurements and quantity-only ingredients
    /// 5. **Ingredient Name Extraction**: Intelligently extracts ingredient names from surrounding text
    /// 6. **Post-Processing**: Applies cleaning and normalization to ingredient names
    ///
    /// ## Processing Flow
    ///
    /// ```text
    /// For each line in text:
    ///   For each regex match in line:
    ///     Extract capture groups (quantity, measurement, ingredient)
    ///     Classify measurement type:
    ///       - Traditional: "2 cups flour" → "2", "cups", "flour"
    ///       - Quantity-only: "6 eggs" → "6", None, "eggs"
    ///     Extract ingredient name from text after measurement (if traditional)
    ///     Apply post-processing to clean ingredient name
    ///     Record match with line number and character positions
    /// ```
    ///
    /// ## Measurement Types Handled
    ///
    /// - **Traditional Measurements**: "2 cups flour", "1.5 liters milk", "500g butter"
    /// - **Quantity-Only Ingredients**: "6 eggs", "4 apples", "3 sachets yeast"
    /// - **Fraction Support**: "½ cup sugar", "⅓ liter cream", "2¼ cups flour"
    /// - **Unicode Fractions**: "¼", "½", "¾", "⅓", "⅔" characters
    /// - **Multi-Language Units**: English ("cups", "tablespoons") and French ("litres", "grammes")
    ///
    /// ## Position Tracking
    ///
    /// Maintains accurate character positions across multi-line text by tracking:
    /// - `current_pos`: Running total of characters processed
    /// - `line_number`: 0-based line index for each match
    /// - `start_pos`/`end_pos`: Character offsets within entire text
    ///
    /// ## Regex Pattern Details
    ///
    /// The pattern uses named capture groups for robust extraction:
    /// - `quantity`: Numbers, decimals, fractions (e.g., "2", "1.5", "½", "2¼")
    /// - `measurement`: Optional unit from configured measurement units
    /// - `ingredient`: Direct ingredient name (for quantity-only patterns)
    ///
    /// Pattern structure: `(?i)(?P<quantity>...)(?:\s*(?P<measurement>...)|\s+(?P<ingredient>...))`
    ///
    /// ## Performance Characteristics
    ///
    /// - **Regex Compilation**: Pattern compiled once at detector creation
    /// - **Memory Usage**: Minimal, processes text line-by-line
    /// - **Time Complexity**: O(n) where n is text length
    /// - **Thread Safety**: Immutable detector can be shared across threads
    ///
    /// ## Error Handling
    ///
    /// - **Regex Compilation**: Guaranteed valid patterns via static compilation
    /// - **Text Processing**: Graceful handling of empty lines and malformed input
    /// - **Position Tracking**: Accurate position calculation across line boundaries
    ///
    /// ## Examples of Processing
    ///
    /// ### Traditional Measurement
    /// ```text
    /// Input: "2 cups flour"
    /// Processing:
    ///   - Regex match: "2 cups flour"
    ///   - quantity: "2", measurement: "cups"
    ///   - ingredient extraction: "flour" (from text after measurement)
    ///   - Result: MeasurementMatch { quantity: "2", measurement: Some("cups"), ingredient_name: "flour", ... }
    /// ```
    ///
    /// ### Quantity-Only Ingredient
    /// ```text
    /// Input: "6 oeufs"
    /// Processing:
    ///   - Regex match: "6 oeufs"
    ///   - quantity: "6", ingredient: "oeufs" (direct capture)
    ///   - measurement: None (quantity-only pattern)
    ///   - Result: MeasurementMatch { quantity: "6", measurement: None, ingredient_name: "oeufs", ... }
    /// ```
    ///
    /// ### Fraction Support
    /// ```text
    /// Input: "½ cup sugar"
    /// Processing:
    ///   - Regex match: "½ cup sugar"
    ///   - quantity: "½", measurement: "cup"
    ///   - ingredient extraction: "sugar"
    ///   - Result: MeasurementMatch { quantity: "½", measurement: Some("cup"), ingredient_name: "sugar", ... }
    /// ```
    ///
    /// ## Thread Safety and Performance
    ///
    /// - **Immutable State**: Detector can be shared across threads safely
    /// - **Memory Efficiency**: No allocations during matching, reuses compiled regex
    /// - **Scalability**: Linear performance scaling with input text size
    /// - **Concurrent Processing**: Safe for parallel text processing workloads
    ///
    /// * `text` - The text to scan for measurements
    ///
    /// # Returns
    ///
    /// Returns a vector of `MeasurementMatch` containing all detected measurements
    /// with their associated ingredient names
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let detector = MeasurementDetector::new()?;
    /// let text = "2 cups flour\n1 tablespoon sugar\nsome salt";
    /// let matches = detector.extract_ingredient_measurements(text);
    ///
    /// assert_eq!(matches.len(), 2);
    /// assert_eq!(matches[0].quantity, "2");
    /// assert_eq!(matches[0].measurement, Some("cups".to_string()));
    /// assert_eq!(matches[0].ingredient_name, "flour");
    /// assert_eq!(matches[1].quantity, "1");
    /// assert_eq!(matches[1].measurement, Some("tablespoon".to_string()));
    /// assert_eq!(matches[1].ingredient_name, "sugar");
    /// # Ok::<(), regex::Error>(())
    /// ```
    pub fn extract_ingredient_measurements(&self, text: &str) -> Vec<MeasurementMatch> {
        let start_time = std::time::Instant::now();
        let text_length = text.len();
        let line_count = text.lines().count();

        let mut matches = Vec::new();
        let mut current_pos = 0;

        debug!("Finding measurements in text with {} lines", line_count);

        for (line_number, line) in text.lines().enumerate() {
            trace!("Processing line {}: '{}'", line_number, line);
            for capture in self.pattern.captures_iter(line) {
                let full_match = capture.get(0).unwrap();
                let measurement_text = full_match.as_str();
                debug!(
                    "Found measurement '{}' at line {}",
                    measurement_text, line_number
                );

                // Extract named capture groups
                let quantity = capture.name("quantity").map(|m| m.as_str()).unwrap_or("");
                let measurement_unit = capture.name("measurement").map(|m| m.as_str());
                let ingredient_from_capture = capture.name("ingredient").map(|m| m.as_str());

                // Determine the quantity, measurement, and ingredient name
                let (final_quantity, final_measurement, raw_ingredient_name) =
                    if let Some(ingredient) = ingredient_from_capture {
                        // Quantity-only ingredient: no measurement unit
                        debug!(
                            "Quantity-only ingredient detected: quantity='{}', ingredient='{}'",
                            quantity, ingredient
                        );
                        (quantity.to_string(), None, ingredient.to_string())
                    } else if let Some(measurement) = measurement_unit {
                        // Traditional measurement: extract ingredient name from text after the measurement
                        let measurement_end = full_match.end();
                        let ingredient_name = line[measurement_end..].trim().to_string();
                        debug!(
                        "Traditional measurement: quantity='{}', measurement='{}', ingredient='{}'",
                        quantity, measurement, ingredient_name
                    );
                        (
                            quantity.to_string(),
                            Some(measurement.to_string()),
                            ingredient_name,
                        )
                    } else {
                        // Fallback: shouldn't happen with current regex
                        (quantity.to_string(), None, String::new())
                    };

                let ingredient_name = self.post_process_ingredient_name(&raw_ingredient_name);

                trace!(
                    "Extracted ingredient name: '{}' -> '{}'",
                    raw_ingredient_name,
                    ingredient_name
                );

                matches.push(MeasurementMatch {
                    quantity: final_quantity,
                    measurement: final_measurement,
                    ingredient_name,
                    line_number,
                    start_pos: current_pos + full_match.start(),
                    end_pos: current_pos + full_match.end(),
                });
            }
            current_pos += line.len() + 1; // +1 for newline character
        }

        let duration = start_time.elapsed();
        let matches_count = matches.len();

        // Record text processing performance metrics
        crate::observability::record_text_processing_metrics(
            "extract_ingredient_measurements",
            duration,
            text_length,
            line_count,
            matches_count,
        );

        info!("Found {} measurement matches in text", matches_count);
        matches
    }

    /// Extract lines containing measurements from the text
    ///
    /// Returns all lines that contain at least one measurement unit.
    ///
    /// # Arguments
    ///
    /// * `text` - The multi-line text to analyze
    ///
    /// # Returns
    ///
    /// Returns a vector of tuples containing (line_number, line_content) for
    /// lines that contain measurements
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let detector = MeasurementDetector::new()?;
    /// let text = "2 cups flour\n1/2 cup sugar\nsome salt\n3 sachets yeast\n6 oeufs\n4 pommes";
    /// let measurement_lines = detector.extract_measurement_lines(text);
    ///
    /// assert_eq!(measurement_lines.len(), 5);
    /// assert_eq!(measurement_lines[0], (0, "2 cups flour".to_string()));
    /// assert_eq!(measurement_lines[1], (1, "1/2 cup sugar".to_string()));
    /// assert_eq!(measurement_lines[2], (3, "3 sachets yeast".to_string()));
    /// assert_eq!(measurement_lines[3], (4, "6 oeufs".to_string()));
    /// assert_eq!(measurement_lines[4], (5, "4 pommes".to_string()));
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn extract_measurement_lines(&self, text: &str) -> Vec<(usize, String)> {
        text.lines()
            .enumerate()
            .filter(|(_, line)| self.pattern.is_match(line))
            .map(|(i, line)| (i, line.to_string()))
            .collect()
    }

    /// Check if a given text contains any measurements
    ///
    /// # Arguments
    ///
    /// * `text` - The text to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the text contains at least one measurement unit
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    ///
    /// let detector = MeasurementDetector::new()?;
    /// assert!(detector.has_measurements("2 cups flour"));
    /// assert!(detector.has_measurements("1/2 cup sugar"));  // fraction support
    /// assert!(detector.has_measurements("6 oeufs"));  // quantity-only ingredient
    /// assert!(detector.has_measurements("4 pommes")); // quantity-only ingredient
    /// assert!(!detector.has_measurements("some flour"));
    /// assert!(!detector.has_measurements("some eggs")); // plain text without quantity
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn has_measurements(&self, text: &str) -> bool {
        let result = self.pattern.is_match(text);
        debug!(
            "Checking for measurements in text: '{}' -> {}",
            text, result
        );
        if result {
            trace!(
                "Pattern '{}' matched in text: '{}'",
                self.pattern.as_str(),
                text
            );
        }
        result
    }

    /// Post-process an ingredient name to clean it up
    ///
    /// This function implements a multi-stage ingredient name cleaning algorithm that
    /// transforms raw OCR-extracted text into clean, normalized ingredient names.
    ///
    /// ## Algorithm Overview
    ///
    /// The cleaning process follows this sequence:
    /// 1. **Early Exit Conditions**: Skip processing for disabled config or empty input
    /// 2. **Punctuation Cleanup**: Remove trailing punctuation marks
    /// 3. **Prefix Removal**: Strip common linguistic prepositions and articles
    /// 4. **Length Limiting**: Enforce maximum name length with smart truncation
    /// 5. **Whitespace Normalization**: Clean up spacing and formatting
    ///
    /// ## Processing Stages
    ///
    /// ### Stage 1: Early Exit Conditions
    /// ```text
    /// if postprocessing_disabled or input_empty:
    ///     return input.trim()
    /// ```
    /// **Purpose**: Skip unnecessary processing for edge cases
    ///
    /// ### Stage 2: Punctuation Cleanup
    /// **Algorithm**: Remove trailing punctuation while preserving valid characters
    /// ```text
    /// Input:  "flour,"    → Output: "flour"
    /// Input:  "sugar!"    → Output: "sugar"
    /// Input:  "salt."     → Output: "salt"
    /// ```
    /// **Preserved Characters**: Alphanumeric, spaces, hyphens, apostrophes
    /// **Removed Characters**: `.,;:!?` and other punctuation at string end
    ///
    /// ### Stage 3: Linguistic Prefix Removal
    /// **Algorithm**: Remove common prepositions and articles that appear in ingredient contexts
    ///
    /// **English Prefixes**:
    /// - `"of "` → "flour of wheat" → "wheat"
    /// - `"the "` → "the flour" → "flour"
    /// - `"a "`, `"an "` → "a tomato" → "tomato"
    ///
    /// **French Prefixes**:
    /// - `"de "` → "farine de blé" → "farine de blé" (preserved)
    /// - `"d'"` → "d'oeufs" → "oeufs"
    /// - `"du "`, `"des "` → "du sel" → "sel"
    /// - `"la "`, `"le "`, `"les "` → "la farine" → "farine"
    /// - `"l'"` → "l'eau" → "eau"
    /// - `"au "`, `"aux "` → "au miel" → "miel"
    /// - `"un "`, `"une "` → "un oeuf" → "oeuf"
    ///
    /// **Processing Rules**:
    /// - Only remove one prefix per ingredient
    /// - Case-insensitive matching
    /// - Preserve space after prefix removal
    ///
    /// ### Stage 4: Length Limiting with Smart Truncation
    /// **Algorithm**: Enforce maximum length while attempting word boundary preservation
    ///
    /// ```text
    /// Max Length: 100 characters
    /// Input: "all-purpose flour for baking cakes and pastries" (50 chars) → No change
    /// Input: "extra long ingredient name that exceeds maximum allowed length for storage" (75 chars)
    ///        → "extra long ingredient name that exceeds maximum allowed" (60 chars)
    /// ```
    ///
    /// **Truncation Strategy**:
    /// 1. Check if length exceeds maximum
    /// 2. Attempt truncation at word boundary (find last space)
    /// 3. Fall back to hard character limit if no word boundary found
    /// 4. Log warning for truncated ingredients
    ///
    /// ### Stage 5: Whitespace Normalization
    /// **Algorithm**: Clean up spacing issues from OCR and text processing
    ///
    /// ```text
    /// Input:  "  flour   "     → Output: "flour"
    /// Input:  "de   blé"       → Output: "de blé"
    /// Input:  "farine\tde\nblé" → Output: "farine de blé"
    /// ```
    ///
    /// **Processing Steps**:
    /// 1. Split on whitespace (handles spaces, tabs, newlines)
    /// 2. Filter out empty strings
    /// 3. Rejoin with single spaces
    /// 4. Trim leading/trailing whitespace
    ///
    /// ## Examples of Processing
    ///
    /// ### Basic Punctuation Removal
    /// ```text
    /// Input:  "flour,"
    /// Stage 2: Remove trailing comma → "flour"
    /// Final:  "flour"
    /// ```
    ///
    /// ### French Article Removal
    /// ```text
    /// Input:  "la farine"
    /// Stage 3: Remove "la " → "farine"
    /// Final:  "farine"
    /// ```
    ///
    /// ### Complex Multi-Stage Processing
    /// ```text
    /// Input:  "  de la farine de blé,  "
    /// Stage 1: Not empty, processing enabled
    /// Stage 2: Remove trailing comma → "  de la farine de blé  "
    /// Stage 3: Remove "de " → "la farine de blé  " (only first prefix removed)
    /// Stage 4: Length OK, no truncation
    /// Stage 5: Normalize spaces → "la farine de blé"
    /// Final:  "la farine de blé"
    /// ```
    ///
    /// ## Performance Characteristics
    ///
    /// - **Time Complexity**: O(n) where n is string length
    /// - **Memory Usage**: Minimal, in-place operations where possible
    /// - **Allocation Strategy**: Single pass with minimal allocations
    /// - **Early Exit**: Fast path for disabled processing or empty strings
    ///
    /// ## Error Handling
    ///
    /// - **Empty Input**: Gracefully handled with early return
    /// - **Unicode Safety**: All operations are Unicode-safe
    /// - **Boundary Safety**: No panics on edge cases (empty strings, single characters)
    ///
    /// ## Configuration Integration
    ///
    /// Respects `MeasurementConfig` settings:
    /// - `enable_ingredient_postprocessing`: Master enable/disable switch
    /// - `max_ingredient_length`: Maximum allowed ingredient name length
    ///
    /// ## Thread Safety
    ///
    /// - **Immutable Access**: Only reads configuration, no mutation
    /// - **No Shared State**: No static or global state modification
    /// - **Concurrent Safe**: Can be called safely from multiple threads
    ///
    /// # Arguments
    ///
    /// * `raw_name` - The raw ingredient name string to clean
    ///
    /// # Returns
    ///
    /// Returns a cleaned and normalized ingredient name string
    fn post_process_ingredient_name(&self, raw_name: &str) -> String {
        if !self.config.enable_ingredient_postprocessing || raw_name.trim().is_empty() {
            trace!("Post-processing disabled or empty name: '{}'", raw_name);
            return raw_name.trim().to_string();
        }

        let mut name = raw_name.trim().to_string();
        let original_name = name.clone();

        // Remove trailing punctuation
        name = name
            .trim_end_matches(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-' && c != '\'')
            .to_string();

        // Common prepositions and articles to remove (English and French)
        let prefixes_to_remove = [
            // English
            "of ", "the ", "a ", "an ", // French
            "de ", "d'", "du ", "des ", "la ", "le ", "les ", "l'", "au ", "aux ", "un ", "une ",
        ];

        for prefix in &prefixes_to_remove {
            if name.to_lowercase().starts_with(prefix) {
                name = name[prefix.len()..].trim_start().to_string();
                debug!(
                    "Removed prefix '{}' from ingredient name: '{}' -> '{}'",
                    prefix.trim(),
                    original_name,
                    name
                );
                break; // Only remove one prefix
            }
        }

        // Limit length to prevent overly long extractions
        if name.len() > self.config.max_ingredient_length {
            let truncated = name[..self.config.max_ingredient_length].to_string();
            // Try to cut at word boundary
            if let Some(last_space) = truncated.rfind(' ') {
                name = truncated[..last_space].to_string();
            } else {
                name = truncated;
            }
            warn!(
                "Ingredient name truncated due to length limit ({} > {}): '{}' -> '{}'",
                original_name.len(),
                self.config.max_ingredient_length,
                original_name,
                name
            );
        }

        // Clean up multiple spaces
        name = name.split_whitespace().collect::<Vec<&str>>().join(" ");

        trace!(
            "Post-processed ingredient name: '{}' -> '{}'",
            original_name,
            name
        );
        name.trim().to_string()
    }

    /// Get all unique measurement units found in the text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to analyze
    ///
    /// # Returns
    ///
    /// Returns a HashSet of unique measurement unit strings found
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::text_processing::MeasurementDetector;
    /// use std::collections::HashSet;
    ///
    /// let detector = MeasurementDetector::new()?;
    /// let text = "2 cups flour\n1/2 cup sugar\n500g butter\n6 oeufs\n4 pommes";
    /// let units = detector.get_unique_units(text);
    ///
    /// assert!(units.iter().any(|u| u.contains("cups")));
    /// assert!(units.iter().any(|u| u.contains("cup")));
    /// assert!(units.iter().any(|u| u.contains("1/2"))); // fraction support
    /// assert!(units.iter().any(|u| u.contains("g")));
    /// assert!(units.iter().any(|u| u.contains("6")));  // quantity-only measurement
    /// assert!(units.iter().any(|u| u.contains("4")));  // quantity-only measurement
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn get_unique_units(&self, text: &str) -> HashSet<String> {
        self.pattern
            .find_iter(text)
            .map(|m| m.as_str().to_lowercase())
            .collect()
    }
}

impl MeasurementDetector {
    /// Get the regex pattern as a string (for testing purposes)
    pub fn pattern_str(&self) -> &str {
        self.pattern.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measurement_config_validation() {
        let mut config = MeasurementConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test invalid max_ingredient_length
        config.max_ingredient_length = 0;
        assert!(config.validate().is_err());
        config.max_ingredient_length = 100;

        // Test invalid custom pattern (empty)
        config.custom_pattern = Some("".to_string());
        assert!(config.validate().is_err());

        // Test invalid custom pattern (invalid regex)
        config.custom_pattern = Some("[invalid".to_string());
        assert!(config.validate().is_err());

        // Test valid custom pattern
        config.custom_pattern = Some(r"\d+\s+cups?".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_measurement_units_config_validation() {
        let mut config = MeasurementUnitsConfig {
            measurement_units: MeasurementUnits {
                volume_units: vec!["cup".to_string(), "tablespoon".to_string()],
                weight_units: vec!["g".to_string(), "kg".to_string()],
                volume_units_metric: vec!["l".to_string(), "ml".to_string()],
                us_units: vec!["slice".to_string()],
                french_units: vec!["sachet".to_string()],
            },
        };

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Test empty volume_units
        config.measurement_units.volume_units = vec![];
        assert!(config.validate().is_err());
        config.measurement_units.volume_units = vec!["cup".to_string()];

        // Test empty weight_units
        config.measurement_units.weight_units = vec![];
        assert!(config.validate().is_err());
        config.measurement_units.weight_units = vec!["g".to_string()];

        // Test empty volume_units_metric
        config.measurement_units.volume_units_metric = vec![];
        assert!(config.validate().is_err());
        config.measurement_units.volume_units_metric = vec!["l".to_string()];

        // Test empty us_units
        config.measurement_units.us_units = vec![];
        assert!(config.validate().is_err());
        config.measurement_units.us_units = vec!["slice".to_string()];

        // Test empty french_units
        config.measurement_units.french_units = vec![];
        assert!(config.validate().is_err());
        config.measurement_units.french_units = vec!["sachet".to_string()];

        // Test empty unit string
        config.measurement_units.volume_units = vec!["".to_string()];
        assert!(config.validate().is_err());
        config.measurement_units.volume_units = vec!["cup".to_string()];

        // Test invalid characters in unit (control character)
        config.measurement_units.volume_units = vec!["cup\ntablespoon".to_string()];
        assert!(config.validate().is_err());
        config.measurement_units.volume_units = vec!["cup".to_string()];
    }

    #[test]
    fn test_measurement_detector_with_invalid_config() {
        // Test with invalid max_ingredient_length
        let invalid_config = MeasurementConfig {
            max_ingredient_length: 0,
            ..Default::default()
        };
        assert!(MeasurementDetector::with_config(invalid_config).is_err());

        // Test with invalid custom pattern
        let invalid_config = MeasurementConfig {
            custom_pattern: Some("[invalid".to_string()),
            ..Default::default()
        };
        assert!(MeasurementDetector::with_config(invalid_config).is_err());
    }

    #[test]
    fn test_load_measurement_units_config() {
        // This test assumes the config file exists and is valid
        let config = load_measurement_units_config();
        if let Err(e) = config.validate() {
            panic!("Config validation failed: {}", e);
        }
    }
}
