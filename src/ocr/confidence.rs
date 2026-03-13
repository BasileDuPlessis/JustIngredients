//! Confidence scoring for OCR results
//!
//! This module provides confidence scoring functionality for OCR results,
//! including text quality analysis, pattern recognition, and processing time evaluation.

use crate::ocr_config::OcrConfig;

/// Confidence score for OCR results
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcrConfidence {
    /// Overall confidence score (0.0 to 1.0)
    pub overall_score: f32,
    /// Text quality score based on length and content
    pub text_quality_score: f32,
    /// Pattern recognition score (presence of measurements, ingredients)
    pub pattern_score: f32,
    /// Processing time score (faster processing may indicate clearer text)
    pub processing_score: f32,
    /// Flags indicating potential issues
    pub flags: Vec<ConfidenceFlag>,
}

/// Flags indicating confidence issues
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConfidenceFlag {
    /// Very short text (may indicate failed OCR)
    TooShort,
    /// No measurements found (unusual for recipe images)
    NoMeasurements,
    /// Very long processing time (may indicate complex/difficult image)
    SlowProcessing,
    /// Text contains many non-alphanumeric characters (may indicate garbage)
    HighNoiseRatio,
    /// Text appears to be mostly numbers (may indicate failed OCR)
    MostlyNumeric,
    /// Tesseract's own confidence score is low
    LowTesseractConfidence,
}

/// Calculate confidence score for OCR results using Tesseract's confidence
pub fn calculate_ocr_confidence_with_tesseract(
    text: &str,
    tesseract_confidence: f32,
    processing_duration: std::time::Duration,
    config: &OcrConfig,
) -> OcrConfidence {
    let mut flags = Vec::new();

    // Text quality score (0.0 to 1.0)
    let text_quality_score = calculate_text_quality_score(text, &mut flags);

    // Pattern recognition score (0.0 to 1.0)
    let pattern_score = calculate_pattern_recognition_score(text, &mut flags);

    // Processing time score (0.0 to 1.0) - faster is generally better
    let processing_score = calculate_processing_time_score(processing_duration, config, &mut flags);

    // Tesseract confidence score (0-100, convert to 0.0-1.0)
    let tesseract_score = (tesseract_confidence / 100.0).clamp(0.0, 1.0);

    // If Tesseract confidence is very low, add a flag
    if tesseract_confidence < 50.0 {
        flags.push(ConfidenceFlag::LowTesseractConfidence);
    }

    // Overall score is weighted average including Tesseract confidence
    let overall_score = (
        tesseract_score * 0.5 +        // Tesseract's own confidence (50%)
        text_quality_score * 0.2 +     // Text quality analysis (20%)
        pattern_score * 0.2 +          // Pattern recognition (20%)
        processing_score * 0.1
        // Processing time (10%)
    )
    .clamp(0.0, 1.0);

    OcrConfidence {
        overall_score,
        text_quality_score,
        pattern_score,
        processing_score,
        flags,
    }
}

/// Calculate text quality score based on length, content, and noise
fn calculate_text_quality_score(text: &str, flags: &mut Vec<ConfidenceFlag>) -> f32 {
    let trimmed = text.trim();

    // Very short text is suspicious
    if trimmed.len() < 10 {
        flags.push(ConfidenceFlag::TooShort);
        return 0.2;
    }

    // Count alphanumeric characters vs total
    let total_chars = trimmed.chars().count() as f32;
    let alpha_numeric_chars = trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count() as f32;

    let noise_ratio = if total_chars > 0.0 {
        alpha_numeric_chars / total_chars
    } else {
        0.0
    };

    // High noise ratio indicates potential garbage text
    if noise_ratio < 0.7 {
        flags.push(ConfidenceFlag::HighNoiseRatio);
    }

    // Count numeric characters
    let numeric_chars = trimmed.chars().filter(|c| c.is_ascii_digit()).count() as f32;

    let numeric_ratio = if total_chars > 0.0 {
        numeric_chars / total_chars
    } else {
        0.0
    };

    // Mostly numeric text may indicate failed OCR
    if numeric_ratio > 0.8 {
        flags.push(ConfidenceFlag::MostlyNumeric);
    }

    // Base score from noise ratio, penalized for being mostly numeric
    let base_score = noise_ratio;
    let numeric_penalty = if numeric_ratio > 0.8 { 0.3 } else { 0.0 };

    (base_score - numeric_penalty).clamp(0.0, 1.0)
}

