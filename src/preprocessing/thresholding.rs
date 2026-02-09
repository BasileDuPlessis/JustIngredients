//! # Image Thresholding Module
//!
//! This module provides binary thresholding functionality for OCR preprocessing.
//! It includes Otsu's method for automatic threshold selection.

use image::DynamicImage;
use tracing;

use super::types::{PreprocessingError, ThresholdedImageResult};

/// Applies Otsu's thresholding algorithm to convert an image to binary (black/white).
///
/// This function automatically determines the optimal threshold value using Otsu's method,
/// which maximizes the between-class variance. The result is a binary image where text
/// appears as black pixels on a white background, optimized for OCR processing.
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
/// println!("Optimal threshold: {}", thresholded.threshold);
/// // thresholded.image is now binary and ready for OCR
/// # Ok(())
/// # }
/// ```
pub fn apply_otsu_threshold(
    image: &DynamicImage,
) -> Result<ThresholdedImageResult, PreprocessingError> {
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
        let binary_value = if intensity > optimal_threshold {
            255u8
        } else {
            0u8
        };
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

/// Finds the optimal threshold using Otsu's method by maximizing between-class variance.
///
/// This function implements Otsu's algorithm, which assumes the image contains two classes
/// of pixels (foreground and background) and finds the threshold that maximizes the variance
/// between these classes.
///
/// # Arguments
///
/// * `histogram` - The 256-bin histogram of pixel intensities
/// * `total_pixels` - Total number of pixels in the image
///
/// # Returns
///
/// Returns the optimal threshold value (0-255) or a `PreprocessingError` if calculation fails
fn find_otsu_threshold(
    histogram: &[u32; 256],
    total_pixels: f64,
) -> Result<u8, PreprocessingError> {
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
            (total_weighted_sum - cumulative_weighted_sums[threshold_idx])
                / (cumulative_sums[255] - cumulative_sums[threshold_idx])
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

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
        let result = apply_otsu_threshold(&dynamic_img)
            .expect("apply_otsu_threshold should succeed with valid grayscale image");

        // Check that we got a valid threshold
        assert!(result.threshold > 0 && result.threshold < 255);

        // Check that result image is binary (only 0 or 255 values)
        let binary_img = result.image.to_luma8();
        for pixel in binary_img.pixels() {
            assert!(pixel[0] == 0 || pixel[0] == 255);
        }

        // Check processing time
        assert!(result.processing_time_ms < 1000); // Should be fast
    }

    #[test]
    fn test_apply_otsu_threshold_uniform_image() {
        // Create a uniform gray image
        let mut img = image::GrayImage::new(10, 10);
        for pixel in img.pixels_mut() {
            pixel[0] = 128;
        }

        let dynamic_img = DynamicImage::ImageLuma8(img);
        let _result = apply_otsu_threshold(&dynamic_img)
            .expect("apply_otsu_threshold should succeed with uniform grayscale image");

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
        let threshold = find_otsu_threshold(&histogram, total_pixels)
            .expect("find_otsu_threshold should succeed with valid histogram");

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
        let _threshold = find_otsu_threshold(&histogram, total_pixels)
            .expect("find_otsu_threshold should succeed with single class histogram");

        // Should still return a valid threshold
        // (threshold is u8, so it's always <= 255)
    }
}
