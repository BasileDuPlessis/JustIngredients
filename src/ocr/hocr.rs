//! HOCR (HTML-based OCR) processing utilities
//!
//! This module provides functionality for extracting and processing HOCR output
//! from Tesseract OCR, including spatial text positioning and HTML structure validation.

use regex;
use tracing::{error, warn};

use crate::ocr_errors::OcrError;

use super::types::HocrLine;

/// Extract HOCR text from an image file using OCR with comprehensive error handling
///
/// This function implements HOCR extraction with circuit breaker protection and
/// fallback mechanisms for robust operation.
///
/// # Arguments
///
/// * `image_path` - Path to the image file to process
/// * `config` - OCR configuration parameters
/// * `instance_manager` - OCR instance manager for Tesseract access
/// * `circuit_breaker` - Circuit breaker for fault tolerance
///
/// # Returns
/// Returns `Result<String, OcrError>` containing the HOCR HTML output or an error
///
/// # Errors
/// Returns `OcrError` if image processing fails, OCR fails, or circuit breaker is open
pub async fn extract_hocr_from_image(
    image_path: &str,
    config: &super::OcrConfig,
    instance_manager: &super::OcrInstanceManager,
    circuit_breaker: &super::CircuitBreaker,
) -> Result<String, OcrError> {
    // Check circuit breaker state first
    let circuit_open = circuit_breaker
        .is_open()
        .map_err(|e| OcrError::Extraction(format!("Circuit breaker state check failed: {}", e)))?;
    if circuit_open {
        warn!(
            "Circuit breaker is open, skipping HOCR extraction for {}",
            image_path
        );
        return Err(OcrError::Extraction("Circuit breaker is open".to_string()));
    }

    // Validate image path and format
    super::validate_image_path(image_path, config)
        .map_err(|e| OcrError::Validation(format!("Image validation failed: {}", e)))?;

    // Validate image format and size limits
    super::validate_image_with_format_limits(image_path, config)?;

    // Get OCR instance from pool
    let instance = instance_manager
        .get_instance(config)
        .map_err(|e| OcrError::Initialization(format!("Failed to get OCR instance: {}", e)))?;

    // Apply timeout to the entire HOCR extraction process
    let timeout_duration = std::time::Duration::from_secs(config.recovery.operation_timeout_secs);

    let result = match tokio::time::timeout(timeout_duration, async {
        // Get mutable access to the OCR instance
        let mut tess = instance.lock().map_err(|e| {
            OcrError::Extraction(format!("Failed to acquire OCR instance lock: {}", e))
        })?;

        // Set the image for OCR processing
        tess.set_image(image_path).map_err(|e| {
            OcrError::Extraction(format!("Failed to set image for HOCR processing: {}", e))
        })?;

        // Extract HOCR text with spatial information
        perform_hocr_extraction(&mut tess, image_path)
    })
    .await
    {
        Ok(inner_result) => inner_result,
        Err(_) => {
            error!("HOCR extraction timed out for {}", image_path);
            circuit_breaker.record_failure().map_err(|e| {
                OcrError::Extraction(format!("Failed to record circuit breaker failure: {}", e))
            })?;
            return Err(OcrError::Timeout("HOCR extraction timed out".to_string()));
        }
    };

    // Record success in circuit breaker
    circuit_breaker.record_success().map_err(|e| {
        OcrError::Extraction(format!("Failed to record circuit breaker success: {}", e))
    })?;

    result
}

/// Perform the actual HOCR extraction using leptess
///
/// Uses leptess::get_hocr_text() to extract OCR results in HOCR format,
/// which includes spatial positioning information for text elements.
/// Falls back to regular text extraction if HOCR generation fails.
fn perform_hocr_extraction(
    tess: &mut leptess::LepTess,
    image_path: &str,
) -> Result<String, OcrError> {
    // First attempt: Extract HOCR text with spatial information using leptess
    match tess.get_hocr_text(0) {
        Ok(hocr_text) => {
            // Validate the HOCR output is well-formed XML/HTML
            match validate_hocr_output(&hocr_text) {
                Ok(_) => return Ok(hocr_text),
                Err(validation_error) => {
                    warn!(
                        "HOCR validation failed: {}. Attempting fallback.",
                        validation_error
                    );
                    // Continue to fallback below
                }
            }
        }
        Err(hocr_error) => {
            warn!(
                "HOCR extraction failed: {}. Attempting fallback to regular text extraction.",
                hocr_error
            );
            // Continue to fallback below
        }
    }

    // Fallback: Generate basic HOCR structure from regular text extraction
    warn!("Using fallback HOCR generation for {}", image_path);
    generate_fallback_hocr(tess, image_path)
}

