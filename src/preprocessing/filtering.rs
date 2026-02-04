//! # Image Filtering Module
//!
//! This module provides noise reduction and morphological operations for OCR preprocessing.
//! It includes Gaussian blur for noise reduction and morphological operations for cleaning binary images.

use image::DynamicImage;
use tracing;

use super::types::{
    ClaheImageResult, DenoisedImageResult, MorphologicalImageResult, MorphologicalOperation,
    PreprocessingError,
};

/// Applies Gaussian blur to reduce image noise while preserving text edges.
///
/// This function uses Gaussian blur with a configurable sigma value to reduce
/// salt-and-pepper noise and other high-frequency noise that can interfere with
/// OCR accuracy. The blur is applied before thresholding to improve the quality
/// of the binary image.
///
/// # Arguments
///
/// * `image` - The input image to denoise
/// * `sigma` - Standard deviation for Gaussian kernel (recommended: 1.0-1.5)
///
/// # Returns
///
/// Returns a `Result` containing the denoised image and metadata, or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::reduce_noise;
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("noisy_recipe.jpg")?;
/// let denoised = reduce_noise(&img, 1.2)?;
/// // denoised.image has reduced noise while preserving text edges
/// # Ok(())
/// # }
/// ```
pub fn reduce_noise(
    image: &DynamicImage,
    sigma: f32,
) -> Result<DenoisedImageResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Validate sigma parameter
    if sigma <= 0.0 || sigma > 5.0 {
        return Err(PreprocessingError::ProcessingFailed {
            message: format!(
                "Invalid sigma value: {}. Must be between 0.1 and 5.0",
                sigma
            ),
        });
    }

    // Apply Gaussian blur using image crate's built-in filter
    // The image crate's blur function uses a Gaussian kernel
    let blurred = image.blur(sigma);

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "Noise reduction completed in {:.2}ms: sigma={:.2}, dimensions={}x{}",
        processing_time.as_millis(),
        sigma,
        blurred.width(),
        blurred.height()
    );

    Ok(DenoisedImageResult {
        image: blurred,
        sigma,
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Applies morphological operations to clean up binary images.
///
/// Morphological operations are used to remove noise, fill gaps, and improve
/// the quality of binary images for better OCR accuracy. This function supports
/// erosion, dilation, opening, and closing operations using a 3x3 kernel.
///
/// # Arguments
///
/// * `image` - The input binary image to process
/// * `operation` - The morphological operation to apply
///
/// # Returns
///
/// Returns a `Result` containing the processed image and metadata, or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::{apply_morphological_operation, MorphologicalOperation};
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("binary_recipe.png")?;
/// let cleaned = apply_morphological_operation(&img, MorphologicalOperation::Opening)?;
/// // cleaned.image has reduced noise while preserving text structure
/// # Ok(())
/// # }
/// ```
pub fn apply_morphological_operation(
    image: &DynamicImage,
    operation: MorphologicalOperation,
) -> Result<MorphologicalImageResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Convert to grayscale if not already
    let gray = image.to_luma8();

    // Apply the specified morphological operation
    let processed = match operation {
        MorphologicalOperation::Erosion => apply_erosion(&gray),
        MorphologicalOperation::Dilation => apply_dilation(&gray),
        MorphologicalOperation::Opening => {
            // Opening: erosion followed by dilation
            let eroded = apply_erosion(&gray);
            apply_dilation(&eroded)
        }
        MorphologicalOperation::Closing => {
            // Closing: dilation followed by erosion
            let dilated = apply_dilation(&gray);
            apply_erosion(&dilated)
        }
    };

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "Morphological operation completed in {:.2}ms: operation={:?}, dimensions={}x{}",
        processing_time.as_millis(),
        operation,
        processed.width(),
        processed.height()
    );

    Ok(MorphologicalImageResult {
        image: DynamicImage::ImageLuma8(processed),
        operation,
        kernel_size: 3, // Using 3x3 kernel as specified
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Applies erosion morphological operation using a 3x3 kernel.
///
/// Erosion shrinks bright regions and removes small bright artifacts.
/// For binary images, this removes small white noise particles.
///
/// # Arguments
///
/// * `image` - The input grayscale/binary image
///
/// # Returns
///
/// Returns the eroded image
fn apply_erosion(image: &image::GrayImage) -> image::GrayImage {
    let (width, height) = image.dimensions();
    let mut result = image::GrayImage::new(width, height);

    // 3x3 kernel for erosion (min operation)
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            // Find minimum value in 3x3 neighborhood
            let mut min_val = 255u8;
            for ky in -1..=1 {
                for kx in -1..=1 {
                    let nx = (x as i32 + kx) as u32;
                    let ny = (y as i32 + ky) as u32;
                    let pixel = image.get_pixel(nx, ny);
                    min_val = min_val.min(pixel[0]);
                }
            }
            result.put_pixel(x, y, image::Luma([min_val]));
        }
    }

    result
}

/// Applies dilation morphological operation using a 3x3 kernel.
///
/// Dilation expands bright regions and can fill small dark gaps.
/// For binary images, this can fill small gaps in text characters.
///
/// # Arguments
///
/// * `image` - The input grayscale/binary image
///
/// # Returns
///
/// Returns the dilated image
fn apply_dilation(image: &image::GrayImage) -> image::GrayImage {
    let (width, height) = image.dimensions();
    let mut result = image::GrayImage::new(width, height);

    // 3x3 kernel for dilation (max operation)
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            // Find maximum value in 3x3 neighborhood
            let mut max_val = 0u8;
            for ky in -1..=1 {
                for kx in -1..=1 {
                    let nx = (x as i32 + kx) as u32;
                    let ny = (y as i32 + ky) as u32;
                    let pixel = image.get_pixel(nx, ny);
                    max_val = max_val.max(pixel[0]);
                }
            }
            result.put_pixel(x, y, image::Luma([max_val]));
        }
    }

    result
}

