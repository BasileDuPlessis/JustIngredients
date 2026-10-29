//! Validation module for common validation patterns
//!
//! This module consolidates validation logic that was previously scattered across
//! multiple modules, providing reusable validation functions for:
//!
//! - Recipe names
//! - Ingredient input
//! - Measurement matches
//! - Quantity ranges
//! - Basic input constraints

use crate::errors::AppError;
use crate::text_processing::MeasurementMatch;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// A quantity starting a typed line: "1 1/2", "1/2", "2,5", "250" (also before a unit
    /// without a space: "250g").
    static ref LEADING_QUANTITY: Regex = Regex::new(r"^(\d+ \d+/\d+|\d+/\d+|\d+(?:[.,]\d+)?)")
        .expect("Invalid quantity regex pattern");
}

/// Longest ingredient name accepted, in characters: typed lines and name edits.
pub const MAX_INGREDIENT_NAME_CHARS: usize = 100;

/// Fraction characters, typed or pasted ("½ citron", "1½ cup").
const FRACTION_CHARACTERS: [(char, &str); 9] = [
    ('½', "1/2"),
    ('¼', "1/4"),
    ('¾', "3/4"),
    ('⅓', "1/3"),
    ('⅔', "2/3"),
    ('⅛', "1/8"),
    ('⅜', "3/8"),
    ('⅝', "5/8"),
    ('⅞', "7/8"),
];

/// Validates a recipe name input
///
/// # Arguments
/// * `name` - The recipe name to validate
///
/// # Returns
/// * `Ok(&str)` - The trimmed recipe name if valid
/// * `Err(AppError::Validation)` - Error message
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_recipe_name;
///
/// assert!(validate_recipe_name("My Recipe").is_ok());
/// assert_eq!(validate_recipe_name("").unwrap_err(), just_ingredients::errors::AppError::Validation("empty".to_string()));
/// assert_eq!(validate_recipe_name(&"a".repeat(256)).unwrap_err(), just_ingredients::errors::AppError::Validation("too_long".to_string()));
/// ```
pub fn validate_recipe_name(name: &str) -> Result<&str, AppError> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(AppError::Validation("empty".to_string()));
    }

    if trimmed.len() > 255 {
        return Err(AppError::Validation("too_long".to_string()));
    }

    Ok(trimmed)
}

/// Validate basic input constraints
///
/// # Arguments
/// * `input` - The input string to validate
///
/// # Returns
/// * `Ok(())` - Input is valid
/// * `Err(AppError::Validation)` - Error message
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_basic_input;
///
/// assert!(validate_basic_input("valid input").is_ok());
/// assert_eq!(validate_basic_input("").unwrap_err(), just_ingredients::errors::AppError::Validation("edit-empty".to_string()));
/// assert_eq!(validate_basic_input(&"a".repeat(201)).unwrap_err(), just_ingredients::errors::AppError::Validation("edit-too-long".to_string()));
/// ```
pub fn validate_basic_input(input: &str) -> Result<(), AppError> {
    if input.is_empty() {
        return Err(AppError::Validation("edit-empty".to_string()));
    }

    if input.chars().count() > 200 {
        return Err(AppError::Validation("edit-too-long".to_string()));
    }

    Ok(())
}

/// Validate that quantity is within reasonable range
///
/// # Arguments
/// * `measurement_match` - The measurement match to validate
///
/// # Returns
/// * `Ok(())` - Quantity is within valid range
/// * `Err(AppError::Validation)` - Error message
///
/// # Examples
/// ```
/// use just_ingredients::validation::validate_quantity_range;
/// use just_ingredients::text_processing::MeasurementMatch;
///
/// let valid_match = MeasurementMatch {
///     quantity: "2.5".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 10,
///     requires_quantity_confirmation: false,
/// };
///
/// assert!(validate_quantity_range(&valid_match).is_ok());
///
/// let invalid_match = MeasurementMatch {
///     quantity: "0".to_string(),
///     measurement: Some("cups".to_string()),
///     ingredient_name: "flour".to_string(),
///     line_number: 0,
///     start_pos: 0,
///     end_pos: 10,
///     requires_quantity_confirmation: false,
/// };
///
/// assert_eq!(validate_quantity_range(&invalid_match).unwrap_err(), just_ingredients::errors::AppError::Validation("edit-invalid-quantity".to_string()));
/// ```
pub fn validate_quantity_range(measurement_match: &MeasurementMatch) -> Result<(), AppError> {
    if let Some(qty) = parse_quantity(&measurement_match.quantity) {
        if qty <= 0.0 || qty > MAX_QUANTITY {
            return Err(AppError::Validation("edit-invalid-quantity".to_string()));
        }
    }
    Ok(())
}

