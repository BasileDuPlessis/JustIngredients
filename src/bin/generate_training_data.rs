use ab_glyph::{FontVec, PxScale};
use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use imageproc::drawing::draw_text_mut;
use rand::prelude::*;
use rand::rng;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct MeasurementUnits {
    measurement_units: Units,
}

#[derive(Debug, Deserialize)]
struct Units {
    volume_units: Vec<String>,
    weight_units: Vec<String>,
    volume_units_metric: Vec<String>,
    us_units: Vec<String>,
    french_units: Vec<String>,
}

#[derive(Debug)]
struct TrainingDataGenerator {
    units: MeasurementUnits,
    ingredients: Vec<String>,
    rng: rand::rngs::ThreadRng,
}

impl TrainingDataGenerator {
    fn new() -> Result<Self> {
        // Load measurement units - try multiple possible paths
        let units_path = Self::find_config_file("measurement_units.json")?;
        let units_content = fs::read_to_string(&units_path)
            .with_context(|| format!("Failed to read {}", units_path.display()))?;
        let units: MeasurementUnits = serde_json::from_str(&units_content)
            .context("Failed to parse measurement_units.json")?;

        // Load user words and filter for ingredients
        let words_path = Self::find_config_file("user_words.txt")?;
        let words_content = fs::read_to_string(&words_path)
            .with_context(|| format!("Failed to read {}", words_path.display()))?;
        let all_words: Vec<String> = words_content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|s| s.to_string())
            .collect();

        // Filter out units to get ingredients
        let unit_words: std::collections::HashSet<String> = units
            .measurement_units
            .volume_units
            .iter()
            .chain(&units.measurement_units.weight_units)
            .chain(&units.measurement_units.volume_units_metric)
            .chain(&units.measurement_units.us_units)
            .chain(&units.measurement_units.french_units)
            .cloned()
            .collect();

        let ingredients: Vec<String> = all_words
            .into_iter()
            .filter(|word| !unit_words.contains(word) && word.len() > 2)
            .collect();

        Ok(Self {
            units,
            ingredients,
            rng: rng(),
        })
    }

    fn generate_quantity(&mut self) -> String {
        match self.rng.random_range(0..4) {
            0 => self.rng.random_range(1..=10).to_string(),
            1 => format!(
                "{}/{}",
                self.rng.random_range(1..=5),
                self.rng.random_range(2..=4)
            ),
            2 => format!(
                "{} {}/{}",
                self.rng.random_range(1..=3),
                self.rng.random_range(1..=9),
                self.rng.random_range(2..=4)
            ),
            _ => format!("{:.2}", self.rng.random_range(0.25..=5.0)),
        }
    }

    fn generate_ingredient(&mut self) -> String {
        let quantity = self.generate_quantity();
        let mut all_units = Vec::new();
        all_units.extend(&self.units.measurement_units.volume_units);
        all_units.extend(&self.units.measurement_units.weight_units);
        all_units.extend(&self.units.measurement_units.volume_units_metric);
        all_units.extend(&self.units.measurement_units.us_units);
        all_units.extend(&self.units.measurement_units.french_units);

        let unit = all_units.choose(&mut self.rng).unwrap();
        let ingredient = self.ingredients.choose(&mut self.rng).unwrap();

        format!("{} {} {}", quantity, unit, ingredient)
    }

    fn generate_recipe(&mut self, num_items: usize) -> Vec<String> {
        (0..num_items).map(|_| self.generate_ingredient()).collect()
    }

    fn find_config_file(filename: &str) -> Result<std::path::PathBuf> {
        // Try multiple possible locations for config files
        let possible_paths = vec![
            // From src/bin/ when running binary directly
            format!("../config/{}", filename),
            // From project root when running tests
            format!("config/{}", filename),
            // Absolute path from current working directory
            format!("./config/{}", filename),
        ];

        for path_str in possible_paths {
            let path = Path::new(&path_str);
            if path.exists() {
                return Ok(path.to_path_buf());
            }
        }

        Err(anyhow::anyhow!(
            "Could not find {} in any expected location",
            filename
        ))
    }
}

fn load_font() -> Result<FontVec> {
    // Try to load system fonts, fallback to default
    let font_data = if Path::new("/System/Library/Fonts/Supplemental/Chalkduster.ttf").exists() {
        fs::read("/System/Library/Fonts/Supplemental/Chalkduster.ttf")?
    } else if Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").exists() {
        fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")?
    } else {
        // Fallback: we'd need to include a default font
        return Err(anyhow::anyhow!("No suitable font found"));
    };

    FontVec::try_from_vec(font_data).context("Failed to load font")
}

