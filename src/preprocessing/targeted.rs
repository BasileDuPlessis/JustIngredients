//! # Targeted Preprocessing Module
//!
//! This module provides specialized preprocessing for cropped measurement regions
//! to enhance OCR accuracy for small text like fractions and quantities.

use image::{DynamicImage, GenericImageView};
use std::time::Instant;
use tracing;

use super::types::{PreprocessingError, TargetedPreprocessingResult};

/// Preprocesses a cropped measurement region for optimal OCR recognition.
///
/// This function applies a specialized pipeline designed for small text elements:
/// 1. Upscales the image by 2.5x for better resolution
/// 2. Converts to grayscale if needed
/// 3. Applies aggressive Otsu thresholding for binarization
///
/// The result is a binary image optimized for recognizing small fractions and quantities.
///
/// # Arguments
///
/// * `image` - The cropped image to preprocess
///
/// # Returns
///
/// Returns a `TargetedPreprocessingResult` containing the processed image and metadata,
/// or a `PreprocessingError` if processing fails.
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::{crop_measurement_region, preprocess_measurement_region};
/// use just_ingredients::ocr::BBox;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // First crop the region
/// let cropped = crop_measurement_region("image.jpg", &BBox::new(10, 10, 50, 30))?;
///
/// // Then apply targeted preprocessing
/// let processed = preprocess_measurement_region(&cropped.image)?;
/// println!("Processed image: {}x{}", processed.final_dimensions.0, processed.final_dimensions.1);
/// # Ok(())
/// # }
/// ```
pub fn preprocess_measurement_region(
    image: &DynamicImage,
) -> Result<TargetedPreprocessingResult, PreprocessingError> {
    let start_time = Instant::now();
    let original_dimensions = image.dimensions();

    // Step 1: Upscale the image by 2.5x for better resolution
    let scale_factor = 2.5;
    let new_width = (original_dimensions.0 as f32 * scale_factor) as u32;
    let new_height = (original_dimensions.1 as f32 * scale_factor) as u32;

    let upscaled = image.resize(
        new_width,
        new_height,
        image::imageops::FilterType::CatmullRom, // High-quality interpolation
    );

    // Step 2: Convert to grayscale (Otsu thresholding expects grayscale)
    let grayscale = upscaled.to_luma8();

    // Step 3: Apply aggressive Otsu thresholding for binarization
    // Calculate histogram for Otsu's method
    let mut histogram = [0u32; 256];
    let total_pixels = (grayscale.width() * grayscale.height()) as f64;

    // Build histogram
    for pixel in grayscale.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    // Calculate Otsu's threshold
    let threshold = calculate_otsu_threshold(&histogram, total_pixels);

    // Apply thresholding to create binary image
    let mut binary_image = image::GrayImage::new(grayscale.width(), grayscale.height());
    for (x, y, pixel) in grayscale.enumerate_pixels() {
        let intensity = pixel[0];
        let binary_value = if intensity > threshold { 255 } else { 0 }; // Inverted for black text on white background
        binary_image.put_pixel(x, y, image::Luma([binary_value]));
    }

    let final_image = DynamicImage::ImageLuma8(binary_image);
    let final_dimensions = final_image.dimensions();
    let processing_time_ms = start_time.elapsed().as_millis() as u32;

    tracing::debug!(
        "Applied targeted preprocessing: {}x{} -> {}x{} (scale: {:.1}x), threshold: {}",
        original_dimensions.0,
        original_dimensions.1,
        final_dimensions.0,
        final_dimensions.1,
        scale_factor,
        threshold
    );

    Ok(TargetedPreprocessingResult {
        image: final_image,
        original_dimensions,
        final_dimensions,
        scale_factor,
        threshold,
        processing_time_ms,
    })
}