/// Calculate pattern recognition score based on expected recipe content
fn calculate_pattern_recognition_score(text: &str, flags: &mut Vec<ConfidenceFlag>) -> f32 {
    // Check for measurement patterns
    let measurement_patterns = [
        r"\d+\s*(?:cup|cups|tablespoon|tablespoons|teaspoon|teaspoons|tbsp|tsp|g|kg|ml|l)",
        r"\d+/\d+\s*(?:cup|cups|tablespoon|tablespoons|teaspoon|teaspoons|tbsp|tsp)",
        r"\d+[½⅓⅔¼¾⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⅟]\s*(?:cup|cups|tablespoon|tablespoons|teaspoon|teaspoons)",
    ];

    let mut measurement_count = 0;
    for pattern in &measurement_patterns {
        if let Ok(regex) = regex::Regex::new(pattern) {
            measurement_count += regex.find_iter(text).count();
        }
    }

    // Check for ingredient-like words
    let ingredient_indicators = [
        "flour", "sugar", "salt", "butter", "milk", "eggs", "oil", "water", "farine", "sucre",
        "sel", "beurre", "lait", "œufs", "huile", "eau",
    ];

    let ingredient_count = ingredient_indicators
        .iter()
        .filter(|&word| text.to_lowercase().contains(word))
        .count();

    // No measurements found is suspicious for recipe images
    if measurement_count == 0 {
        flags.push(ConfidenceFlag::NoMeasurements);
    }

    // Score based on presence of expected patterns
    let measurement_score = if measurement_count > 0 {
        (measurement_count as f32 / 5.0).min(1.0) // Cap at 5 measurements
    } else {
        0.0
    };

    let ingredient_score = if ingredient_count > 0 {
        (ingredient_count as f32 / 3.0).min(1.0) // Cap at 3 ingredients
    } else {
        0.0
    };

    // Weighted average
    (measurement_score * 0.6) + (ingredient_score * 0.4)
}

/// Calculate processing time score (faster processing generally indicates clearer text)
fn calculate_processing_time_score(
    duration: std::time::Duration,
    config: &OcrConfig,
    flags: &mut Vec<ConfidenceFlag>,
) -> f32 {
    let processing_ms = duration.as_millis() as f32;

    // Define expected processing time ranges (in milliseconds)
    let fast_threshold = 1000.0; // 1 second
    let slow_threshold = config.recovery.operation_timeout_secs as f32 * 1000.0 * 0.8; // 80% of timeout

    // Very slow processing may indicate difficult image
    if processing_ms > slow_threshold {
        flags.push(ConfidenceFlag::SlowProcessing);
    }

    // Score: 1.0 for fast processing, decreasing to 0.0 for slow processing
    if processing_ms <= fast_threshold {
        1.0
    } else if processing_ms >= slow_threshold {
        0.0
    } else {
        // Linear interpolation between fast and slow thresholds
        let range = slow_threshold - fast_threshold;
        let distance_from_fast = processing_ms - fast_threshold;
        1.0 - (distance_from_fast / range).clamp(0.0, 1.0)
    }
}

/// Check if OCR confidence indicates low quality results that need review
pub fn should_flag_for_review(confidence: &OcrConfidence, threshold: f32) -> bool {
    confidence.overall_score < threshold || !confidence.flags.is_empty()
}

/// Get human-readable description of confidence issues
pub fn get_confidence_description(confidence: &OcrConfidence) -> String {
    if confidence.flags.is_empty() && confidence.overall_score >= 0.7 {
        return "High confidence OCR result".to_string();
    }

    let mut issues = Vec::new();

    for flag in &confidence.flags {
        let description = match flag {
            ConfidenceFlag::TooShort => "Text is very short (may indicate failed OCR)",
            ConfidenceFlag::NoMeasurements => "No measurements found (unusual for recipes)",
            ConfidenceFlag::SlowProcessing => "Slow processing time (may indicate difficult image)",
            ConfidenceFlag::HighNoiseRatio => "High proportion of non-alphanumeric characters",
            ConfidenceFlag::MostlyNumeric => "Text is mostly numeric (may indicate failed OCR)",
            ConfidenceFlag::LowTesseractConfidence => "Tesseract confidence score is low",
        };
        issues.push(description);
    }

    if confidence.overall_score < 0.5 {
        issues.push("Low overall confidence score");
    }

    format!("OCR result flagged for review: {}", issues.join(", "))
}