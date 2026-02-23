//! # Image Cropping Module
//!
//! This module provides image cropping functionality for isolating specific regions
//! of interest, particularly for targeted OCR operations on measurement quantities.

use std::time::Instant;
use tracing;

use super::types::{CroppedImageResult, PreprocessingError};
use crate::ocr::BBox;

/// Crops a specific region from an image based on a bounding box, targeting the left portion
/// where measurement quantities are typically located.
///
/// This function isolates the left 20% of the bounding box width (with 7-pixel padding)
/// to focus OCR on the quantity portion of ingredient text.
///
/// # Arguments
///
/// * `image_path` - Path to the image file to crop
/// * `bbox` - Bounding box defining the region of interest
///
/// # Returns
///
/// Returns a `CroppedImageResult` containing the cropped image and metadata,
/// or a `PreprocessingError` if the operation fails.
///
/// # Examples
///
/// ```no_run
/// use just_ingredients::preprocessing::crop_measurement_region;
/// use just_ingredients::ocr::BBox;
///
/// let bbox = BBox::new(100, 50, 300, 80);
/// let result = crop_measurement_region("image.jpg", &bbox)?;
/// println!("Cropped region: {}x{}", result.image.width(), result.image.height());
/// # Ok::<(), just_ingredients::preprocessing::types::PreprocessingError>(())
/// ```
pub fn crop_measurement_region(
    image_path: &str,
    bbox: &BBox,
) -> Result<CroppedImageResult, PreprocessingError> {
    let start_time = Instant::now();

    // Load the image
    let mut img = image::open(image_path).map_err(|e| PreprocessingError::ImageLoad {
        message: format!("Failed to load image '{}': {}", image_path, e),
    })?;

    // Calculate the crop region
    let crop_region = calculate_measurement_crop_region(bbox);

    // Ensure crop region is within image bounds
    let img_width = img.width();
    let img_height = img.height();

    let safe_crop_x0 = crop_region.x0.min(img_width.saturating_sub(1));
    let safe_crop_y0 = crop_region.y0.min(img_height.saturating_sub(1));
    let safe_crop_x1 = crop_region.x1.min(img_width).max(safe_crop_x0 + 1);
    let safe_crop_y1 = crop_region.y1.min(img_height).max(safe_crop_y0 + 1);

    let safe_crop_region = BBox::new(safe_crop_x0, safe_crop_y0, safe_crop_x1, safe_crop_y1);

    // Crop the image
    let cropped_img = img.crop(
        safe_crop_x0,
        safe_crop_y0,
        safe_crop_x1 - safe_crop_x0,
        safe_crop_y1 - safe_crop_y0,
    );

    let processing_time_ms = start_time.elapsed().as_millis() as u32;

    tracing::debug!(
        "Cropped measurement region from {}x{} image: original bbox {:?}, crop region {:?}, result {}x{}",
        img_width,
        img_height,
        bbox,
        safe_crop_region,
        cropped_img.width(),
        cropped_img.height()
    );

    Ok(CroppedImageResult {
        image: cropped_img,
        original_bbox: bbox.clone(),
        cropped_region: safe_crop_region,
        processing_time_ms,
    })
}

