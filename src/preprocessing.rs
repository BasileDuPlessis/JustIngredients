//! # Image Preprocessing Module
//!
//! This module provides image preprocessing functionality for OCR accuracy improvement.
//! It includes scaling, filtering, and other operations to optimize images before
//! text recognition with Tesseract.

use image::{DynamicImage, GenericImageView};

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

/// Configuration for image scaling operations.
#[derive(Debug, Clone)]
pub struct ImageScaler {
    /// Target character height in pixels for optimal OCR recognition.
    /// Recommended range: 20-35 pixels.
    target_char_height: u32,
}

impl ImageScaler {
    /// Default target character height for OCR optimization.
    const DEFAULT_TARGET_HEIGHT: u32 = 28;

    /// Minimum allowed target height.
    const MIN_TARGET_HEIGHT: u32 = 20;

    /// Maximum allowed target height.
    const MAX_TARGET_HEIGHT: u32 = 35;

    /// Creates a new ImageScaler with the default target height (28 pixels).
    ///
    /// # Examples
    ///
    /// ```
    /// use just_ingredients::preprocessing::ImageScaler;
    ///
    /// let scaler = ImageScaler::new();
    /// assert_eq!(scaler.target_char_height(), 28);
    /// ```
    pub fn new() -> Self {
        Self {
            target_char_height: Self::DEFAULT_TARGET_HEIGHT,
        }
    }

    /// Creates a new ImageScaler with a custom target height.
    ///
    /// # Arguments
    ///
    /// * `height` - Target character height in pixels (20-35).
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the ImageScaler or a `PreprocessingError`.
    ///
    /// # Examples
    ///
    /// ```
    /// use just_ingredients::preprocessing::ImageScaler;
    ///
    /// let scaler = ImageScaler::with_target_height(30).unwrap();
    /// assert_eq!(scaler.target_char_height(), 30);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `PreprocessingError::InvalidTargetHeight` if the height is outside the valid range.
    pub fn with_target_height(height: u32) -> Result<Self, PreprocessingError> {
        if !(Self::MIN_TARGET_HEIGHT..=Self::MAX_TARGET_HEIGHT).contains(&height) {
            return Err(PreprocessingError::InvalidTargetHeight { height });
        }

        Ok(Self {
            target_char_height: height,
        })
    }

    /// Returns the current target character height.
    pub fn target_char_height(&self) -> u32 {
        self.target_char_height
    }

    /// Scales an image to optimize it for OCR processing.
    ///
    /// This method applies cubic interpolation scaling to achieve the target character height.
    /// The scaling factor is calculated based on the estimated text height in the image.
    ///
    /// # Arguments
    ///
    /// * `image` - The input image to scale.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the scaled image or a `PreprocessingError`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use just_ingredients::preprocessing::ImageScaler;
    /// use image::open;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let scaler = ImageScaler::new();
    /// let img = open("recipe.jpg")?;
    /// let scaled = scaler.scale(&img)?;
    /// // scaled image is now optimized for OCR
    /// # Ok(())
    /// # }
    /// ```
    pub fn scale(&self, image: &DynamicImage) -> Result<DynamicImage, PreprocessingError> {
        let (width, height) = image.dimensions();

        // Estimate current text height (simplified heuristic)
        let estimated_text_height = self.estimate_text_height(image);

        // Calculate scale factor to reach target height
        let scale_factor = self.target_char_height as f32 / estimated_text_height as f32;

        // Apply minimum and maximum scale limits to prevent excessive scaling
        let scale_factor = scale_factor.clamp(0.5, 3.0);

        let new_width = (width as f32 * scale_factor) as u32;
        let new_height = (height as f32 * scale_factor) as u32;

        // Use cubic interpolation (Catmull-Rom) for high-quality scaling
        let scaled = image.resize(
            new_width,
            new_height,
            image::imageops::FilterType::CatmullRom,
        );

        Ok(scaled)
    }

    /// Estimates the text height in an image using a simple heuristic.
    ///
    /// This is a basic implementation that assumes text takes up a significant portion
    /// of the image height. More sophisticated implementations could use edge detection
    /// or connected component analysis.
    ///
    /// # Arguments
    ///
    /// * `image` - The image to analyze.
    ///
    /// # Returns
    ///
    /// Estimated text height in pixels.
    fn estimate_text_height(&self, image: &DynamicImage) -> u32 {
        let (_, height) = image.dimensions();

        // Simple heuristic: assume text is roughly 1/10 to 1/20 of image height
        // This is a placeholder - real implementation would use more sophisticated analysis
        let estimated_height = height / 15;

        // Clamp to reasonable bounds
        estimated_height.clamp(10, 150)
    }
}

impl Default for ImageScaler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = RgbImage::new(width, height);
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_new_scaler() {
        let scaler = ImageScaler::new();
        assert_eq!(scaler.target_char_height(), 28);
    }

    #[test]
    fn test_with_valid_target_height() {
        let scaler = ImageScaler::with_target_height(25).unwrap();
        assert_eq!(scaler.target_char_height(), 25);
    }

    #[test]
    fn test_with_invalid_target_height_too_low() {
        let result = ImageScaler::with_target_height(15);
        assert!(matches!(
            result,
            Err(PreprocessingError::InvalidTargetHeight { height: 15 })
        ));
    }

    #[test]
    fn test_with_invalid_target_height_too_high() {
        let result = ImageScaler::with_target_height(40);
        assert!(matches!(
            result,
            Err(PreprocessingError::InvalidTargetHeight { height: 40 })
        ));
    }

    #[test]
    fn test_scale_basic_functionality() {
        let scaler = ImageScaler::new();
        let img = create_test_image(100, 100);

        let result = scaler.scale(&img);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        let (scaled_width, scaled_height) = scaled.dimensions();

        // Scaled image should have different dimensions (scaled for target height)
        // Exact dimensions depend on the estimation heuristic
        assert!(scaled_width > 0 && scaled_height > 0);
    }

    #[test]
    fn test_estimate_text_height() {
        let scaler = ImageScaler::new();
        let img = create_test_image(200, 300);

        let estimated = scaler.estimate_text_height(&img);
        assert!((10..=150).contains(&estimated));
    }
}