/// Generate a basic HOCR structure when direct HOCR extraction fails
///
/// Creates a minimal HOCR HTML document with the extracted text,
/// providing basic structure even when spatial information is unavailable.
fn generate_fallback_hocr(
    tess: &mut leptess::LepTess,
    image_path: &str,
) -> Result<String, OcrError> {
    // Extract regular text as fallback
    let text = tess
        .get_utf8_text()
        .map_err(|e| OcrError::Extraction(format!("Fallback text extraction failed: {}", e)))?;

    // Check that we got some text content
    if text.trim().is_empty() {
        return Err(OcrError::Extraction(
            "Both HOCR and fallback text extraction produced empty results".to_string(),
        ));
    }

    // Generate basic HOCR structure with the extracted text
    // This provides a minimal HTML document that can be parsed by HOCR consumers
    let hocr_html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>OCR Results for {}</title>
</head>
<body>
  <div class="ocr_page" id="page_1" title="bbox 0 0 1000 1000">
    <div class="ocr_carea" id="block_1_1" title="bbox 10 10 990 990">
      <p class="ocr_par" id="par_1_1" title="bbox 10 10 990 990">
        <span class="ocr_line" id="line_1_1" title="bbox 10 10 990 40">{}</span>
      </p>
    </div>
  </div>
</body>
</html>"#,
        image_path,
        text.trim()
    );

    // Validate the generated fallback HOCR
    validate_hocr_output(&hocr_html)?;

    Ok(hocr_html)
}

/// Validate that HOCR output is well-formed XML/HTML
///
/// Performs basic validation to ensure the HOCR output contains expected
/// HTML structure and elements. This helps catch malformed output early.
fn validate_hocr_output(hocr_text: &str) -> Result<(), OcrError> {
    // Check that output is not empty
    if hocr_text.trim().is_empty() {
        return Err(OcrError::Extraction("HOCR output is empty".to_string()));
    }

    // Check for basic HTML structure - should start with DOCTYPE or HTML tag
    if !hocr_text.trim_start().starts_with("<!DOCTYPE html>")
        && !hocr_text.trim_start().starts_with("<html")
        && !hocr_text.trim_start().starts_with("<?xml")
    {
        return Err(OcrError::Extraction(
            "HOCR output does not contain valid HTML/XML structure".to_string(),
        ));
    }

    // Check for closing HTML tag
    if !hocr_text.contains("</html>") {
        return Err(OcrError::Extraction(
            "HOCR output is missing closing HTML tag".to_string(),
        ));
    }

    // Check for basic HOCR structure - should contain ocr_page class
    if !hocr_text.contains("class=\"ocr_page\"") {
        return Err(OcrError::Extraction(
            "HOCR output is missing required ocr_page class".to_string(),
        ));
    }

    // Check for reasonable content length (not just HTML boilerplate)
    let content_length = hocr_text.len();
    if content_length < 200 {
        warn!(
            "HOCR output is suspiciously short ({} chars), may indicate processing issues",
            content_length
        );
    }

    Ok(())
}

/// Parse HOCR HTML content to extract text lines with their bounding boxes
///
/// Extracts all `<span class="ocr_line" title="bbox x0 y0 x1 y1">text</span>` elements
/// from HOCR output and returns them as a vector of `HocrLine` instances.
///
/// # Arguments
///
/// * `hocr_text` - The HOCR HTML content as a string
///
/// # Returns
///
/// Returns a `Result` containing a vector of `HocrLine` instances, or an `OcrError` if parsing fails.
pub fn parse_hocr_to_lines(hocr_text: &str) -> Result<Vec<HocrLine>, OcrError> {
    let mut lines = Vec::new();

    // Regex to match ocr_line spans with bbox attributes
    // Matches: <span class="ocr_line" ... title="bbox x0 y0 x1 y1">text</span>
    // Allows for optional attributes between class and title
    let line_regex = regex::Regex::new(
        r#"<span\s+class="ocr_line"[^>]*?title="bbox\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)">(.*?)</span>"#
    ).map_err(|e| OcrError::Extraction(format!("Failed to compile HOCR regex: {}", e)))?;

    // Find all matches in the HOCR text
    for capture in line_regex.captures_iter(hocr_text) {
        if let (Some(x0_match), Some(y0_match), Some(x1_match), Some(y1_match), Some(text_match)) = (
            capture.get(1),
            capture.get(2),
            capture.get(3),
            capture.get(4),
            capture.get(5),
        ) {
            // Parse coordinates
            let x0 = x0_match.as_str().parse::<u32>().map_err(|e| {
                OcrError::Extraction(format!("Failed to parse x0 coordinate: {}", e))
            })?;
            let y0 = y0_match.as_str().parse::<u32>().map_err(|e| {
                OcrError::Extraction(format!("Failed to parse y0 coordinate: {}", e))
            })?;
            let x1 = x1_match.as_str().parse::<u32>().map_err(|e| {
                OcrError::Extraction(format!("Failed to parse x1 coordinate: {}", e))
            })?;
            let y1 = y1_match.as_str().parse::<u32>().map_err(|e| {
                OcrError::Extraction(format!("Failed to parse y1 coordinate: {}", e))
            })?;

            // Extract and clean text content
            let text = html_decode_text(text_match.as_str());

            // Create HocrLine and add to results
            let hocr_line = HocrLine::from_coords(text, x0, y0, x1, y1);
            lines.push(hocr_line);
        }
    }

    Ok(lines)
}