/// Largest quantity accepted anywhere: validation, manual edits and extraction mapping.
pub const MAX_QUANTITY: f64 = 10_000.0;

/// Answers meaning "this ingredient has no quantity" to the quantity question.
const NO_QUANTITY_ANSWERS: [&str; 6] = ["-", "none", "no", "aucune", "aucun", "sans"];

/// Parses the answer to "type the quantity": `Some("")` for no quantity (`-`, `none`,
/// `aucune`, …), `Some(quantity)` for a number in (0, [`MAX_QUANTITY`]] formatted like the
/// database column, `None` for anything else.
pub fn parse_quantity_answer(input: &str) -> Option<String> {
    let answer = input.trim().to_lowercase();
    if NO_QUANTITY_ANSWERS.contains(&answer.as_str()) {
        return Some(String::new());
    }
    parse_quantity(&answer)
        .filter(|quantity| quantity.is_finite() && *quantity > 0.0 && *quantity <= MAX_QUANTITY)
        .map(crate::extraction::mapping::format_quantity)
}

/// Parse quantity string to f64 (handles fractions and decimals)
///
/// # Arguments
/// * `quantity_str` - The quantity string to parse
///
/// # Returns
/// * `Some(f64)` - The parsed quantity value
/// * `None` - Failed to parse the quantity
///
/// # Examples
/// ```
/// use just_ingredients::validation::parse_quantity;
///
/// assert_eq!(parse_quantity("2"), Some(2.0));
/// assert_eq!(parse_quantity("1/2"), Some(0.5));
/// assert_eq!(parse_quantity("2.5"), Some(2.5));
/// assert_eq!(parse_quantity("1 1/2"), Some(1.5));
/// assert_eq!(parse_quantity("invalid"), None);
/// ```
pub fn parse_quantity(quantity_str: &str) -> Option<f64> {
    let quantity = quantity_str.trim();
    // Mixed number "1 1/2": a whole part, then a fraction
    if let Some((whole, fraction)) = quantity.split_once(char::is_whitespace) {
        let whole = parse_simple_quantity(whole)?;
        let fraction = fraction.trim();
        return (fraction.contains('/') && whole.fract() == 0.0)
            .then(|| parse_simple_quantity(fraction))
            .flatten()
            .map(|fraction| whole + fraction);
    }
    parse_simple_quantity(quantity)
}

/// A number ("2", "2.5", "2,5") or a fraction ("1/2"), without spaces.
fn parse_simple_quantity(quantity: &str) -> Option<f64> {
    match quantity.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator: f64 = numerator.parse().ok()?;
            let denominator: f64 = denominator.parse().ok()?;
            (denominator != 0.0).then_some(numerator / denominator)
        }
        // Comma as decimal separator (European format)
        None => quantity.replace(',', ".").parse::<f64>().ok(),
    }
}