/// Applies Contrast Limited Adaptive Histogram Equalization (CLAHE) to enhance local contrast.
///
/// CLAHE improves local contrast by applying histogram equalization to small regions
/// (tiles) of the image, with a clip limit to prevent noise amplification. This is
/// particularly effective for images with varying lighting conditions or low contrast.
///
/// # Arguments
///
/// * `image` - The input image to enhance
/// * `clip_limit` - Maximum value for histogram clipping (recommended: 2.0-4.0)
/// * `tile_size` - Size of tiles for local equalization (recommended: (8, 8) to (16, 16))
///
/// # Returns
///
/// Returns a `Result` containing the contrast-enhanced image and metadata, or a `PreprocessingError`
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::apply_clahe;
/// use image::open;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let img = open("low_contrast_recipe.jpg")?;
/// let enhanced = apply_clahe(&img, 3.0, (8, 8))?;
/// // enhanced.image has improved local contrast
/// # Ok(())
/// # }
/// ```
pub fn apply_clahe(
    image: &DynamicImage,
    clip_limit: f32,
    tile_size: (u32, u32),
) -> Result<ClaheImageResult, PreprocessingError> {
    let start_time = std::time::Instant::now();

    // Validate parameters
    if clip_limit <= 0.0 {
        return Err(PreprocessingError::ProcessingFailed {
            message: format!("Invalid clip limit: {}. Must be > 0.0", clip_limit),
        });
    }

    if tile_size.0 == 0 || tile_size.1 == 0 {
        return Err(PreprocessingError::ProcessingFailed {
            message: "Invalid tile size: dimensions must be > 0".to_string(),
        });
    }

    // Convert to grayscale for CLAHE processing
    let gray = image.to_luma8();
    let (width, height) = gray.dimensions();

    // Ensure tile size is not larger than image
    let tile_width = tile_size.0.min(width);
    let tile_height = tile_size.1.min(height);

    // Calculate number of tiles
    let tiles_x = width.div_ceil(tile_width) as usize;
    let tiles_y = height.div_ceil(tile_height) as usize;

    // Create output image
    let mut output = image::GrayImage::new(width, height);

    // Process each tile
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let tile_start_x = tile_x as u32 * tile_width;
            let tile_start_y = tile_y as u32 * tile_height;
            let tile_end_x = (tile_start_x + tile_width).min(width);
            let tile_end_y = (tile_start_y + tile_height).min(height);

            // Extract tile
            let tile = extract_tile(&gray, tile_start_x, tile_start_y, tile_end_x, tile_end_y);

            // Apply CLAHE to tile
            let enhanced_tile = apply_clahe_to_tile(&tile, clip_limit);

            // Copy enhanced tile back to output
            copy_tile_to_output(&mut output, &enhanced_tile, tile_start_x, tile_start_y);
        }
    }

    let processing_time = start_time.elapsed();

    tracing::debug!(
        target: "ocr_preprocessing",
        "CLAHE applied in {:.2}ms: clip_limit={}, tile_size={:?}",
        processing_time.as_millis(),
        clip_limit,
        tile_size
    );

    Ok(ClaheImageResult {
        image: DynamicImage::ImageLuma8(output),
        clip_limit,
        tile_size,
        processing_time_ms: processing_time.as_millis() as u32,
    })
}

