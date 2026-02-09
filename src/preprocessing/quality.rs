//! # Image Quality Assessment Module
//!
//! This module provides image quality assessment functionality for adaptive preprocessing.
//! It evaluates contrast, brightness, and sharpness to determine optimal preprocessing strategies.

use image::DynamicImage;
use tracing;

use super::types::{ImageQuality, ImageQualityResult, PreprocessingError};

/// Assesses the quality of an image to determine optimal preprocessing strategy.
///
/// This function calculates basic quality metrics including contrast, brightness,
/// and sharpness to classify images as High, Medium, or Low quality. High-quality
/// images may skip heavy preprocessing steps for better performance.
///
/// # Arguments
///
/// * `image` - The input image to assess
///
/// # Returns
///
/// Returns a `Result` containing the quality assessment or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::{assess_image_quality, ImageQuality};
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("recipe.jpg")?;
/// let quality = assess_image_quality(&img)?;
/// match quality.quality {
///     ImageQuality::High => println!("High quality - minimal preprocessing"),
///     ImageQuality::Medium => println!("Medium quality - standard preprocessing"),
///     ImageQuality::Low => println!("Low quality - full preprocessing pipeline"),
/// }
/// # Ok(())
/// # }
/// ```
pub fn assess_image_quality(
    image: &DynamicImage,
) -> Result<ImageQualityResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Convert to grayscale for analysis
    let gray = image.to_luma8();

    // Calculate quality metrics
    let contrast_ratio = calculate_contrast_ratio(&gray);
    let brightness = calculate_brightness(&gray);
    let sharpness = calculate_sharpness(&gray);

    // Classify overall quality based on metrics
    let quality = classify_image_quality(contrast_ratio, brightness, sharpness);

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "Quality assessment completed in {:.2}ms: quality={:?}, contrast={:.3}, brightness={:.3}, sharpness={:.3}",
        processing_time.as_millis(),
        quality,
        contrast_ratio,
        brightness,
        sharpness
    );

    Ok(ImageQualityResult {
        quality,
        contrast_ratio,
        brightness,
        sharpness,
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Calculates the contrast ratio of a grayscale image.
///
/// Contrast ratio is computed as the ratio of the 90th percentile to the 10th percentile
/// of pixel intensities, providing a robust measure of image contrast.
/// For uniform images, this will be close to 1.0, indicating low contrast.
///
/// # Arguments
///
/// * `image` - The grayscale image to analyze
///
/// # Returns
///
/// Contrast ratio between 0.0 and 1.0 (higher values indicate better contrast)
fn calculate_contrast_ratio(image: &image::GrayImage) -> f32 {
    let mut pixels: Vec<u8> = image.pixels().map(|p| p[0]).collect();

    if pixels.is_empty() {
        return 0.0;
    }

    // Sort pixels for percentile calculation
    pixels.sort_unstable();

    let len = pixels.len();
    let p10_idx = (len as f32 * 0.1) as usize;
    let p90_idx = (len as f32 * 0.9) as usize;

    let p10 = pixels[p10_idx] as f32 / 255.0;
    let p90 = pixels[p90_idx] as f32 / 255.0;

    // Calculate the range between p90 and p10
    // For uniform images, this will be small (close to 0)
    // For high contrast images, this will be large (close to 1)
    let contrast_range = (p90 - p10).max(0.0);

    // Normalize to 0.0-1.0 range
    contrast_range.min(1.0)
}

/// Calculates the brightness level of a grayscale image.
///
/// Brightness is computed as the mean pixel intensity, normalized to 0.0-1.0 range.
/// Values closer to 0.5 are considered optimal for OCR.
///
/// # Arguments
///
/// * `image` - The grayscale image to analyze
///
/// # Returns
///
/// Brightness level between 0.0 and 1.0
fn calculate_brightness(image: &image::GrayImage) -> f32 {
    let total_pixels = image.width() * image.height();
    if total_pixels == 0 {
        return 0.5; // Default neutral brightness
    }

    let sum: u32 = image.pixels().map(|p| p[0] as u32).sum();
    let mean = sum as f32 / total_pixels as f32;

    mean / 255.0
}

/// Calculates the sharpness score of a grayscale image.
///
/// Sharpness is estimated using the variance of the Laplacian operator,
/// which detects edges and high-frequency content.
///
/// # Arguments
///
/// * `image` - The grayscale image to analyze
///
/// # Returns
///
/// Sharpness score between 0.0 and 1.0 (higher is sharper)
fn calculate_sharpness(image: &image::GrayImage) -> f32 {
    let (width, height) = image.dimensions();

    if width < 3 || height < 3 {
        return 0.5; // Default sharpness for very small images
    }

    let mut laplacian_sum = 0.0;
    let mut pixel_count = 0;

    // Apply Laplacian kernel to estimate sharpness
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            // Laplacian kernel: [[0, 1, 0], [1, -4, 1], [0, 1, 0]]
            let center = image.get_pixel(x, y)[0] as f32;
            let top = image.get_pixel(x, y - 1)[0] as f32;
            let bottom = image.get_pixel(x, y + 1)[0] as f32;
            let left = image.get_pixel(x - 1, y)[0] as f32;
            let right = image.get_pixel(x + 1, y)[0] as f32;

            let laplacian = -4.0 * center + top + bottom + left + right;
            laplacian_sum += laplacian * laplacian;
            pixel_count += 1;
        }
    }

    if pixel_count == 0 {
        return 0.5;
    }

    // Calculate variance of Laplacian (higher values indicate sharper images)
    let variance = laplacian_sum / pixel_count as f32;

    // Normalize to 0.0-1.0 range (rough heuristic based on typical values)
    (variance / 1000.0).min(1.0)
}