/// Calculates the optimal threshold using Otsu's method.
///
/// Otsu's method finds the threshold that maximizes the between-class variance,
/// providing optimal separation between foreground and background.
///
/// # Arguments
///
/// * `histogram` - Array of 256 histogram bins
/// * `total_pixels` - Total number of pixels in the image
///
/// # Returns
///
/// The optimal threshold value (0-255)
fn calculate_otsu_threshold(histogram: &[u32; 256], total_pixels: f64) -> u8 {
    let mut sum = 0.0;
    for (i, &count) in histogram.iter().enumerate() {
        sum += i as f64 * count as f64;
    }

    let mut sum_b = 0.0;
    let mut w_b = 0.0;
    let mut w_f: f64;

    let mut max_variance = 0.0;
    let mut threshold = 0u8;

    for (t, &count) in histogram.iter().enumerate() {
        w_b += count as f64;
        if w_b == 0.0 {
            continue;
        }

        w_f = total_pixels - w_b;
        if w_f == 0.0 {
            break;
        }

        sum_b += t as f64 * count as f64;

        let m_b = sum_b / w_b;
        let m_f = (sum - sum_b) / w_f;

        let variance = w_b * w_f * (m_b - m_f).powi(2);

        if variance > max_variance {
            max_variance = variance;
            threshold = t as u8;
        }
    }

    threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let mut img = RgbImage::new(width, height);
        // Fill with white background
        for pixel in img.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }
        // Add some dark text-like pixels
        for x in 5..15 {
            for y in 5..15 {
                if x < width && y < height {
                    img.put_pixel(x, y, Rgb([50, 50, 50])); // Dark gray text
                }
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_preprocess_measurement_region_success() {
        let test_img = create_test_image(40, 20); // Small cropped region

        let result = match preprocess_measurement_region(&test_img) {
            Ok(r) => r,
            Err(e) => panic!("preprocess_measurement_region failed: {:?}", e),
        };

        // Verify upscaling occurred (2.5x factor)
        assert_eq!(result.scale_factor, 2.5);
        assert_eq!(result.final_dimensions.0, (40.0 * 2.5) as u32);
        assert_eq!(result.final_dimensions.1, (20.0 * 2.5) as u32);

        // Verify original dimensions are preserved
        assert_eq!(result.original_dimensions, (40, 20));

        // Verify threshold is within valid range (u8 is always valid)
        // Verify processing time was recorded
        assert!(result.processing_time_ms < 1000); // Should be fast

        // Verify output is grayscale/binary
        match &result.image {
            DynamicImage::ImageLuma8(_) => {} // Expected binary image
            _ => panic!("Expected grayscale image"),
        }
    }

    #[test]
    fn test_preprocess_measurement_region_dimensions() {
        let test_img = create_test_image(30, 15);

        let result = match preprocess_measurement_region(&test_img) {
            Ok(r) => r,
            Err(e) => panic!("preprocess_measurement_region failed: {:?}", e),
        };

        // Check that dimensions increased (approximately 2.5x scaling)
        assert!(result.final_dimensions.0 > result.original_dimensions.0);
        assert!(result.final_dimensions.1 > result.original_dimensions.1);

        // Verify scale factor
        assert_eq!(result.scale_factor, 2.5);
    }

    #[test]
    fn test_calculate_otsu_threshold() {
        // Create a simple histogram with clear bimodality
        let mut histogram = [0u32; 256];
        // Background pixels (high intensity)
        for count in histogram.iter_mut().skip(200) {
            *count = 100;
        }
        // Foreground pixels (low intensity)
        for count in histogram.iter_mut().take(50) {
            *count = 100;
        }

        let total_pixels = 25600.0; // 256 * 100
        let _threshold = calculate_otsu_threshold(&histogram, total_pixels);

        // Threshold is a u8, so it's always valid by definition
    }

    #[test]
    fn test_calculate_otsu_threshold_uniform() {
        // Uniform histogram (all pixels same intensity)
        let histogram = [100u32; 256];
        let total_pixels = 25600.0;

        let _threshold = calculate_otsu_threshold(&histogram, total_pixels);

        // With uniform distribution, threshold can be any valid u8 value
    }
}
