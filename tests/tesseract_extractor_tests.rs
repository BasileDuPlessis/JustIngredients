//! The Tesseract fallback behind the extractor interface (`EXTRACTION_BACKEND=tesseract`),
//! on the recipe test image. Needs Tesseract installed (it is in CI and in the Docker image).
//! Only checks that the fallback works end to end: OCR quality on this image is poor (e.g.
//! "Drow sugar"), which is why Gemini replaces it.

use std::error::Error;

use just_ingredients::extraction::mapping::to_measurement_matches;
use just_ingredients::extraction::tesseract::TesseractExtractor;
use just_ingredients::extraction::{ImageInput, IngredientExtractor};

#[tokio::test]
async fn tesseract_fallback_extracts_ingredients_from_the_test_image() -> Result<(), Box<dyn Error>>
{
    let image = ImageInput::prepare(std::fs::read("test_images/recipe_with_fraction.jpg")?).await?;
    let extraction = TesseractExtractor::default().extract(&image).await?;

    assert!(extraction.is_recipe, "text was found");
    assert!(!extraction.transcription.is_empty());
    let matches = to_measurement_matches(&extraction);
    assert!(matches.len() >= 3, "ingredients detected: {matches:?}");
    Ok(())
}
