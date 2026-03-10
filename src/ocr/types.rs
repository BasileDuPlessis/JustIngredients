//! # OCR Types Module
//!
//! Shared data structures and types for OCR processing.

use serde::{Deserialize, Serialize};

/// Represents a bounding box with coordinates for spatial positioning
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BBox {
    /// Left coordinate (x0)
    pub x0: u32,
    /// Top coordinate (y0)
    pub y0: u32,
    /// Right coordinate (x1)
    pub x1: u32,
    /// Bottom coordinate (y1)
    pub y1: u32,
}

impl BBox {
    /// Create a new bounding box
    pub fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Calculate the width of the bounding box
    pub fn width(&self) -> u32 {
        self.x1.saturating_sub(self.x0)
    }

    /// Calculate the height of the bounding box
    pub fn height(&self) -> u32 {
        self.y1.saturating_sub(self.y0)
    }

    /// Calculate the area of the bounding box
    pub fn area(&self) -> u64 {
        (self.width() as u64) * (self.height() as u64)
    }
}

/// Represents a line of text with its bounding box from HOCR output
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HocrLine {
    /// The text content of the line
    pub text: String,
    /// The bounding box coordinates for this line
    pub bbox: BBox,
}

impl HocrLine {
    /// Create a new HOCR line
    pub fn new(text: String, bbox: BBox) -> Self {
        Self { text, bbox }
    }

    /// Create a new HOCR line from coordinates
    pub fn from_coords(text: String, x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Self {
            text,
            bbox: BBox::new(x0, y0, x1, y1),
        }
    }
}

/// Result of adaptive image preprocessing.
#[derive(Debug)]
pub struct AdaptivePreprocessingResult {
    /// The final preprocessed image
    pub image: image::DynamicImage,
    /// Description of the preprocessing strategy applied
    pub preprocessing_strategy: String,
}

/// Result of constrained OCR processing for measurement extraction.
#[derive(Debug)]
pub struct ConstrainedOcrResult {
    /// The extracted text (should contain only numbers/fractions)
    pub text: String,
    /// Confidence score from Tesseract (0-100)
    pub confidence: f32,
    /// PSM mode used for OCR
    pub psm_mode: String,
    /// Character whitelist applied
    pub character_whitelist: String,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}
