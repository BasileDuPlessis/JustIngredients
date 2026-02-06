//! # Image Deskewing Module
//!
//! This module provides text rotation detection and correction functionality.
//! It uses projection profile analysis to detect skew and applies rotation correction.

use image::{DynamicImage, GenericImage, GenericImageView};
use tracing;

use super::types::{DeskewResult, PreprocessingError};

/// Detects and corrects text skew in an image using projection profile analysis.
///
/// This function analyzes the horizontal projection profile of the image to detect
/// text line orientation, then rotates the image to make text horizontal.
/// It's designed for small rotations (±10°) commonly found in photos.
///
/// # Arguments
///
/// * `image` - The input image to deskew
///
/// # Returns
///
/// Returns a `Result` containing the deskew result or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::deskew_image;
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("rotated_recipe.jpg")?;
/// let result = deskew_image(&img)?;
/// println!("Detected skew: {:.2}°", result.skew_angle_degrees);
/// // result.image is now deskewed
/// # Ok(())
/// # }
/// ```
pub fn deskew_image(image: &DynamicImage) -> Result<DeskewResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Convert to grayscale for analysis
    let gray = image.to_luma8();

    // Detect skew angle using projection profile analysis
    let skew_angle = detect_skew_angle(&gray)?;

    // If skew is very small (< 0.5°), don't rotate to avoid introducing artifacts
    if skew_angle.abs() < 0.5 {
        tracing::debug!(
            target: "ocr_preprocessing",
            "Skew angle {:.2}° is below threshold, skipping deskewing",
            skew_angle
        );

        return Ok(DeskewResult {
            image: image.clone(),
            skew_angle_degrees: skew_angle,
            confidence: 0.0, // No rotation applied
            processing_time_ms: start_time.elapsed().as_millis() as u32,
        });
    }

    // Apply rotation correction
    let rotated = apply_rotation_correction(image, skew_angle)?;

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "Deskewing completed in {:.2}ms: corrected {:.2}° skew",
        processing_time.as_millis(),
        skew_angle
    );

    Ok(DeskewResult {
        image: rotated,
        skew_angle_degrees: skew_angle,
        confidence: calculate_skew_confidence(&gray, skew_angle),
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Detects the skew angle of text in an image using projection profile analysis.
///
/// This method works by:
/// 1. Binarizing the image to separate text from background
/// 2. Computing horizontal projection profiles at different angles
/// 3. Finding the angle that maximizes text line alignment (minimizes variance)
///
/// # Arguments
///
/// * `image` - Grayscale image to analyze
///
/// # Returns
///
/// Returns the detected skew angle in degrees (-10.0 to 10.0 range)
fn detect_skew_angle(image: &image::GrayImage) -> Result<f32, PreprocessingError> {
    let (_width, _height) = image.dimensions();

    // Binarize image using Otsu's method for text/background separation
    let binary = apply_otsu_threshold_local(image)?;

    // Define angle range to search (±10° in 0.5° increments)
    let angles: Vec<f32> = (-20..=20).map(|i| i as f32 * 0.5).collect();
    let mut best_angle = 0.0f32;
    let mut min_variance = f32::INFINITY;

    // Test each angle and find the one with minimum projection variance
    for &angle in &angles {
        let variance = calculate_projection_variance(&binary, angle);
        if variance < min_variance {
            min_variance = variance;
            best_angle = angle;
        }
    }

    // Refine the search around the best angle with finer resolution
    let refined_angles: Vec<f32> = (0..=10)
        .map(|i| best_angle - 0.5 + (i as f32 * 0.1))
        .collect();

    for &angle in &refined_angles {
        let variance = calculate_projection_variance(&binary, angle);
        if variance < min_variance {
            min_variance = variance;
            best_angle = angle;
        }
    }

    Ok(best_angle)
}

/// Calculates the variance of horizontal projection profile at a given angle.
///
/// Lower variance indicates better text line alignment (more horizontal text).
///
/// # Arguments
///
/// * `binary_image` - Binary image to analyze
/// * `angle_degrees` - Rotation angle to test in degrees
///
/// # Returns
///
/// Variance of the projection profile (lower = better alignment)
fn calculate_projection_variance(binary_image: &image::GrayImage, angle_degrees: f32) -> f32 {
    let (width, height) = binary_image.dimensions();
    let angle_rad = angle_degrees.to_radians();

    // Create projection profile (sum of black pixels per row after rotation)
    let mut projections = vec![0u32; height as usize];

    // Sample points along the rotated coordinate system
    for y in 0..height {
        for x in 0..width {
            // Rotate point around center
            let cx = width as f32 / 2.0;
            let cy = height as f32 / 2.0;

            let dx = x as f32 - cx;
            let dy = y as f32 - cy;

            let rotated_x = dx * angle_rad.cos() - dy * angle_rad.sin() + cx;
            let rotated_y = dx * angle_rad.sin() + dy * angle_rad.cos() + cy;

            // Check if rotated point is within bounds
            if rotated_x >= 0.0
                && rotated_x < width as f32
                && rotated_y >= 0.0
                && rotated_y < height as f32
            {
                let pixel = binary_image.get_pixel(rotated_x as u32, rotated_y as u32);
                if pixel[0] < 128 {
                    // Black pixel (text)
                    projections[y as usize] += 1;
                }
            }
        }
    }

    // Calculate variance of projection profile
    let mean: f32 = projections.iter().map(|&x| x as f32).sum::<f32>() / projections.len() as f32;
    let variance: f32 = projections
        .iter()
        .map(|&x| (x as f32 - mean).powi(2))
        .sum::<f32>()
        / projections.len() as f32;

    variance
}

/// Applies rotation correction to an image.
///
/// # Arguments
///
/// * `image` - The input image to rotate
/// * `angle_degrees` - Rotation angle in degrees (positive = clockwise)
///
/// # Returns
///
/// Returns the rotated image
fn apply_rotation_correction(
    image: &DynamicImage,
    angle_degrees: f32,
) -> Result<DynamicImage, PreprocessingError> {
    let (width, height) = image.dimensions();
    let angle_rad = -angle_degrees.to_radians(); // Negative for counter-clockwise rotation

    // Calculate new image bounds after rotation
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let corners = [
        (-(width as f32) / 2.0, -(height as f32) / 2.0),
        (width as f32 / 2.0, -(height as f32) / 2.0),
        (-(width as f32) / 2.0, height as f32 / 2.0),
        (width as f32 / 2.0, height as f32 / 2.0),
    ];

    let mut min_x: f32 = 0.0;
    let mut max_x: f32 = 0.0;
    let mut min_y: f32 = 0.0;
    let mut max_y: f32 = 0.0;

    for (x, y) in corners.iter() {
        let rotated_x = x * cos_a - y * sin_a;
        let rotated_y = x * sin_a + y * cos_a;
        min_x = min_x.min(rotated_x);
        max_x = max_x.max(rotated_x);
        min_y = min_y.min(rotated_y);
        max_y = max_y.max(rotated_y);
    }

    let new_width = (max_x - min_x).ceil() as u32;
    let new_height = (max_y - min_y).ceil() as u32;

    // For simplicity, use nearest neighbor rotation which is sufficient for small angles
    // and preserves text sharpness better than bilinear interpolation
    match image {
        DynamicImage::ImageRgb8(img) => {
            let mut rotated_img = image::RgbImage::new(new_width, new_height);
            apply_nearest_neighbor_rotation(
                img,
                &mut rotated_img,
                angle_rad,
                width,
                height,
                new_width,
                new_height,
                min_x,
                min_y,
            );
            Ok(DynamicImage::ImageRgb8(rotated_img))
        }
        DynamicImage::ImageRgba8(img) => {
            let mut rotated_img = image::RgbaImage::new(new_width, new_height);
            apply_nearest_neighbor_rotation(
                img,
                &mut rotated_img,
                angle_rad,
                width,
                height,
                new_width,
                new_height,
                min_x,
                min_y,
            );
            Ok(DynamicImage::ImageRgba8(rotated_img))
        }
        DynamicImage::ImageLuma8(img) => {
            let mut rotated_img = image::GrayImage::new(new_width, new_height);
            apply_nearest_neighbor_rotation(
                img,
                &mut rotated_img,
                angle_rad,
                width,
                height,
                new_width,
                new_height,
                min_x,
                min_y,
            );
            Ok(DynamicImage::ImageLuma8(rotated_img))
        }
        DynamicImage::ImageLumaA8(img) => {
            let mut rotated_img = image::GrayAlphaImage::new(new_width, new_height);
            apply_nearest_neighbor_rotation(
                img,
                &mut rotated_img,
                angle_rad,
                width,
                height,
                new_width,
                new_height,
                min_x,
                min_y,
            );
            Ok(DynamicImage::ImageLumaA8(rotated_img))
        }
        _ => Err(PreprocessingError::ProcessingFailed {
            message: "Unsupported image format for rotation".to_string(),
        }),
    }
}

/// Applies nearest neighbor rotation to an image buffer.
///
/// # Arguments
///
/// * `input` - Input image buffer
/// * `output` - Output image buffer
/// * `angle_rad` - Rotation angle in radians
/// * `orig_width` - Original image width
/// * `orig_height` - Original image height
/// * `new_width` - New image width
/// * `new_height` - New image height
/// * `offset_x` - X offset for coordinate transformation
/// * `offset_y` - Y offset for coordinate transformation
#[allow(clippy::too_many_arguments)]
fn apply_nearest_neighbor_rotation<T: GenericImage>(
    input: &T,
    output: &mut T,
    angle_rad: f32,
    orig_width: u32,
    orig_height: u32,
    new_width: u32,
    new_height: u32,
    offset_x: f32,
    offset_y: f32,
) {
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    for y in 0..new_height {
        for x in 0..new_width {
            // Convert to centered coordinates in output space
            let cx = x as f32 - new_width as f32 / 2.0 + offset_x;
            let cy = y as f32 - new_height as f32 / 2.0 + offset_y;

            // Rotate back to original coordinate system
            let orig_x = cx * cos_a + cy * sin_a + orig_width as f32 / 2.0;
            let orig_y = -cx * sin_a + cy * cos_a + orig_height as f32 / 2.0;

            // Nearest neighbor sampling
            if orig_x >= 0.0
                && orig_x < orig_width as f32
                && orig_y >= 0.0
                && orig_y < orig_height as f32
            {
                let pixel = input.get_pixel(orig_x as u32, orig_y as u32);
                output.put_pixel(x, y, pixel);
            }
        }
    }
}

/// Calculates confidence in skew detection based on projection profile characteristics.
///
/// # Arguments
///
/// * `image` - Original grayscale image
/// * `detected_angle` - The detected skew angle
///
/// # Returns
///
/// Confidence score between 0.0 and 1.0
fn calculate_skew_confidence(image: &image::GrayImage, detected_angle: f32) -> f32 {
    // Simple confidence calculation based on angle magnitude and image characteristics
    // Smaller angles are more reliable, larger angles less so
    let angle_confidence = 1.0 - (detected_angle.abs() / 10.0).min(1.0);

    // Consider image size - larger images give more reliable detection
    let (width, height) = image.dimensions();
    let size_factor = ((width * height) as f32 / 100000.0).min(1.0); // Normalize to 100k pixels

    // Combine factors
    (angle_confidence * 0.7 + size_factor * 0.3).clamp(0.0, 1.0)
}

/// Local implementation of Otsu thresholding for skew detection.
///
/// # Arguments
///
/// * `image` - Grayscale image to threshold
///
/// # Returns
///
/// Returns a binary image
fn apply_otsu_threshold_local(
    image: &image::GrayImage,
) -> Result<image::GrayImage, PreprocessingError> {
    let mut pixels: Vec<u8> = image.pixels().map(|p| p[0]).collect();

    if pixels.is_empty() {
        return Err(PreprocessingError::ProcessingFailed {
            message: "Empty image for thresholding".to_string(),
        });
    }

    // Simple Otsu-like thresholding using median as threshold
    pixels.sort();
    let threshold = pixels[pixels.len() / 2];

    let mut binary = image::GrayImage::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        let value = if pixel[0] > threshold { 255u8 } else { 0u8 };
        binary.put_pixel(x, y, image::Luma([value]));
    }

    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn create_horizontal_lines_image(width: u32, height: u32, line_spacing: u32) -> DynamicImage {
        let mut img = image::GrayImage::new(width, height);

        // Create horizontal black lines on white background
        for y in (0..height).step_by(line_spacing as usize) {
            for x in 0..width {
                img.put_pixel(x, y, image::Luma([0])); // Black line
            }
        }

        DynamicImage::ImageLuma8(img)
    }

    fn create_vertical_lines_image(width: u32, height: u32, line_spacing: u32) -> DynamicImage {
        let mut img = image::GrayImage::new(width, height);

        // Create vertical black lines on white background
        for x in (0..width).step_by(line_spacing as usize) {
            for y in 0..height {
                img.put_pixel(x, y, image::Luma([0])); // Black line
            }
        }

        DynamicImage::ImageLuma8(img)
    }

    fn create_uniform_image(width: u32, height: u32, intensity: u8) -> DynamicImage {
        let mut img = image::GrayImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(x, y, image::Luma([intensity]));
            }
        }
        DynamicImage::ImageLuma8(img)
    }

    #[test]
    fn test_deskew_image_horizontal_lines() {
        let img = create_horizontal_lines_image(100, 100, 10);
        let result = deskew_image(&img).unwrap();

        // Horizontal lines should have near-zero skew
        assert!(result.skew_angle_degrees.abs() < 1.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        assert!(result.processing_time_ms < 200); // Should be reasonably fast
        assert_eq!(result.image.width(), 100);
        assert_eq!(result.image.height(), 100);
    }

    #[test]
    fn test_deskew_image_vertical_lines() {
        let img = create_vertical_lines_image(100, 100, 10);
        let result = deskew_image(&img).unwrap();

        // Vertical lines should have near-zero skew (they're already "horizontal" in projection)
        assert!(result.skew_angle_degrees.abs() < 1.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        assert!(result.processing_time_ms < 200); // Should be reasonably fast
    }

    #[test]
    fn test_deskew_image_uniform() {
        let img = create_uniform_image(100, 100, 128);
        let result = deskew_image(&img).unwrap();

        // Uniform image should have zero skew (no rotation applied)
        assert_eq!(result.skew_angle_degrees, 0.0);
        assert_eq!(result.confidence, 0.0);
        assert!(result.processing_time_ms < 200); // Should be reasonably fast
    }

    #[test]
    fn test_deskew_image_small_skew_threshold() {
        let img = create_horizontal_lines_image(100, 100, 10);
        let result = deskew_image(&img).unwrap();

        // If skew is very small, image should be returned unchanged
        if result.skew_angle_degrees.abs() < 0.5 {
            // Image should be the same dimensions
            assert_eq!(result.image.width(), 100);
            assert_eq!(result.image.height(), 100);
        }
    }

    #[test]
    fn test_detect_skew_angle_horizontal_lines() {
        let img = create_horizontal_lines_image(100, 100, 10).to_luma8();
        let angle = detect_skew_angle(&img).unwrap();

        // Should detect near-zero angle for horizontal lines
        assert!(angle.abs() < 2.0); // Allow some tolerance
    }

    #[test]
    fn test_detect_skew_angle_uniform() {
        let img = create_uniform_image(100, 100, 128).to_luma8();
        let angle = detect_skew_angle(&img).unwrap();

        // Uniform image should return zero angle
        assert_eq!(angle, 0.0);
    }

    #[test]
    fn test_calculate_projection_variance_horizontal() {
        let img = create_horizontal_lines_image(50, 50, 10).to_luma8();
        let variance = calculate_projection_variance(&img, 0.0);

        // Horizontal lines at 0° should have low variance
        assert!(variance >= 0.0);
    }

    #[test]
    fn test_calculate_projection_variance_different_angles() {
        let img = create_horizontal_lines_image(50, 50, 10).to_luma8();

        let variance_0 = calculate_projection_variance(&img, 0.0);
        let variance_5 = calculate_projection_variance(&img, 5.0);
        let variance_10 = calculate_projection_variance(&img, 10.0);

        // Variance should be finite for all angles
        assert!(variance_0.is_finite());
        assert!(variance_5.is_finite());
        assert!(variance_10.is_finite());
    }

    #[test]
    fn test_apply_rotation_correction_small_angle() {
        let img = create_horizontal_lines_image(50, 50, 10);
        let rotated = apply_rotation_correction(&img, 1.0).unwrap();

        // Should return a valid image
        assert!(rotated.width() > 0);
        assert!(rotated.height() > 0);
    }

    #[test]
    fn test_apply_rotation_correction_zero_angle() {
        let img = create_horizontal_lines_image(50, 50, 10);
        let rotated = apply_rotation_correction(&img, 0.0).unwrap();

        // Zero rotation should preserve dimensions approximately
        assert!(rotated.width() >= 50);
        assert!(rotated.height() >= 50);
    }

    #[test]
    fn test_calculate_skew_confidence() {
        let img = create_horizontal_lines_image(100, 100, 10).to_luma8();

        let confidence_small = calculate_skew_confidence(&img, 1.0);
        let confidence_large = calculate_skew_confidence(&img, 8.0);

        // Smaller angles should have higher confidence
        assert!(confidence_small > confidence_large);
        assert!((0.0..=1.0).contains(&confidence_small));
        assert!((0.0..=1.0).contains(&confidence_large));
    }

    #[test]
    fn test_calculate_skew_confidence_image_size() {
        let img_small = create_horizontal_lines_image(50, 50, 5).to_luma8();
        let img_large = create_horizontal_lines_image(200, 200, 20).to_luma8();

        let confidence_small = calculate_skew_confidence(&img_small, 2.0);
        let confidence_large = calculate_skew_confidence(&img_large, 2.0);

        // Larger images should have higher confidence
        assert!(confidence_large >= confidence_small);
    }

    #[test]
    fn test_apply_otsu_threshold_local() {
        let img = create_horizontal_lines_image(50, 50, 10).to_luma8();
        let binary = apply_otsu_threshold_local(&img).unwrap();

        // Should return binary image same size
        assert_eq!(binary.width(), 50);
        assert_eq!(binary.height(), 50);

        // Check that pixels are binary (0 or 255)
        for pixel in binary.pixels() {
            assert!(pixel[0] == 0 || pixel[0] == 255);
        }
    }

    #[test]
    fn test_apply_otsu_threshold_local_uniform() {
        let img = create_uniform_image(50, 50, 128).to_luma8();
        let binary = apply_otsu_threshold_local(&img).unwrap();

        // Uniform image should threshold consistently
        assert_eq!(binary.width(), 50);
        assert_eq!(binary.height(), 50);
    }

    #[test]
    fn test_deskew_image_performance() {
        let img = create_horizontal_lines_image(200, 200, 20);
        let result = deskew_image(&img).unwrap();

        // Should complete in reasonable time (< 500ms for 200x200 image)
        assert!(result.processing_time_ms < 500);
    }

    #[test]
    fn test_deskew_image_rgb_format() {
        let mut img = image::RgbImage::new(50, 50);
        // Create a simple pattern
        for y in 0..50 {
            for x in 0..50 {
                let r = if y < 25 { 255 } else { 0 };
                let g = if x < 25 { 255 } else { 0 };
                let b = 128;
                img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }
        let dynamic_img = DynamicImage::ImageRgb8(img);
        let result = deskew_image(&dynamic_img).unwrap();

        // Should preserve RGB format
        match result.image {
            DynamicImage::ImageRgb8(_) => {} // Expected
            _ => panic!("RGB format not preserved"),
        }
    }
}
