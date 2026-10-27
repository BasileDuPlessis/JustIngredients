//! Live test of the Gemini extraction against the real API, on every image of `test_images/`
//! that has an expected answer in `scripts/llm_benchmark/expected/`.
//!
//! It calls the paid API (about 0.002 USD per image), so it is ignored by default and never
//! runs in CI. Run it with a key in the environment or in `.env`:
//!
//! ```text
//! cargo test --test gemini_live -- --ignored --nocapture
//! ```

use std::error::Error;
use std::path::Path;

use just_ingredients::extraction::gemini::GeminiExtractor;
use just_ingredients::extraction::mapping::to_measurement_matches;
use just_ingredients::extraction::{GeminiConfig, ImageInput, IngredientExtractor};
use just_ingredients::secret::SecretString;
use just_ingredients::validation::parse_quantity;
use serde::Deserialize;

#[derive(Deserialize)]
struct Expected {
    ingredients: Vec<ExpectedIngredient>,
}

#[derive(Deserialize)]
struct ExpectedIngredient {
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
}

#[tokio::test]
#[ignore = "calls the paid Gemini API: set GEMINI_API_KEY and run with --ignored"]
async fn extracts_the_test_images() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    let Some(api_key) = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
    else {
        eprintln!("GEMINI_API_KEY not set: skipping the live Gemini test");
        return Ok(());
    };
    let mut config = GeminiConfig {
        api_key: SecretString::new(api_key.trim()),
        ..GeminiConfig::default()
    };
    if let Ok(model) = std::env::var("GEMINI_MODEL") {
        config.model = model;
    }
    let extractor = GeminiExtractor::new(&config)?;

    let expected_dir = Path::new("scripts/llm_benchmark/expected");
    let mut checked = 0;
    for entry in std::fs::read_dir("test_images")? {
        let path = entry?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let expected_path = expected_dir.join(format!("{stem}.json"));
        if !expected_path.exists() {
            continue;
        }
        let expected: Expected = serde_json::from_str(&std::fs::read_to_string(&expected_path)?)?;

        let image = ImageInput::prepare(std::fs::read(&path)?).await?;
        let extraction = extractor.extract(&image).await?;
        let matches = to_measurement_matches(&extraction);
        println!("{stem}: {} ingredients", matches.len());

        for item in &expected.ingredients {
            let found = matches
                .iter()
                .find(|m| m.ingredient_name.eq_ignore_ascii_case(&item.name))
                .ok_or_else(|| format!("{stem}: '{}' not extracted: {matches:?}", item.name))?;
            let quantity = parse_quantity(&found.quantity);
            let same_quantity = match (quantity, item.quantity) {
                (Some(got), Some(want)) => (got - want).abs() < 1e-6,
                (None, None) => true,
                _ => false,
            };
            assert!(
                same_quantity,
                "{stem}: quantity of '{}': {found:?}",
                item.name
            );
            assert_eq!(
                found.measurement.as_deref(),
                item.unit.as_deref(),
                "{stem}: unit of '{}'",
                item.name
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no test image with an expected answer");
    Ok(())
}