/// Classifies image quality based on calculated metrics.
///
/// Uses a weighted combination of contrast, brightness, and sharpness metrics
/// to determine overall image quality for OCR preprocessing decisions.
///
/// # Arguments
///
/// * `contrast_ratio` - Contrast ratio (0.0-1.0)
/// * `brightness` - Brightness level (0.0-1.0)
/// * `sharpness` - Sharpness score (0.0-1.0)
///
/// # Returns
///
/// Overall image quality classification
fn classify_image_quality(contrast_ratio: f32, brightness: f32, sharpness: f32) -> ImageQuality {
    // Calculate quality score (weighted combination of metrics)
    let contrast_score = contrast_ratio; // Higher contrast is better
    let brightness_score = 1.0 - (brightness - 0.5).abs() * 2.0; // Closer to 0.5 is better
    let sharpness_score = sharpness; // Higher sharpness is better

    // Weighted average (contrast and sharpness are more important than brightness)
    let quality_score = (contrast_score * 0.4) + (brightness_score * 0.2) + (sharpness_score * 0.4);

    // Classify based on score thresholds
    if quality_score >= 0.7 {
        ImageQuality::High
    } else if quality_score >= 0.4 {
        ImageQuality::Medium
    } else {
        ImageQuality::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn create_uniform_image(width: u32, height: u32, intensity: u8) -> DynamicImage {
        let mut img = image::GrayImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(x, y, image::Luma([intensity]));
            }
        }
        DynamicImage::ImageLuma8(img)
    }

    fn create_gradient_image(width: u32, height: u32) -> DynamicImage {
        let mut img = image::GrayImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let intensity = ((x as f32 / width as f32) * 255.0) as u8;
                img.put_pixel(x, y, image::Luma([intensity]));
            }
        }
        DynamicImage::ImageLuma8(img)
    }

    #[test]
    fn test_assess_image_quality_uniform_dark() {
        let img = create_uniform_image(100, 100, 0); // All black
        let result = assess_image_quality(&img)
            .expect("assess_image_quality should succeed with valid image");

        assert_eq!(result.quality, ImageQuality::Low);
        assert_eq!(result.contrast_ratio, 0.0);
        assert_eq!(result.brightness, 0.0);
        assert!(result.sharpness >= 0.0 && result.sharpness <= 1.0);
        assert!(result.processing_time_ms < 100); // Should be fast
    }

    #[test]
    fn test_assess_image_quality_uniform_bright() {
        let img = create_uniform_image(100, 100, 255); // All white
        let result = assess_image_quality(&img)
            .expect("assess_image_quality should succeed with valid image");

        assert_eq!(result.quality, ImageQuality::Low);
        assert_eq!(result.contrast_ratio, 0.0);
        assert_eq!(result.brightness, 1.0);
        assert!(result.sharpness >= 0.0 && result.sharpness <= 1.0);
        assert!(result.processing_time_ms < 100); // Should be fast
    }

    #[test]
    fn test_assess_image_quality_uniform_medium() {
        let img = create_uniform_image(100, 100, 128); // Medium gray
        let result = assess_image_quality(&img)
            .expect("assess_image_quality should succeed with valid image");

        assert_eq!(result.quality, ImageQuality::Low);
        assert_eq!(result.contrast_ratio, 0.0);
        // 128/255 ≈ 0.50196, not exactly 0.5
        assert!((result.brightness - 0.50196).abs() < 0.0001);
        assert!(result.sharpness >= 0.0 && result.sharpness <= 1.0);
        assert!(result.processing_time_ms < 100); // Should be fast
    }

    #[test]
    fn test_assess_image_quality_gradient() {
        let img = create_gradient_image(100, 100);
        let result = assess_image_quality(&img)
            .expect("assess_image_quality should succeed with valid image");

        // Gradient should have good contrast
        assert!(result.contrast_ratio > 0.5);
        assert!(result.brightness > 0.4 && result.brightness < 0.6);
        assert!(result.sharpness >= 0.0 && result.sharpness <= 1.0);
        assert!(result.processing_time_ms < 100); // Should be fast
    }

    #[test]
    fn test_calculate_contrast_ratio_uniform() {
        let img = create_uniform_image(50, 50, 128).to_luma8();
        let contrast = calculate_contrast_ratio(&img);

        assert_eq!(contrast, 0.0);
    }

    #[test]
    fn test_calculate_contrast_ratio_gradient() {
        let img = create_gradient_image(50, 50).to_luma8();
        let contrast = calculate_contrast_ratio(&img);

        // Gradient should have high contrast (approximately 0.8)
        assert!(contrast > 0.7);
    }

    #[test]
    fn test_calculate_brightness_uniform() {
        let img_dark = create_uniform_image(50, 50, 0).to_luma8();
        let img_bright = create_uniform_image(50, 50, 255).to_luma8();
        let img_medium = create_uniform_image(50, 50, 128).to_luma8();

        assert_eq!(calculate_brightness(&img_dark), 0.0);
        assert_eq!(calculate_brightness(&img_bright), 1.0);
        // 128/255 ≈ 0.50196, not exactly 0.5
        assert!((calculate_brightness(&img_medium) - 0.50196).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_brightness_gradient() {
        let img = create_gradient_image(50, 50).to_luma8();
        let brightness = calculate_brightness(&img);

        // Gradient should have medium brightness
        assert!(brightness > 0.4 && brightness < 0.6);
    }

    #[test]
    fn test_calculate_sharpness_uniform() {
        let img = create_uniform_image(50, 50, 128).to_luma8();
        let sharpness = calculate_sharpness(&img);

        // Uniform image should have low sharpness
        assert!(sharpness < 0.1);
    }

    #[test]
    fn test_calculate_sharpness_small_image() {
        let img = create_uniform_image(2, 2, 128).to_luma8();
        let sharpness = calculate_sharpness(&img);

        // Small images return default sharpness
        assert_eq!(sharpness, 0.5);
    }

    #[test]
    fn test_classify_image_quality_high() {
        // High contrast, optimal brightness, high sharpness
        let quality = classify_image_quality(0.9, 0.5, 0.9);
        assert_eq!(quality, ImageQuality::High);
    }

    #[test]
    fn test_classify_image_quality_medium() {
        // Medium metrics
        let quality = classify_image_quality(0.6, 0.5, 0.6);
        assert_eq!(quality, ImageQuality::Medium);
    }

    #[test]
    fn test_classify_image_quality_low() {
        // Low contrast, poor brightness, low sharpness
        let quality = classify_image_quality(0.1, 0.1, 0.1);
        assert_eq!(quality, ImageQuality::Low);
    }

    #[test]
    fn test_classify_image_quality_edge_cases() {
        // Test boundary conditions
        let high_boundary = classify_image_quality(0.7, 0.5, 0.7);
        assert_eq!(high_boundary, ImageQuality::High);

        let medium_boundary = classify_image_quality(0.3, 0.5, 0.3);
        assert_eq!(medium_boundary, ImageQuality::Medium);

        let low_boundary = classify_image_quality(0.2, 0.5, 0.2);
        assert_eq!(low_boundary, ImageQuality::Low);
    }

    #[test]
    fn test_assess_image_quality_performance() {
        let img = create_gradient_image(200, 200);
        let result = assess_image_quality(&img)
            .expect("assess_image_quality should succeed with valid image");

        // Should complete in reasonable time (< 100ms for 200x200 image)
        assert!(result.processing_time_ms < 100);
    }
}