/// Calculates the crop region for measurement extraction from a bounding box.
///
/// Targets the left 20% of the bounding box width with 7-pixel padding to isolate
/// quantity information while providing some context.
///
/// # Arguments
///
/// * `bbox` - The bounding box of the text line containing the measurement
///
/// # Returns
///
/// A new `BBox` defining the crop region
fn calculate_measurement_crop_region(bbox: &BBox) -> BBox {
    let bbox_width = bbox.width();

    // Target the left 20% of the bounding box width for quantity extraction
    let crop_width = (bbox_width as f32 * 0.20) as u32;
    let padding = 7; // 5-10 pixel padding as specified

    // Calculate crop coordinates with padding
    let crop_x0 = bbox.x0.saturating_sub(padding);
    let crop_x1 = (bbox.x0 + crop_width).saturating_add(padding);

    // Include full height with padding
    let crop_y0 = bbox.y0.saturating_sub(padding);
    let crop_y1 = bbox.y1.saturating_add(padding);

    BBox::new(crop_x0, crop_y0, crop_x1, crop_y1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};
    use tempfile::NamedTempFile;

    fn create_test_image(width: u32, height: u32) -> NamedTempFile {
        let mut img = RgbImage::new(width, height);
        // Fill with white
        for pixel in img.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }
        // Add some black pixels to simulate text
        for x in 10..20 {
            for y in 10..20 {
                if x < width && y < height {
                    img.put_pixel(x, y, Rgb([0, 0, 0]));
                }
            }
        }

        let temp_file = match NamedTempFile::with_suffix(".png") {
            Ok(f) => f,
            Err(e) => panic!("Failed to create temp file: {:?}", e),
        };
        match DynamicImage::ImageRgb8(img).save(&temp_file) {
            Ok(_) => {},
            Err(e) => panic!("Failed to save image: {:?}", e),
        };
        temp_file
    }

    #[test]
    fn test_crop_measurement_region_success() {
        let temp_img = create_test_image(100, 50);
        let bbox = BBox::new(20, 10, 80, 40);

        let path_str = match temp_img.path().to_str() {
            Some(s) => s,
            None => panic!("Temp file path is not valid UTF-8"),
        };
        let result = match crop_measurement_region(path_str, &bbox) {
            Ok(r) => r,
            Err(e) => panic!("crop_measurement_region failed: {:?}", e),
        };

        // Verify the cropped image was created
        assert!(result.image.width() > 0);
        assert!(result.image.height() > 0);

        // Verify original bbox is preserved
        assert_eq!(result.original_bbox, bbox);

        // Verify cropped region is within bounds
        assert!(result.cropped_region.x0 <= result.cropped_region.x1);
        assert!(result.cropped_region.y0 <= result.cropped_region.y1);

        // Verify processing time was recorded
        assert!(result.processing_time_ms < 1000); // Should be fast for a small test image
    }

    #[test]
    fn test_crop_measurement_region_bbox_at_edge() {
        let temp_img = create_test_image(100, 50);
        let bbox = BBox::new(0, 0, 30, 20); // BBox at top-left edge

        let path_str = match temp_img.path().to_str() {
            Some(s) => s,
            None => panic!("Temp file path is not valid UTF-8"),
        };
        let result = match crop_measurement_region(path_str, &bbox) {
            Ok(r) => r,
            Err(e) => panic!("crop_measurement_region failed: {:?}", e),
        };

        // Should not go negative and should be clamped to image bounds
        assert_eq!(result.cropped_region.x0, 0);
        assert_eq!(result.cropped_region.y0, 0);
        assert!(result.cropped_region.x1 > 0);
        assert!(result.cropped_region.y1 > 0);
    }

    #[test]
    fn test_crop_measurement_region_invalid_image() {
        let bbox = BBox::new(10, 10, 50, 30);
        let result = crop_measurement_region("nonexistent.jpg", &bbox);

        assert!(matches!(result, Err(PreprocessingError::ImageLoad { .. })));
    }

    #[test]
    fn test_calculate_measurement_crop_region() {
        let bbox = BBox::new(100, 50, 200, 80); // 100x30 bbox

        let crop_region = calculate_measurement_crop_region(&bbox);

        // Expected: left 20% (20px) + 7px padding on each side
        // x0: 100 - 7 = 93
        // x1: 100 + 20 + 7 = 127
        // y0: 50 - 7 = 43
        // y1: 80 + 7 = 87

        assert_eq!(crop_region.x0, 93);
        assert_eq!(crop_region.y0, 43);
        assert_eq!(crop_region.x1, 127);
        assert_eq!(crop_region.y1, 87);
    }

    #[test]
    fn test_calculate_measurement_crop_region_small_bbox() {
        let bbox = BBox::new(5, 5, 15, 15); // 10x10 bbox

        let crop_region = calculate_measurement_crop_region(&bbox);

        // 20% of 10 = 2px width
        // x0: 5 - 7 = 0 (saturating_sub)
        // x1: 5 + 2 + 7 = 14
        // y0: 5 - 7 = 0
        // y1: 15 + 7 = 22

        assert_eq!(crop_region.x0, 0);
        assert_eq!(crop_region.y0, 0);
        assert_eq!(crop_region.x1, 14);
        assert_eq!(crop_region.y1, 22);
    }
}