/// Extracts a tile from the image.
fn extract_tile(
    image: &image::GrayImage,
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
) -> image::GrayImage {
    let tile_width = end_x - start_x;
    let tile_height = end_y - start_y;
    let mut tile = image::GrayImage::new(tile_width, tile_height);

    for y in 0..tile_height {
        for x in 0..tile_width {
            let pixel = image.get_pixel(start_x + x, start_y + y);
            tile.put_pixel(x, y, *pixel);
        }
    }

    tile
}

/// Applies CLAHE to a single tile.
fn apply_clahe_to_tile(tile: &image::GrayImage, clip_limit: f32) -> image::GrayImage {
    let (width, height) = tile.dimensions();
    let total_pixels = (width * height) as f32;

    // Calculate histogram
    let mut histogram = [0u32; 256];
    for pixel in tile.pixels() {
        let intensity = pixel[0] as usize;
        histogram[intensity] += 1;
    }

    // Apply clip limit
    let clip_limit_pixels = (clip_limit * (total_pixels / 256.0)).round() as u32;
    let mut excess_pixels = 0u32;

    for count in &mut histogram {
        if *count > clip_limit_pixels {
            excess_pixels += *count - clip_limit_pixels;
            *count = clip_limit_pixels;
        }
    }

    // Redistribute excess pixels uniformly
    let uniform_increment = excess_pixels / 256;
    let mut remainder = excess_pixels % 256;

    for count in &mut histogram {
        *count += uniform_increment;
        if remainder > 0 {
            *count += 1;
            remainder -= 1;
        }
    }

    // Calculate cumulative distribution function (CDF)
    let mut cdf = [0.0f32; 256];
    let mut cumulative = 0.0;

    for i in 0..256 {
        cumulative += histogram[i] as f32 / total_pixels;
        cdf[i] = cumulative;
    }

    // Apply histogram equalization
    let mut result = image::GrayImage::new(width, height);
    for (x, y, pixel) in tile.enumerate_pixels() {
        let intensity = pixel[0] as usize;
        let new_intensity = (cdf[intensity] * 255.0).round() as u8;
        result.put_pixel(x, y, image::Luma([new_intensity]));
    }

    result
}

