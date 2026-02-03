//! # Image Preprocessing Module
//!
//! This module provides image preprocessing functionality for OCR accuracy improvement.
//! It includes scaling, filtering, and other operations to optimize images before
//! text recognition with Tesseract.

use image::{DynamicImage, GenericImageView};
use tracing;

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

    /// Estimates text height using advanced image analysis techniques.
    ///
    /// This method uses multiple heuristics to provide a more accurate text height estimation:
    /// - Analyzes image histogram for text-like features
    /// - Considers image dimensions and aspect ratio
    /// - Applies recipe-specific optimizations
    ///
    /// # Arguments
    ///
    /// * `image` - The image to analyze
    ///
    /// # Returns
    ///
    /// Estimated text height in pixels (10-150 range)
    pub fn estimate_text_height_advanced(&self, image: &DynamicImage) -> u32 {
        let (width, height) = image.dimensions();

        // Convert to grayscale for analysis
        let gray = image.to_luma8();

        // Calculate histogram-based metrics
        let mut histogram = [0u32; 256];
        for pixel in gray.pixels() {
            histogram[pixel[0] as usize] += 1;
        }

        // Calculate image statistics
        let total_pixels = (width * height) as f32;
        let dark_pixels = histogram[0..128].iter().sum::<u32>() as f32;
        let dark_ratio = dark_pixels / total_pixels;

        // Estimate text density (recipes often have 20-40% text coverage)
        let text_density = dark_ratio.clamp(0.1, 0.6);

        // Base estimation from image dimensions
        let aspect_ratio = width as f32 / height as f32;
        let mut estimated_height = if aspect_ratio > 1.5 {
            // Wide image (landscape) - likely full recipe layout
            (height as f32 * 0.12) as u32
        } else if aspect_ratio < 0.8 {
            // Tall image (portrait) - likely ingredient list
            (height as f32 * 0.08) as u32
        } else {
            // Square-ish image - balanced approach
            (height as f32 * 0.10) as u32
        };

        // Adjust based on text density
        if text_density > 0.4 {
            // High text density - smaller text
            estimated_height = (estimated_height as f32 * 0.8) as u32;
        } else if text_density < 0.2 {
            // Low text density - larger text or sparse layout
            estimated_height = (estimated_height as f32 * 1.2) as u32;
        }

        // Recipe-specific optimizations
        if width > 1000 && height > 1000 {
            // High-resolution image - text might be smaller relative to image
            estimated_height = (estimated_height as f32 * 0.9) as u32;
        }

        // Clamp to reasonable bounds for OCR optimization
        estimated_height.clamp(10, 150)
    }

    /// Scales an image using OCR-optimized logic with intelligent decision making.
    ///
    /// This method provides more sophisticated scaling than the basic `scale()` method:
    /// - Uses advanced text height estimation
    /// - Applies recipe-specific scaling rules
    /// - Includes performance logging
    /// - Prevents excessive scaling with adaptive limits
    ///
    /// # Arguments
    ///
    /// * `image` - The input image to scale
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the scaled image and scaling metadata, or a `PreprocessingError`
    pub fn scale_for_ocr(&self, image: &DynamicImage) -> Result<ScaledImageResult, PreprocessingError> {
        let start_time = std::time::Instant::now();
        let (original_width, original_height) = image.dimensions();

        // Use advanced text height estimation
        let estimated_text_height = self.estimate_text_height_advanced(image);

        // Calculate optimal scale factor
        let scale_factor = self.calculate_optimal_scale_factor(estimated_text_height, original_width, original_height);

        // Apply adaptive scaling limits based on image characteristics
        let scale_factor = self.apply_adaptive_scaling_limits(scale_factor, original_width, original_height);

        // Calculate new dimensions
        let new_width = (original_width as f32 * scale_factor) as u32;
        let new_height = (original_height as f32 * scale_factor) as u32;

        // Apply scaling with high-quality interpolation
        let scaled_image = image.resize(
            new_width,
            new_height,
            image::imageops::FilterType::CatmullRom,
        );

        let processing_time = start_time.elapsed();

        // Log performance metrics
        tracing::debug!(
            target: "ocr_preprocessing",
            "Image scaled: {}x{} -> {}x{} (factor: {:.2}, text_height: {}, time: {:.2}ms)",
            original_width,
            original_height,
            new_width,
            new_height,
            scale_factor,
            estimated_text_height,
            processing_time.as_millis()
        );

        Ok(ScaledImageResult {
            image: scaled_image,
            original_dimensions: (original_width, original_height),
            new_dimensions: (new_width, new_height),
            scale_factor,
            estimated_text_height,
            processing_time_ms: processing_time.as_millis() as u32,
        })
    }

    /// Calculates the optimal scale factor based on estimated text height and image characteristics.
    ///
    /// # Arguments
    ///
    /// * `estimated_text_height` - Estimated height of text in pixels
    /// * `width` - Original image width
    /// * `height` - Original image height
    ///
    /// # Returns
    ///
    /// Optimal scale factor for OCR processing
    fn calculate_optimal_scale_factor(&self, estimated_text_height: u32, width: u32, height: u32) -> f32 {
        let target_ratio = self.target_char_height as f32 / estimated_text_height as f32;

        // Recipe-specific scaling adjustments
        let aspect_ratio = width as f32 / height as f32;
        let size_category = width * height;

        let mut adjusted_ratio = target_ratio;

        // Adjust for very small text (likely needs more upscaling)
        if estimated_text_height < 15 {
            adjusted_ratio *= 1.2;
        }
        // Adjust for very large text (likely needs less scaling)
        else if estimated_text_height > 80 {
            adjusted_ratio *= 0.9;
        }

        // Adjust based on image size
        if size_category < 100_000 {
            // Small images - be more aggressive with upscaling
            adjusted_ratio *= 1.1;
        } else if size_category > 2_000_000 {
            // Large images - be more conservative
            adjusted_ratio *= 0.95;
        }

        // Adjust based on aspect ratio (recipes often have specific layouts)
        if aspect_ratio > 2.0 {
            // Very wide images (likely full recipe pages)
            adjusted_ratio *= 0.95;
        } else if aspect_ratio < 0.5 {
            // Very tall images (likely ingredient lists)
            adjusted_ratio *= 1.05;
        }

        adjusted_ratio
    }

    /// Applies adaptive scaling limits to prevent excessive scaling.
    ///
    /// # Arguments
    ///
    /// * `scale_factor` - The calculated scale factor
    /// * `width` - Original image width
    /// * `height` - Original image height
    ///
    /// # Returns
    ///
    /// Scale factor clamped to safe limits
    fn apply_adaptive_scaling_limits(&self, scale_factor: f32, width: u32, height: u32) -> f32 {
        let size_category = width * height;

        // Adaptive limits based on image size
        let (min_scale, max_scale) = if size_category < 100_000 {
            // Small images - allow more upscaling
            (0.8, 4.0)
        } else if size_category > 2_000_000 {
            // Large images - be more conservative
            (0.3, 2.0)
        } else {
            // Medium images - balanced approach
            (0.5, 3.0)
        };

        scale_factor.clamp(min_scale, max_scale)
    }
}

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

