//! # Shared Types for Image Preprocessing
//!
//! This module contains all the shared types, structs, and enums used across
//! the preprocessing sub-modules.

use image::DynamicImage;

/// Errors that can occur during image preprocessing operations.
#[derive(Debug, Clone)]
pub enum PreprocessingError {
    /// Invalid target height specified
    InvalidTargetHeight { height: u32 },
    /// Image processing operation failed
    ProcessingFailed { message: String },
    /// Failed to load or decode image
    ImageLoad { message: String },
}

impl std::fmt::Display for PreprocessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreprocessingError::InvalidTargetHeight { height } => {
                write!(
                    f,
                    "Invalid target height: {}. Must be between 20 and 35 pixels",
                    height
                )
            }
            PreprocessingError::ProcessingFailed { message } => {
                write!(f, "Image processing failed: {}", message)
            }
            PreprocessingError::ImageLoad { message } => {
                write!(f, "Failed to load image: {}", message)
            }
        }
    }
}

impl std::error::Error for PreprocessingError {}

/// Result of an OCR-optimized scaling operation.
#[derive(Debug, Clone)]
pub struct ScaledImageResult {
    /// The scaled image
    pub image: DynamicImage,
    /// Original image dimensions (width, height)
    pub original_dimensions: (u32, u32),
    /// New image dimensions (width, height)
    pub new_dimensions: (u32, u32),
    /// Scale factor applied
    pub scale_factor: f32,
    /// Estimated text height in original image
    pub estimated_text_height: u32,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of image thresholding operation.
#[derive(Debug, Clone)]
pub struct ThresholdedImageResult {
    /// The thresholded binary image
    pub image: DynamicImage,
    /// Optimal threshold value found by Otsu's method
    pub threshold: u8,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of image noise reduction operation.
#[derive(Debug, Clone)]
pub struct DenoisedImageResult {
    /// The denoised image
    pub image: DynamicImage,
    /// Sigma value used for Gaussian blur
    pub sigma: f32,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of morphological operations on binary images.
#[derive(Debug, Clone)]
pub struct MorphologicalImageResult {
    /// The morphologically processed image
    pub image: DynamicImage,
    /// Type of morphological operation applied
    pub operation: MorphologicalOperation,
    /// Kernel size used (e.g., 3 for 3x3 kernel)
    pub kernel_size: u32,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of image quality assessment.
#[derive(Debug, Clone)]
pub struct ImageQualityResult {
    /// Overall quality classification
    pub quality: ImageQuality,
    /// Contrast ratio (0.0-1.0, higher is better)
    pub contrast_ratio: f32,
    /// Brightness level (0.0-1.0, 0.5 is optimal)
    pub brightness: f32,
    /// Sharpness score (0.0-1.0, higher is sharper)
    pub sharpness: f32,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Image quality classifications.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ImageQuality {
    /// High quality - minimal preprocessing needed
    High,
    /// Medium quality - standard preprocessing recommended
    Medium,
    /// Low quality - full preprocessing pipeline needed
    Low,
}

/// Types of morphological operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MorphologicalOperation {
    /// Erosion operation (shrinks bright regions)
    Erosion,
    /// Dilation operation (expands bright regions)
    Dilation,
    /// Opening operation (erosion followed by dilation - removes noise)
    Opening,
    /// Closing operation (dilation followed by erosion - fills gaps)
    Closing,
}

/// Result of deskewing operation.
#[derive(Debug, Clone)]
pub struct DeskewResult {
    /// The deskewed image
    pub image: DynamicImage,
    /// Detected skew angle in degrees
    pub skew_angle_degrees: f32,
    /// Confidence in the deskewing result (0.0-1.0)
    pub confidence: f32,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of CLAHE contrast enhancement operation.
#[derive(Debug, Clone)]
pub struct ClaheImageResult {
    /// The contrast-enhanced image
    pub image: DynamicImage,
    /// Clip limit used for histogram clipping
    pub clip_limit: f32,
    /// Tile size used for local histogram equalization
    pub tile_size: (u32, u32),
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of image cropping operation.
#[derive(Debug, Clone)]
pub struct CroppedImageResult {
    /// The cropped image
    pub image: DynamicImage,
    /// Original bounding box coordinates
    pub original_bbox: crate::ocr::BBox,
    /// Cropped region coordinates relative to original image
    pub cropped_region: crate::ocr::BBox,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}

/// Result of targeted preprocessing operation for measurement regions.
#[derive(Debug, Clone)]
pub struct TargetedPreprocessingResult {
    /// The preprocessed image ready for OCR
    pub image: DynamicImage,
    /// Original image dimensions before preprocessing
    pub original_dimensions: (u32, u32),
    /// Final image dimensions after preprocessing
    pub final_dimensions: (u32, u32),
    /// Scale factor applied during upscaling
    pub scale_factor: f32,
    /// Threshold value used for binarization
    pub threshold: u8,
    /// Processing time in milliseconds
    pub processing_time_ms: u32,
}