/// Reads an ingredient line typed by the user (an added ingredient, a whole-line edit):
/// "250 g de farine", "1 ½ cup sugar", "2 gousses d'ail", "Farine : 200 g", "sel".
///
/// - a quantity first ("2", "2,5", "1/2", "1 1/2", "½"), or last after the name;
/// - then maybe a unit: the canonical ones and their spellings ([`crate::units`]), `cl` and
///   `dl` converted to ml, count units ("gousses", "pincée") only before "de"/"d'"/"of";
/// - "de", "d'" or "of" after a unit is dropped from the name;
/// - units are stored like the extraction stores them ("cups" → "cup", "20 cl" → 200 ml).
///
/// Errors carry the locale key: `edit-empty`, `edit-too-long` (over 200 characters),
/// `edit-no-ingredient-name` (a quantity without a name), `edit-ingredient-name-too-long`
/// (over [`MAX_INGREDIENT_NAME_CHARS`]) and `edit-invalid-quantity` (zero, negative or over
/// [`MAX_QUANTITY`]).
pub fn parse_ingredient_from_text(input: &str) -> Result<MeasurementMatch, AppError> {
    let trimmed = input.trim();
    validate_basic_input(trimmed)?;
    let line = without_bullet(&spell_out_fractions(trimmed));
    if line.starts_with(['-', '−'])
        && line[1..]
            .trim_start()
            .starts_with(|c: char| c.is_ascii_digit())
    {
        return Err(AppError::Validation("edit-invalid-quantity".to_string()));
    }

    let (quantity, unit, name) = match split_leading_quantity(&line) {
        Some((quantity, rest)) => {
            let (unit, name) = split_leading_unit(rest);
            (quantity, unit, name)
        }
        None => split_trailing_quantity(&line).unwrap_or((String::new(), None, line.clone())),
    };
    let (quantity, unit) = normalize_typed_measurement(quantity, unit);

    if name.is_empty() {
        return Err(AppError::Validation("edit-no-ingredient-name".to_string()));
    }
    if name.chars().count() > MAX_INGREDIENT_NAME_CHARS {
        return Err(AppError::Validation(
            "edit-ingredient-name-too-long".to_string(),
        ));
    }
    let ingredient = MeasurementMatch {
        quantity,
        measurement: unit,
        ingredient_name: name,
        line_number: 0,
        start_pos: 0,
        end_pos: trimmed.len(),
        requires_quantity_confirmation: false,
    };
    validate_quantity_range(&ingredient)?;
    Ok(ingredient)
}

