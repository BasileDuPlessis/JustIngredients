//! Preparation of downloaded images for extraction: JPEG, PNG and WebP are sent as they are,
//! other formats the `image` crate can decode (BMP, TIFF, GIF, …) are re-encoded as JPEG
//! (photos re-encoded as PNG often exceed the size limit).
//!
//! Decoding runs on the blocking thread pool, one conversion at a time, with bounded
//! dimensions and allocation, so a crafted file (huge dimensions in a tiny file) or several
//! conversions at once cannot exhaust the 256 MB machine.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageReader, Limits};
use tokio::sync::Semaphore;

use crate::extraction::{ExtractionError, ImageInput, ImageMime};

/// Largest image accepted for extraction, before and after conversion.
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Decoding limits for conversions: far above a phone photo, far below the machine's memory.
const MAX_DIMENSION: u32 = 8_000;
const MAX_DECODE_ALLOC: u64 = 96 * 1024 * 1024;

/// JPEG quality for converted images: text stays sharp, files stay small.
const JPEG_QUALITY: u8 = 90;

/// One conversion at a time: each holds the input, the decoded pixels and the output.
static CONVERSIONS: Semaphore = Semaphore::const_new(1);

impl ImageInput {
    /// Prepares downloaded bytes for extraction, converting unsupported formats to JPEG.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::ImageTooLarge`] above [`MAX_IMAGE_BYTES`];
    /// [`ExtractionError::UnsupportedImage`] when the bytes are not a decodable image or exceed
    /// the decoding limits.
    pub async fn prepare(bytes: Vec<u8>) -> Result<Self, ExtractionError> {
        ensure_size(bytes.len())?;
        if ImageMime::sniff(&bytes).is_some() {
            return Self::from_bytes(bytes);
        }
        let _permit = CONVERSIONS.acquire().await.map_err(|_| {
            ExtractionError::UnsupportedImage("image conversion unavailable".to_string())
        })?;
        let jpeg = tokio::task::spawn_blocking(move || convert_to_jpeg(&bytes))
            .await
            .map_err(|error| {
                ExtractionError::UnsupportedImage(format!("image conversion failed: {error}"))
            })??;
        ensure_size(jpeg.len())?;
        Self::from_bytes(jpeg)
    }
}

fn ensure_size(len: usize) -> Result<(), ExtractionError> {
    let size = len as u64;
    if size > MAX_IMAGE_BYTES {
        return Err(ExtractionError::ImageTooLarge {
            size,
            max: MAX_IMAGE_BYTES,
        });
    }
    Ok(())
}

fn convert_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>, ExtractionError> {
    let unsupported = |reason: String| ExtractionError::UnsupportedImage(reason);
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| unsupported(format!("unreadable image: {error}")))?;
    if reader.format().is_none() {
        return Err(unsupported("not an image".to_string()));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);

    let decoded = reader
        .decode()
        .map_err(|error| unsupported(format!("cannot decode image: {error}")))?;
    // JPEG has no alpha channel: flatten to RGB first.
    let rgb = image::DynamicImage::ImageRgb8(decoded.into_rgb8());
    let mut jpeg = Vec::new();
    rgb.write_with_encoder(JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY))
        .map_err(|error| unsupported(format!("cannot encode JPEG: {error}")))?;
    Ok(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, ImageFormat, Rgb};

    fn encode(format: ImageFormat) -> Result<Vec<u8>, image::ImageError> {
        let pixels: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(4, 3, Rgb([200, 10, 10]));
        let mut out = Cursor::new(Vec::new());
        pixels.write_to(&mut out, format)?;
        Ok(out.into_inner())
    }

    #[tokio::test]
    async fn supported_formats_are_kept_as_they_are() -> Result<(), Box<dyn std::error::Error>> {
        for (format, mime) in [
            (ImageFormat::Png, ImageMime::Png),
            (ImageFormat::Jpeg, ImageMime::Jpeg),
        ] {
            let bytes = encode(format)?;
            let image = ImageInput::prepare(bytes.clone()).await?;
            assert_eq!(image.mime(), mime);
            assert_eq!(image.bytes(), bytes.as_slice(), "not re-encoded");
        }
        Ok(())
    }

    #[tokio::test]
    async fn other_formats_are_converted_to_jpeg() -> Result<(), Box<dyn std::error::Error>> {
        for format in [ImageFormat::Bmp, ImageFormat::Tiff, ImageFormat::Gif] {
            let image = ImageInput::prepare(encode(format)?).await?;
            assert_eq!(image.mime(), ImageMime::Jpeg, "{format:?}");
            let decoded = image::load_from_memory(image.bytes())?;
            assert_eq!((decoded.width(), decoded.height()), (4, 3));
        }
        Ok(())
    }

    #[tokio::test]
    async fn non_images_and_oversized_files_are_rejected() {
        assert!(matches!(
            ImageInput::prepare(b"%PDF-1.7 not an image".to_vec()).await,
            Err(ExtractionError::UnsupportedImage(_))
        ));
        let oversized = vec![0xFF; MAX_IMAGE_BYTES as usize + 1];
        assert!(matches!(
            ImageInput::prepare(oversized).await,
            Err(ExtractionError::ImageTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn huge_dimensions_in_a_small_file_are_rejected() -> Result<(), Box<dyn std::error::Error>>
    {
        // A valid 4x3 BMP whose header claims 30000x30000 pixels (BMP width at byte 18, height
        // at byte 22, little-endian i32): decoding it would need gigabytes.
        let mut bmp = encode(ImageFormat::Bmp)?;
        bmp[18..22].copy_from_slice(&30_000i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&30_000i32.to_le_bytes());
        assert!(matches!(
            ImageInput::prepare(bmp).await,
            Err(ExtractionError::UnsupportedImage(_))
        ));
        Ok(())
    }
}