/// Decode HTML entities and clean text content from HOCR spans
///
/// Performs basic HTML entity decoding and text cleaning for HOCR text content.
/// This handles common HTML entities and normalizes whitespace.
///
/// # Arguments
///
/// * `text` - The raw text content from HOCR span
///
/// # Returns
///
/// Returns the cleaned and decoded text content
fn html_decode_text(text: &str) -> String {
    // Basic HTML entity decoding
    let decoded = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Clean and normalize whitespace
    decoded.trim().to_string()
}

/// Map a MeasurementMatch to its corresponding bounding box from HOCR lines
///
/// Correlates a measurement match with its physical location on the image by finding
/// the HOCR line that corresponds to the measurement's line number.
///
/// # Arguments
///
/// * `measurement` - The measurement match to locate
/// * `hocr_lines` - Vector of parsed HOCR lines with bounding boxes
///
/// # Returns
///
/// Returns `Some(BBox)` if a matching line is found, `None` if no match is found
pub fn map_measurement_to_bbox(
    measurement: &crate::text_processing::MeasurementMatch,
    hocr_lines: &[HocrLine],
) -> Option<super::types::BBox> {
    // MeasurementMatch line_number is 1-based, but vector indexing is 0-based
    let line_index = measurement.line_number.saturating_sub(1);

    // Check bounds to avoid panic
    if line_index >= hocr_lines.len() {
        warn!(
            "Measurement line number {} is out of bounds for {} HOCR lines",
            measurement.line_number,
            hocr_lines.len()
        );
        return None;
    }

    // Get the HOCR line at the corresponding index
    let hocr_line = &hocr_lines[line_index];

    // Optional: Validate that the text content roughly matches
    // This provides additional confidence in the mapping
    if let Some(validation_result) = validate_measurement_line_match(measurement, hocr_line) {
        if !validation_result {
            warn!(
                "Measurement text '{}' does not match HOCR line text '{}' at line {}",
                measurement.quantity, hocr_line.text, measurement.line_number
            );
            // Continue with the mapping despite the mismatch
            // The line number is still the most reliable indicator
        }
    }

    Some(hocr_line.bbox.clone())
}

/// Validate that a measurement match corresponds to the expected HOCR line content
///
/// Performs validation to ensure that the measurement text appears in the
/// corresponding HOCR line, providing additional confidence in text-to-bbox mapping.
///
/// # Arguments
///
/// * `measurement` - The measurement match
/// * `hocr_line` - The corresponding HOCR line
///
/// # Returns
///
/// Returns `Some(true)` if validation passes, `Some(false)` if it fails,
/// Validate that a measurement match corresponds to the expected HOCR line content
///
/// Performs validation to ensure that the measurement text appears in the
/// corresponding HOCR line, providing additional confidence in text-to-bbox mapping.
///
/// # Arguments
///
/// * `measurement` - The measurement match
/// * `hocr_line` - The corresponding HOCR line
///
/// # Returns
///
/// Returns `Some(true)` if validation passes, `Some(false)` if it fails,
/// or `None` if validation cannot be performed
fn validate_measurement_line_match(
    measurement: &crate::text_processing::MeasurementMatch,
    hocr_line: &HocrLine,
) -> Option<bool> {
    // Extract the text around the measurement position
    let line_text = &hocr_line.text;
    let start_pos = measurement.start_pos;
    let end_pos = measurement.end_pos;

    // Check bounds
    if start_pos >= line_text.len() || end_pos > line_text.len() || start_pos >= end_pos {
        return Some(false);
    }

    // Extract the substring that should contain the measurement
    let measured_text = &line_text[start_pos..end_pos];

    // Check if the measured text contains the quantity
    // Allow for some OCR variations and whitespace differences
    let quantity_found = measured_text.contains(&measurement.quantity)
        || measured_text
            .replace(' ', "")
            .contains(&measurement.quantity.replace(' ', ""));

    if !quantity_found {
        return Some(false);
    }

    Some(true)
}