/// Copies a tile back to the output image.
fn copy_tile_to_output(
    output: &mut image::GrayImage,
    tile: &image::GrayImage,
    start_x: u32,
    start_y: u32,
) {
    let (tile_width, tile_height) = tile.dimensions();

    for y in 0..tile_height {
        for x in 0..tile_width {
            let pixel = tile.get_pixel(x, y);
            output.put_pixel(start_x + x, start_y + y, *pixel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn create_test_image(width: u32, height: u32) -> DynamicImage {
        let img = image::RgbImage::new(width, height);
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_reduce_noise_basic() {
        let img = create_test_image(100, 100);
        let result = reduce_noise(&img, 1.0).unwrap();

        // Check that result has expected properties
        assert!(result.sigma == 1.0);
        assert!(result.processing_time_ms >= 0);
        assert!(result.image.width() == 100);
        assert!(result.image.height() == 100);
    }

    #[test]
    fn test_reduce_noise_invalid_sigma() {
        let img = create_test_image(50, 50);

        // Test sigma too low
        let result = reduce_noise(&img, 0.0);
        assert!(result.is_err());

        // Test sigma too high
        let result = reduce_noise(&img, 6.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_reduce_noise_different_sigma_values() {
        let img = create_test_image(50, 50);

        // Test various valid sigma values
        let sigmas = [0.5, 1.0, 1.5, 2.0];
        for &sigma in &sigmas {
            let result = reduce_noise(&img, sigma).unwrap();
            assert_eq!(result.sigma, sigma);
            assert!(result.processing_time_ms >= 0);
        }
    }

    #[test]
    fn test_reduce_noise_preserves_image_format() {
        let rgb_img = create_test_image(50, 50);
        let result = reduce_noise(&rgb_img, 1.0).unwrap();

        // Should preserve RGB format
        match result.image {
            DynamicImage::ImageRgb8(_) => {} // Expected
            _ => panic!("Image format not preserved"),
        }
    }

    #[test]
    fn test_reduce_noise_performance() {
        let img = create_test_image(200, 200);
        let result = reduce_noise(&img, 1.2).unwrap();

        // Should complete in reasonable time (< 100ms for 200x200 image)
        assert!(result.processing_time_ms < 100);
    }

    #[test]
    fn test_apply_morphological_operation_erosion() {
        let img = create_test_image(50, 50);
        let result = apply_morphological_operation(&img, MorphologicalOperation::Erosion).unwrap();

        assert_eq!(result.operation, MorphologicalOperation::Erosion);
        assert_eq!(result.kernel_size, 3);
        assert!(result.processing_time_ms >= 0);
    }

    #[test]
    fn test_apply_morphological_operation_dilation() {
        let img = create_test_image(50, 50);
        let result = apply_morphological_operation(&img, MorphologicalOperation::Dilation).unwrap();

        assert_eq!(result.operation, MorphologicalOperation::Dilation);
        assert_eq!(result.kernel_size, 3);
        assert!(result.processing_time_ms >= 0);
    }

    #[test]
    fn test_apply_morphological_operation_opening() {
        let img = create_test_image(50, 50);
        let result = apply_morphological_operation(&img, MorphologicalOperation::Opening).unwrap();

        assert_eq!(result.operation, MorphologicalOperation::Opening);
        assert_eq!(result.kernel_size, 3);
        assert!(result.processing_time_ms >= 0);
    }

    #[test]
    fn test_apply_morphological_operation_closing() {
        let img = create_test_image(50, 50);
        let result = apply_morphological_operation(&img, MorphologicalOperation::Closing).unwrap();

        assert_eq!(result.operation, MorphologicalOperation::Closing);
        assert_eq!(result.kernel_size, 3);
        assert!(result.processing_time_ms >= 0);
    }

    #[test]
    fn test_apply_erosion_basic() {
        let mut img = image::GrayImage::new(5, 5);

        // Create a simple pattern: white square with black center
        for y in 0..5 {
            for x in 0..5 {
                let value = if x == 2 && y == 2 { 0 } else { 255 };
                img.put_pixel(x, y, image::Luma([value]));
            }
        }

        let eroded = apply_erosion(&img);

        // Erosion should shrink bright regions (expand dark regions)
        // The center pixel should remain black (dark region expands)
        let center_pixel = eroded.get_pixel(2, 2);
        assert_eq!(center_pixel[0], 0); // Should remain black (erosion preserves dark regions)
    }

    #[test]
    fn test_apply_dilation_basic() {
        let mut img = image::GrayImage::new(5, 5);

        // Create a simple pattern: black square with white center
        for y in 0..5 {
            for x in 0..5 {
                let value = if x == 2 && y == 2 { 255 } else { 0 };
                img.put_pixel(x, y, image::Luma([value]));
            }
        }

        let dilated = apply_dilation(&img);

        // Dilation should expand bright regions
        // Pixels around the center should become white
        let adjacent_pixels = [
            dilated.get_pixel(1, 2), // left
            dilated.get_pixel(3, 2), // right
            dilated.get_pixel(2, 1), // top
            dilated.get_pixel(2, 3), // bottom
        ];

        for pixel in adjacent_pixels {
            assert_eq!(pixel[0], 255); // Should be dilated (expanded bright region)
        }
    }

    #[test]
    fn test_morphological_operations_performance() {
        let img = create_test_image(100, 100);

        let operations = [
            MorphologicalOperation::Erosion,
            MorphologicalOperation::Dilation,
            MorphologicalOperation::Opening,
            MorphologicalOperation::Closing,
        ];

        for operation in operations {
            let result = apply_morphological_operation(&img, operation).unwrap();
            // Should complete in reasonable time (< 50ms for 100x100 image)
            assert!(result.processing_time_ms < 50);
        }
    }

    #[test]
    fn test_morphological_operations_preserve_image_format() {
        let rgb_img = create_test_image(50, 50);
        let result =
            apply_morphological_operation(&rgb_img, MorphologicalOperation::Opening).unwrap();

        // Should convert to grayscale for processing but return DynamicImage
        assert!(result.image.width() == 50);
        assert!(result.image.height() == 50);
    }

    #[test]
    fn test_apply_clahe_basic() {
        let img = create_test_image(100, 100);
        let result = apply_clahe(&img, 3.0, (8, 8)).unwrap();

        assert_eq!(result.clip_limit, 3.0);
        assert_eq!(result.tile_size, (8, 8));
        assert!(result.processing_time_ms >= 0);
        assert_eq!(result.image.width(), 100);
        assert_eq!(result.image.height(), 100);
    }

    #[test]
    fn test_apply_clahe_invalid_parameters() {
        let img = create_test_image(50, 50);

        // Test invalid clip limit
        let result = apply_clahe(&img, 0.0, (8, 8));
        assert!(result.is_err());

        // Test invalid tile size
        let result = apply_clahe(&img, 3.0, (0, 8));
        assert!(result.is_err());

        let result = apply_clahe(&img, 3.0, (8, 0));
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_clahe_different_tile_sizes() {
        let img = create_test_image(64, 64);

        let tile_sizes = [(4, 4), (8, 8), (16, 16), (32, 32)];

        for &tile_size in &tile_sizes {
            let result = apply_clahe(&img, 2.0, tile_size).unwrap();
            assert_eq!(result.tile_size, tile_size);
            assert_eq!(result.image.width(), 64);
            assert_eq!(result.image.height(), 64);
        }
    }

    #[test]
    fn test_apply_clahe_different_clip_limits() {
        let img = create_test_image(50, 50);

        let clip_limits = [1.0, 2.0, 3.0, 4.0, 5.0];

        for &clip_limit in &clip_limits {
            let result = apply_clahe(&img, clip_limit, (8, 8)).unwrap();
            assert_eq!(result.clip_limit, clip_limit);
        }
    }

    #[test]
    fn test_apply_clahe_performance() {
        let img = create_test_image(200, 200);
        let result = apply_clahe(&img, 3.0, (8, 8)).unwrap();

        // Should complete in reasonable time (< 200ms for 200x200 image)
        assert!(result.processing_time_ms < 200);
    }

    #[test]
    fn test_apply_clahe_preserves_image_format() {
        let rgb_img = create_test_image(50, 50);
        let result = apply_clahe(&rgb_img, 3.0, (8, 8)).unwrap();

        // Should return grayscale image
        match result.image {
            DynamicImage::ImageLuma8(_) => {} // Expected
            _ => panic!("Image should be converted to grayscale"),
        }
    }

    #[test]
    fn test_extract_tile() {
        let mut img = image::GrayImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, image::Luma([((x + y) % 256) as u8]));
            }
        }

        let tile = extract_tile(&img, 2, 3, 7, 8);
        assert_eq!(tile.width(), 5);
        assert_eq!(tile.height(), 5);

        // Check that tile contains correct pixels
        for y in 0..5 {
            for x in 0..5 {
                let expected = ((x + 2) + (y + 3)) % 256;
                assert_eq!(tile.get_pixel(x, y)[0], expected as u8);
            }
        }
    }

    #[test]
    fn test_apply_clahe_to_tile() {
        let mut tile = image::GrayImage::new(4, 4);

        // Create a tile with low contrast (mostly mid-gray values)
        for y in 0..4 {
            for x in 0..4 {
                let value = 100 + (x + y) as u8; // Values from 100-106
                tile.put_pixel(x, y, image::Luma([value]));
            }
        }

        let enhanced = apply_clahe_to_tile(&tile, 2.0);

        // Enhanced tile should have same dimensions
        assert_eq!(enhanced.width(), 4);
        assert_eq!(enhanced.height(), 4);

        // Check that all pixels are valid (0-255 range)
        for pixel in enhanced.pixels() {
            assert!(pixel[0] <= 255);
        }

        // Check that the operation completed (tile is not identical to input)
        let mut different = false;
        for (orig, enh) in tile.pixels().zip(enhanced.pixels()) {
            if orig[0] != enh[0] {
                different = true;
                break;
            }
        }
        // CLAHE should modify at least some pixels
        assert!(different, "CLAHE should modify pixel values");
    }
}
