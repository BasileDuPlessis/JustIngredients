//! # Image Preprocessing Module
//!
//! This module provides comprehensive image preprocessing functionality for OCR accuracy improvement.
//! It includes scaling, filtering, thresholding, quality assessment, and deskewing operations.
//!
//! The module is organized into focused sub-modules:
//! - `scaling`: Intelligent image scaling with OCR optimization
//! - `thresholding`: Binary thresholding using Otsu's method
//! - `filtering`: Noise reduction and morphological operations
//! - `quality`: Image quality assessment for adaptive preprocessing
//! - `deskewing`: Text rotation detection and correction
//! - `cropping`: Image cropping for targeted OCR regions
//! - `types`: Shared types and error definitions

pub mod cropping;
pub mod deskewing;
pub mod filtering;
pub mod quality;
pub mod scaling;
pub mod thresholding;
pub mod types;

// Re-export commonly used types and functions for convenience
pub use types::{
    ClaheImageResult, CroppedImageResult, DenoisedImageResult, DeskewResult, ImageQuality,
    ImageQualityResult, MorphologicalImageResult, MorphologicalOperation, PreprocessingError,
    ScaledImageResult, ThresholdedImageResult,
};

// Re-export main functions from sub-modules
pub use cropping::crop_measurement_region;
pub use deskewing::deskew_image;
pub use filtering::{apply_clahe, apply_morphological_operation, reduce_noise};
pub use quality::assess_image_quality;
pub use scaling::ImageScaler;
pub use thresholding::apply_otsu_threshold;