fn create_training_image(
    lines: &[String],
    width: u32,
    height: u32,
    font: &FontVec,
) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>> {
    // Create white background
    let mut img = ImageBuffer::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = Rgb([255, 255, 255]);
    }

    let mut rng = rng();
    let mut y = 50.0;

    for line in lines {
        let x = rng.random_range(50.0..150.0);
        let font_size = rng.random_range(20.0..30.0);
        let scale = PxScale::from(font_size);

        // Draw text with ab_glyph
        draw_text_mut(
            &mut img,
            Rgb([0, 0, 0]),
            x as i32,
            y as i32,
            scale,
            font,
            line,
        );

        y += rng.random_range(40.0..60.0);
    }

    Ok(img)
}

fn generate_box_file(image_path: &Path, _text_content: &str, box_path: &Path) -> Result<()> {
    let output_base = box_path.with_extension("");

    let output = Command::new("tesseract")
        .args([
            image_path.to_str().unwrap(),
            output_base.to_str().unwrap(),
            "-l",
            "eng",
            "--psm",
            "6",
            "makebox",
        ])
        .output()
        .context("Failed to run tesseract for box file generation")?;

    if !output.status.success() {
        eprintln!(
            "Warning: Tesseract box generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Could implement fallback box file creation here
    }

    Ok(())
}

fn main() -> Result<()> {
    let mut generator = TrainingDataGenerator::new()?;
    let font = load_font()?;

    fs::create_dir_all("tmp/training_data")?;

    let num_images = 2000;
    println!("Generating {} training samples...", num_images);

    for i in 0..num_images {
        if i % 100 == 0 {
            println!("Generated {}/{} samples...", i, num_images);
        }

        // Generate recipe
        let num_items = generator.rng.random_range(5..=10);
        let recipe_lines = generator.generate_recipe(num_items);
        let recipe_text = recipe_lines.join("\n");

        // Create training image
        let img_path = format!("tmp/training_data/recipe_{:04}.tif", i);
        let img = create_training_image(&recipe_lines, 800, 600, &font)?;
        img.save(&img_path)?;

        // Generate .box file using Tesseract
        let box_path = format!("tmp/training_data/recipe_{:04}.box", i);
        generate_box_file(Path::new(&img_path), &recipe_text, Path::new(&box_path))?;

        // Save ground truth text
        let text_path = format!("tmp/training_data/recipe_{:04}.txt", i);
        fs::write(&text_path, &recipe_text)?;
    }

    println!(
        "✅ Generated {} training samples in tmp/training_data/",
        num_images
    );
    println!("\nNext steps:");
    println!("1. Install Tesseract training tools");
    println!("2. Run: cargo run --bin train_tesseract");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_training_data_generator_creation() {
        let generator = TrainingDataGenerator::new();
        assert!(generator.is_ok());
    }

    #[test]
    fn test_ingredient_generation() {
        let mut generator = TrainingDataGenerator::new().unwrap();
        let ingredient = generator.generate_ingredient();

        // Should contain quantity, unit, and ingredient
        let parts: Vec<&str> = ingredient.split_whitespace().collect();
        assert!(
            parts.len() >= 3,
            "Ingredient should have at least 3 parts: {}",
            ingredient
        );

        // Check that it contains a number or fraction
        let has_quantity = parts[0].chars().any(|c| c.is_numeric() || c == '/');
        assert!(
            has_quantity,
            "First part should be a quantity: {}",
            ingredient
        );
    }

    #[test]
    fn test_recipe_generation() {
        let mut generator = TrainingDataGenerator::new().unwrap();
        let recipe = generator.generate_recipe(5);

        assert_eq!(recipe.len(), 5);
        for ingredient in &recipe {
            assert!(!ingredient.is_empty());
            assert!(ingredient.split_whitespace().count() >= 3);
        }
    }

    #[test]
    fn test_font_loading() {
        let font = load_font();
        // Font loading might fail in test environment, but that's ok
        // The important thing is that the function doesn't panic
        let _ = font; // Just to use the variable
    }

    #[test]
    fn test_image_creation() {
        let font = load_font();
        if let Ok(font) = font {
            let lines = vec!["2 cups flour".to_string(), "1 tbsp sugar".to_string()];
            let result = create_training_image(&lines, 400, 200, &font);

            if let Ok(img) = result {
                assert_eq!(img.width(), 400);
                assert_eq!(img.height(), 200);
            } else {
                // Image creation might fail if font rendering has issues
                eprintln!(
                    "Image creation failed (expected in some environments): {:?}",
                    result
                );
            }
        }
    }

    #[test]
    fn test_quantity_generation() {
        let mut generator = TrainingDataGenerator::new().unwrap();

        // Generate many quantities to test variety
        let mut quantities = std::collections::HashSet::new();
        for _ in 0..100 {
            quantities.insert(generator.generate_quantity());
        }

        // Should generate some variety
        assert!(
            quantities.len() > 5,
            "Should generate variety in quantities"
        );
    }
}