/// "1½ cup" → "1 1/2 cup", inner spaces collapsed.
fn spell_out_fractions(text: &str) -> String {
    let mut spelled = String::with_capacity(text.len());
    for c in text.chars() {
        match FRACTION_CHARACTERS
            .iter()
            .find(|(fraction, _)| *fraction == c)
        {
            Some((_, fraction)) => {
                spelled.push(' ');
                spelled.push_str(fraction);
                spelled.push(' ');
            }
            None => spelled.push(c),
        }
    }
    spelled.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "- 2 œufs" → "2 œufs": a list bullet before the line goes.
fn without_bullet(line: &str) -> String {
    match line.split_once(' ') {
        Some((bullet, rest)) if ["-", "–", "•", "*", "·"].contains(&bullet) => {
            rest.to_string()
        }
        _ => line.to_string(),
    }
}

/// "250 g farine" → ("250", "g farine"), "2,5 kg" → ("2.5", "kg"); `None` without a number
/// first.
fn split_leading_quantity(line: &str) -> Option<(String, &str)> {
    let found = LEADING_QUANTITY.find(line)?;
    Some((
        found.as_str().replace(',', "."),
        line[found.end()..].trim_start(),
    ))
}

/// "de farine" → "farine": the article after a unit; `None` without one.
fn without_article(name: &str) -> Option<&str> {
    ["de ", "d'", "d’", "of "].iter().find_map(|article| {
        name.get(..article.len())
            .filter(|start| start.eq_ignore_ascii_case(article))
            .map(|_| name[article.len()..].trim_start())
    })
}

/// "g de farine" → (Some("g"), "farine"); "oeufs" → (None, "oeufs").
fn split_leading_unit(rest: &str) -> (Option<String>, String) {
    let words: Vec<&str> = rest.split(' ').filter(|word| !word.is_empty()).collect();
    if let Some(unit) = crate::units::leading_unit(&words) {
        let after = words[unit.words..].join(" ");
        match without_article(&after) {
            Some(name) => return (Some(words[..unit.words].join(" ")), name.to_string()),
            None if !unit.counts => return (Some(words[..unit.words].join(" ")), after),
            None => {}
        }
    }
    (None, rest.to_string())
}

/// "Farine : 200 g" → ("200", Some("g"), "Farine"): a quantity, maybe with a unit, ending the
/// line after a name. "Épices 4 saisons" has none ("saisons" is no unit).
fn split_trailing_quantity(line: &str) -> Option<(String, Option<String>, String)> {
    let words: Vec<&str> = line.split(' ').collect();
    (1..words.len()).find_map(|start| {
        let tail = words[start..].join(" ");
        let (quantity, unit) = split_leading_quantity(&tail)?;
        if !unit.is_empty() && !crate::units::is_unit(unit) {
            return None;
        }
        let unit = (!unit.is_empty()).then(|| unit.to_string());
        let name = words[..start].join(" ");
        let name = name.trim_end_matches([':', ',', ' ']);
        (!name.is_empty()).then(|| (quantity, unit, name.to_string()))
    })
}

/// The unit as the extraction stores it ("cups" → "cup", "gousses" kept); `cl` and `dl` become
/// ml with the quantity converted. Other typed quantities keep their form ("1/2").
fn normalize_typed_measurement(quantity: String, unit: Option<String>) -> (String, Option<String>) {
    let Some(unit) = unit else {
        return (quantity, None);
    };
    let typed = parse_quantity(&quantity);
    let normalized = crate::units::normalize_measurement(typed, Some(&unit));
    let quantity = match normalized.quantity {
        Some(converted) if normalized.quantity != typed => {
            crate::extraction::mapping::format_quantity(converted)
        }
        _ => quantity,
    };
    (quantity, normalized.unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_answers_accept_a_range_or_no_quantity() {
        for (answer, expected) in [
            ("-", Some("")),
            ("Aucune", Some("")),
            ("none", Some("")),
            ("2,5", Some("2.5")),
            ("1/2", Some("0.5")),
            ("10000", Some("10000")),
            ("0", None),
            ("-2", None),
            ("1e9", None),
            ("beaucoup", None),
        ] {
            assert_eq!(
                parse_quantity_answer(answer).as_deref(),
                expected,
                "{answer}"
            );
        }
    }

    #[test]
    fn typed_ingredients_keep_missing_and_large_quantities() -> Result<(), AppError> {
        let salt = parse_ingredient_from_text("sel")?;
        assert_eq!(salt.quantity, "", "no quantity typed, none stored");
        let flour = parse_ingredient_from_text("1500 g farine")?;
        assert_eq!(flour.quantity, "1500");
        assert!(
            !flour.requires_quantity_confirmation,
            "1500 g is a normal quantity"
        );
        Ok(())
    }

    #[test]
    fn test_validate_recipe_name() {
        // Valid names
        assert!(validate_recipe_name("My Recipe").is_ok());
        assert!(validate_recipe_name("  Recipe  ").is_ok());
        match validate_recipe_name("  Recipe  ") {
            Ok(name) => assert_eq!(name, "Recipe"),
            Err(_) => unreachable!("Expected valid recipe name"),
        }

        // Empty names
        assert_eq!(
            validate_recipe_name(""),
            Err(AppError::Validation("empty".to_string()))
        );
        assert_eq!(
            validate_recipe_name("   "),
            Err(AppError::Validation("empty".to_string()))
        );

        // Too long names
        let long_name = "a".repeat(256);
        assert_eq!(
            validate_recipe_name(&long_name),
            Err(AppError::Validation("too_long".to_string()))
        );
    }

    #[test]
    fn test_validate_basic_input() {
        // Valid input
        assert!(validate_basic_input("valid input").is_ok());
        assert!(validate_basic_input("a").is_ok());

        // Empty input
        assert_eq!(
            validate_basic_input(""),
            Err(AppError::Validation("edit-empty".to_string()))
        );

        // Too long input
        let long_input = "a".repeat(201);
        assert_eq!(
            validate_basic_input(&long_input),
            Err(AppError::Validation("edit-too-long".to_string()))
        );
    }

    #[test]
    fn test_parse_quantity() {
        // Whole numbers
        assert_eq!(parse_quantity("2"), Some(2.0));
        assert_eq!(parse_quantity("0"), Some(0.0));
        assert_eq!(parse_quantity("-5"), Some(-5.0));

        // Decimals
        assert_eq!(parse_quantity("2.5"), Some(2.5));
        assert_eq!(parse_quantity("1.0"), Some(1.0));

        // Fractions
        assert_eq!(parse_quantity("1/2"), Some(0.5));
        assert_eq!(parse_quantity("3/4"), Some(0.75));
        assert_eq!(parse_quantity("2/1"), Some(2.0));

        // European format
        assert_eq!(parse_quantity("2,5"), Some(2.5));

        // Mixed numbers
        assert_eq!(parse_quantity("1 1/2"), Some(1.5));
        assert_eq!(parse_quantity(" 2  3/4 "), Some(2.75));
        assert_eq!(
            parse_quantity("1 2"),
            None,
            "two numbers, not a mixed number"
        );
        assert_eq!(
            parse_quantity("1.5 1/2"),
            None,
            "the whole part must be whole"
        );
        assert_eq!(parse_quantity("1 1/0"), None);

        // Invalid cases
        assert_eq!(parse_quantity(""), None);
        assert_eq!(parse_quantity("abc"), None);
        assert_eq!(parse_quantity("1/0"), None);
        assert_eq!(parse_quantity("1/"), None);
        assert_eq!(parse_quantity("/2"), None);
    }

    #[test]
    fn test_validate_quantity_range() {
        let create_match = |quantity: &str| MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: Some("cups".to_string()),
            ingredient_name: "flour".to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 10,
            requires_quantity_confirmation: false,
        };

        // Valid ranges
        assert!(validate_quantity_range(&create_match("1")).is_ok());
        assert!(validate_quantity_range(&create_match("10000")).is_ok());
        assert!(validate_quantity_range(&create_match("0.1")).is_ok());
        assert!(validate_quantity_range(&create_match("1/2")).is_ok());

        // Invalid ranges
        assert_eq!(
            validate_quantity_range(&create_match("0")),
            Err(AppError::Validation("edit-invalid-quantity".to_string()))
        );
        assert_eq!(
            validate_quantity_range(&create_match("-1")),
            Err(AppError::Validation("edit-invalid-quantity".to_string()))
        );
        assert_eq!(
            validate_quantity_range(&create_match("10001")),
            Err(AppError::Validation("edit-invalid-quantity".to_string()))
        );
    }

    /// (typed line, quantity, unit, name)
    const TYPED_LINES: &[(&str, &str, Option<&str>, &str)] = &[
        ("2 œufs", "2", None, "œufs"),
        ("12 oeufs", "12", None, "oeufs"),
        ("3 lait", "3", None, "lait"),
        ("2 grosses carottes", "2", None, "grosses carottes"),
        ("2 pommes de terre", "2", None, "pommes de terre"),
        ("sel", "", None, "sel"),
        ("sel et poivre", "", None, "sel et poivre"),
        ("un œuf", "", None, "un œuf"),
        ("1/2 oignon", "1/2", None, "oignon"),
        ("½ citron", "1/2", None, "citron"),
        ("250g farine", "250", Some("g"), "farine"),
        ("  2 g  sel  ", "2", Some("g"), "sel"),
        ("100g de beurre doux", "100", Some("g"), "beurre doux"),
        ("500 grammes de farine", "500", Some("g"), "farine"),
        ("150 gr de sucre", "150", Some("g"), "sucre"),
        ("1 kilo de farine", "1", Some("kg"), "farine"),
        (
            "1 kg de pommes de terre",
            "1",
            Some("kg"),
            "pommes de terre",
        ),
        ("2,5 kg pommes", "2.5", Some("kg"), "pommes"),
        ("2.5 kg pommes", "2.5", Some("kg"), "pommes"),
        ("2 lbs beef", "2", Some("lb"), "beef"),
        ("8 oz cheese", "8", Some("oz"), "cheese"),
        ("2 cups flour", "2", Some("cup"), "flour"),
        ("1 1/2 cups flour", "1 1/2", Some("cup"), "flour"),
        ("1 ½ cup sugar", "1 1/2", Some("cup"), "sugar"),
        ("1½ cup sugar", "1 1/2", Some("cup"), "sugar"),
        ("¼ cup sugar", "1/4", Some("cup"), "sugar"),
        ("2 tbsp butter", "2", Some("tbsp"), "butter"),
        ("1 T sugar", "1", Some("tbsp"), "sugar"),
        ("1 t salt", "1", Some("tsp"), "salt"),
        ("3 c. à soupe d'huile", "3", Some("tbsp"), "huile"),
        (
            "3 cuillères à soupe d'huile d'olive",
            "3",
            Some("tbsp"),
            "huile d'olive",
        ),
        ("1 cuillère à café de sel", "1", Some("tsp"), "sel"),
        ("2 c. à c. de sel", "2", Some("tsp"), "sel"),
        ("2 cs huile", "2", Some("tbsp"), "huile"),
        ("1 cc sel", "1", Some("tsp"), "sel"),
        ("200 ml de lait", "200", Some("ml"), "lait"),
        ("1 l d'eau", "1", Some("l"), "eau"),
        ("1 litre de lait", "1", Some("l"), "lait"),
        ("2 l. lait", "2", Some("l"), "lait"),
        ("1,5 l eau", "1.5", Some("l"), "eau"),
        ("3/4 l lait", "3/4", Some("l"), "lait"),
        ("20 cl de crème", "200", Some("ml"), "crème"),
        ("3 dl eau", "300", Some("ml"), "eau"),
        ("1/2 tasse de lait", "1/2", Some("tasse"), "lait"),
        ("1 pincée de sel", "1", Some("pincée"), "sel"),
        ("2 gousses d'ail", "2", Some("gousses"), "ail"),
        ("4 Tranches de jambon", "4", Some("tranches"), "jambon"),
        (
            "2 sachets de sucre vanillé",
            "2",
            Some("sachets"),
            "sucre vanillé",
        ),
        ("1 verre de lait", "1", Some("verre"), "lait"),
        ("2 slices of bread", "2", Some("slices"), "bread"),
        ("1 bouquet garni", "1", None, "bouquet garni"),
        ("2 cloves garlic", "2", None, "cloves garlic"),
        ("Farine 200 g", "200", Some("g"), "Farine"),
        ("farine : 200 g", "200", Some("g"), "farine"),
        ("Farine T55 500 g", "500", Some("g"), "Farine T55"),
        (
            "Sucre vanillé 2 sachets",
            "2",
            Some("sachets"),
            "Sucre vanillé",
        ),
        ("Oeufs 3", "3", None, "Oeufs"),
        ("Épices 4 saisons", "", None, "Épices 4 saisons"),
        ("- 2 œufs", "2", None, "œufs"),
        ("• 200 g farine", "200", Some("g"), "farine"),
    ];

    #[test]
    fn typed_lines_give_quantity_unit_and_name() {
        for (line, quantity, unit, name) in TYPED_LINES {
            let parsed = parse_ingredient_from_text(line)
                .unwrap_or_else(|e| panic!("{line:?} was refused: {e:?}"));
            assert_eq!(
                (
                    parsed.quantity.as_str(),
                    parsed.measurement.as_deref(),
                    parsed.ingredient_name.as_str()
                ),
                (*quantity, *unit, *name),
                "{line:?}"
            );
            assert!(!parsed.requires_quantity_confirmation, "{line:?}");
        }
    }

    #[test]
    fn typed_lines_refused_with_the_reason() {
        let refused = |line: &str| match parse_ingredient_from_text(line) {
            Err(AppError::Validation(key)) => key,
            other => panic!("{line:?} gave {other:?}"),
        };
        assert_eq!(refused(""), "edit-empty");
        assert_eq!(refused("   "), "edit-empty");
        assert_eq!(refused(&"a".repeat(201)), "edit-too-long");
        assert_eq!(refused("2"), "edit-no-ingredient-name");
        assert_eq!(refused("200 g"), "edit-no-ingredient-name");
        assert_eq!(refused("2 cups"), "edit-no-ingredient-name");
        assert_eq!(refused("-2 g sucre"), "edit-invalid-quantity");
        assert_eq!(refused("-1 cups flour"), "edit-invalid-quantity");
        assert_eq!(refused("0 g sucre"), "edit-invalid-quantity");
        assert_eq!(refused("20000 g sucre"), "edit-invalid-quantity");
        assert_eq!(
            refused("2000 cl lait"),
            "edit-invalid-quantity",
            "20 l once in ml"
        );
        assert_eq!(
            refused(&format!("2 g {}", "a".repeat(101))),
            "edit-ingredient-name-too-long"
        );
        assert_eq!(
            refused(&"é".repeat(MAX_INGREDIENT_NAME_CHARS + 1)),
            "edit-ingredient-name-too-long"
        );
        assert!(parse_ingredient_from_text(&"é".repeat(MAX_INGREDIENT_NAME_CHARS)).is_ok());
    }
}