/// Applies Otsu's thresholding algorithm to convert an image to binary (black/white).
///
/// Otsu's method automatically determines the optimal threshold by maximizing the
/// variance between two classes of pixels (foreground and background). This is
/// particularly effective for images with varying lighting conditions.
///
/// # Arguments
///
/// * `image` - The input image to threshold
///
/// # Returns
///
/// Returns a `Result` containing the thresholded image and metadata, or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::apply_otsu_threshold;
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("recipe.jpg")?;
/// let thresholded = apply_otsu_threshold(&img)?;
/// // thresholded.image is now a binary image optimized for OCR
/// # Ok(())
/// # }
/// ```
pub fn apply_otsu_threshold(image: &DynamicImage) -> Result<ThresholdedImageResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Convert to grayscale for thresholding
    let gray = image.to_luma8();

    // Calculate histogram
    let mut histogram = [0u32; 256];
    let total_pixels = (gray.width() * gray.height()) as f64;

    for pixel in gray.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    // Find optimal threshold using Otsu's method
    let optimal_threshold = find_otsu_threshold(&histogram, total_pixels)?;

    // Apply binary thresholding
    let mut binary_img = image::GrayImage::new(gray.width(), gray.height());

    for (x, y, pixel) in gray.enumerate_pixels() {
        let intensity = pixel[0];
        let binary_value = if intensity > optimal_threshold { 255u8 } else { 0u8 };
        binary_img.put_pixel(x, y, image::Luma([binary_value]));
    }

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "Otsu thresholding completed in {:.2}ms: threshold={}, dimensions={}x{}",
        processing_time.as_millis(),
        optimal_threshold,
        gray.width(),
        gray.height()
    );

    Ok(ThresholdedImageResult {
        image: DynamicImage::ImageLuma8(binary_img),
        threshold: optimal_threshold,
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Finds the optimal threshold using Otsu's method.
///
/// Otsu's method maximizes the variance between two classes of pixels.
/// The algorithm calculates the between-class variance for each possible
/// threshold and returns the one that maximizes it.
///
/// # Arguments
///
/// * `histogram` - Array of 256 histogram bins
/// * `total_pixels` - Total number of pixels in the image
///
/// # Returns
///
/// Returns the optimal threshold value (0-255)
fn find_otsu_threshold(histogram: &[u32; 256], total_pixels: f64) -> Result<u8, PreprocessingError> {
    // Calculate cumulative sums for efficiency
    let mut cumulative_sum = 0f64;
    let mut cumulative_weighted_sum = 0f64;

    // Pre-calculate cumulative statistics
    let mut cumulative_sums = [0f64; 256];
    let mut cumulative_weighted_sums = [0f64; 256];

    for i in 0..256 {
        let pixel_count = histogram[i] as f64;
        cumulative_sum += pixel_count;
        cumulative_weighted_sum += (i as f64) * pixel_count;

        cumulative_sums[i] = cumulative_sum;
        cumulative_weighted_sums[i] = cumulative_weighted_sum;
    }

    // Find optimal threshold by maximizing between-class variance
    let mut max_variance = 0f64;
    let mut optimal_threshold = 128u8; // Default fallback

    let total_weighted_sum = cumulative_weighted_sums[255];

    for threshold in 1..255 {
        let threshold_idx = threshold as usize;

        // Weight of background class (pixels <= threshold)
        let w0 = cumulative_sums[threshold_idx] / total_pixels;

        // Weight of foreground class (pixels > threshold)
        let w1 = 1.0 - w0;

        // Avoid division by zero
        if w0 == 0.0 || w1 == 0.0 {
            continue;
        }

        // Mean of background class
        let mu0 = if cumulative_sums[threshold_idx] > 0.0 {
            cumulative_weighted_sums[threshold_idx] / cumulative_sums[threshold_idx]
        } else {
            0.0
        };

        // Mean of foreground class
        let mu1 = if cumulative_sums[255] - cumulative_sums[threshold_idx] > 0.0 {
            (total_weighted_sum - cumulative_weighted_sums[threshold_idx]) /
            (cumulative_sums[255] - cumulative_sums[threshold_idx])
        } else {
            0.0
        };

        // Between-class variance
        let variance = w0 * w1 * (mu0 - mu1).powi(2);

        if variance > max_variance {
            max_variance = variance;
            optimal_threshold = threshold as u8;
        }
    }

    Ok(optimal_threshold)
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
    fn test_apply_otsu_threshold_simple_image() {
        // Create a simple test image with two distinct regions
        let mut img = image::GrayImage::new(10, 10);

        // Fill first half with dark pixels (0-50)
        for y in 0..10 {
            for x in 0..5 {
                img.put_pixel(x, y, image::Luma([25]));
            }
        }

        // Fill second half with light pixels (200-255)
        for y in 0..10 {
            for x in 5..10 {
                img.put_pixel(x, y, image::Luma([225]));
            }
        }

        let dynamic_img = DynamicImage::ImageLuma8(img);
        let result = apply_otsu_threshold(&dynamic_img).unwrap();

        // The threshold should be between the two intensity values
        assert!((25..=225).contains(&result.threshold));
        // (processing_time_ms is u32, so it's always >= 0)

        // Check that the result is a binary image
        if let DynamicImage::ImageLuma8(binary_img) = &result.image {
            for pixel in binary_img.pixels() {
                assert!(pixel[0] == 0 || pixel[0] == 255);
            }
        } else {
            panic!("Expected binary image");
        }
    }

    #[test]
    fn test_apply_otsu_threshold_uniform_image() {
        // Create a uniform gray image
        let mut img = image::GrayImage::new(10, 10);
        for pixel in img.pixels_mut() {
            pixel[0] = 128;
        }

        let dynamic_img = DynamicImage::ImageLuma8(img);
        let _result = apply_otsu_threshold(&dynamic_img).unwrap();

        // For uniform images, Otsu should still produce a valid threshold
        // (threshold is u8, so it's always <= 255)
    }

    #[test]
    fn test_find_otsu_threshold_basic() {
        // Create a simple histogram with two peaks
        let mut histogram = [0u32; 256];

        // Add pixels to create two distinct classes
        histogram[25] = 5000; // Dark class at intensity 25
        histogram[225] = 5000; // Light class at intensity 225

        let total_pixels = 10000.0;
        let threshold = find_otsu_threshold(&histogram, total_pixels).unwrap();

        // Threshold should be somewhere between the two classes
        assert!((25..=225).contains(&threshold));
    }

    #[test]
    fn test_find_otsu_threshold_single_class() {
        // Create a histogram with only one class
        let mut histogram = [0u32; 256];
        for histogram_val in histogram.iter_mut().take(150).skip(100) {
            *histogram_val = 100;
        }

        let total_pixels = 5000.0;
        let _threshold = find_otsu_threshold(&histogram, total_pixels).unwrap();

        // Should still return a valid threshold
        // (threshold is u8, so it's always <= 255)
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

    #[test]
    fn test_estimate_text_height_advanced() {
        let scaler = ImageScaler::new();

        // Test with different image sizes
        let small_img = create_test_image(100, 100);
        let medium_img = create_test_image(500, 500);
        let large_img = create_test_image(1000, 1000);

        let small_estimate = scaler.estimate_text_height_advanced(&small_img);
        let medium_estimate = scaler.estimate_text_height_advanced(&medium_img);
        let large_estimate = scaler.estimate_text_height_advanced(&large_img);

        // All estimates should be in valid range
        assert!((10..=150).contains(&small_estimate));
        assert!((10..=150).contains(&medium_estimate));
        assert!((10..=150).contains(&large_estimate));

        // Larger images should generally have relatively smaller text estimates
        // (though this is a heuristic, so we just check they're reasonable)
        assert!(small_estimate > 5);
        assert!(medium_estimate > 5);
        assert!(large_estimate > 5);
    }

    #[test]
    fn test_scale_for_ocr_basic() {
        let scaler = ImageScaler::new();
        let img = create_test_image(200, 300);

        let result = scaler.scale_for_ocr(&img);
        assert!(result.is_ok());

        let scaled_result = result.unwrap();

        // Check that dimensions changed appropriately
        assert!(scaled_result.new_dimensions.0 > 0);
        assert!(scaled_result.new_dimensions.1 > 0);

        // Check metadata
        assert!(scaled_result.scale_factor > 0.0);
        assert!((10..=150).contains(&scaled_result.estimated_text_height));
        // processing_time_ms is u32, so it's always >= 0
    }

    #[test]
    fn test_calculate_optimal_scale_factor() {
        let scaler = ImageScaler::new();

        // Test various scenarios
        let factor1 = scaler.calculate_optimal_scale_factor(20, 400, 600); // Normal case
        let factor2 = scaler.calculate_optimal_scale_factor(10, 200, 200); // Small text
        let factor3 = scaler.calculate_optimal_scale_factor(100, 800, 600); // Large text

        // All factors should be reasonable
        assert!(factor1 > 0.1 && factor1 < 5.0);
        assert!(factor2 > 0.1 && factor2 < 5.0);
        assert!(factor3 > 0.1 && factor3 < 5.0);

        // Small text should generally need more scaling
        assert!(factor2 > factor1);
    }

    #[test]
    fn test_apply_adaptive_scaling_limits() {
        let scaler = ImageScaler::new();

        // Test small image
        let limit1 = scaler.apply_adaptive_scaling_limits(5.0, 100, 100);
        assert!(limit1 <= 4.0); // Should be clamped

        // Test medium image
        let limit2 = scaler.apply_adaptive_scaling_limits(4.0, 500, 500);
        assert!(limit2 <= 3.0); // Should be clamped

        // Test large image
        let limit3 = scaler.apply_adaptive_scaling_limits(3.0, 1500, 1500);
        assert!(limit3 <= 2.0); // Should be clamped

        // Test minimum limits
        let limit4 = scaler.apply_adaptive_scaling_limits(0.1, 500, 500);
        assert!(limit4 >= 0.5); // Should be clamped up
    }

    #[test]
    fn test_scaled_image_result_structure() {
        let scaler = ImageScaler::new();
        let img = create_test_image(100, 100);

        let result = scaler.scale_for_ocr(&img).unwrap();

        // Check that all fields are populated
        assert_eq!(result.original_dimensions, (100, 100));
        assert!(result.new_dimensions.0 > 0 && result.new_dimensions.1 > 0);
        assert!(result.scale_factor > 0.0);
        assert!((10..=150).contains(&result.estimated_text_height));
        // processing_time_ms is u32, so it's always >= 0
    }
}
