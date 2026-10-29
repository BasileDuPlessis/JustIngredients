//! Validation module for common validation patterns
//!
//! This module consolidates validation logic that was previously scattered across
//! multiple modules, providing reusable validation functions for:
//!
//! - Recipe names
//! - Ingredient input
//! - Typed ingredient lines
//! - Quantity ranges
//! - Basic input constraints

use crate::errors::AppError;
use crate::text_processing::MeasurementMatch;
use lazy_static::lazy_static;
use regex::Regex;

/// A quantity: "1 1/2", "1/2", "2,5", "250". ASCII digits only: "２" is no quantity.
const QUANTITY: &str = r"[0-9]+ [0-9]+/[0-9]+|[0-9]+/[0-9]+|[0-9]+(?:[.,][0-9]+)?";

lazy_static! {
    /// A quantity starting a typed line, also before a unit without a space ("250g").
    static ref LEADING_QUANTITY: Regex =
        Regex::new(&format!("^(?:{QUANTITY})")).expect("Invalid quantity regex pattern");
    /// A range starting a typed line: "2-3", "100 – 150", "2 à 3", "2 ou 3", "2 to 3".
    static ref LEADING_RANGE: Regex = Regex::new(&format!(
        "^({QUANTITY})(?: ?[-–—] ?| (?:à|ou|to|or) )({QUANTITY})"
    ))
    .expect("Invalid range regex pattern");
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
    let answer = spell_out_fractions(input.trim()).to_lowercase();
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
/// - a list bullet or number first is ignored ("- 2 œufs", "1) 2 œufs");
/// - a quantity first ("2", "2,5", "1/2", "1 1/2", "1-1/2", "½"), or last after the name; a
///   range ("2-3", "2 à 3") keeps its lower bound and is flagged ⚠️, like the extraction does;
/// - then maybe a unit: the canonical ones and their spellings ([`crate::units`]), `cl` and
///   `dl` converted to ml, count units ("gousses", "pincée"; "bouquet" and "filet" only before
///   "de"/"d'"/"of", so "1 bouquet garni" stays a name);
/// - "de", "d'" or "of" after a unit is dropped from the name;
/// - units are stored like the extraction stores them ("cups" → "cup", "20 cl" → 200 ml).
///
/// Errors carry the locale key: `edit-empty`, `edit-too-long` (over 200 characters),
/// `edit-one-line` (several lines), `edit-no-ingredient-name` (a quantity without a name),
/// `edit-ingredient-name-too-long` (over [`MAX_INGREDIENT_NAME_CHARS`]) and
/// `edit-invalid-quantity` (zero, negative, over [`MAX_QUANTITY`] or unreadable: "1/0").
pub fn parse_ingredient_from_text(input: &str) -> Result<MeasurementMatch, AppError> {
    let trimmed = input.trim();
    validate_basic_input(trimmed)?;
    // A pasted list would become one name: one ingredient per message
    if trimmed.contains('\n') {
        return Err(AppError::Validation("edit-one-line".to_string()));
    }
    let line = spell_out_fractions(trimmed);
    let line = without_bullet(&line);
    let invalid_quantity = || AppError::Validation("edit-invalid-quantity".to_string());
    if line
        .strip_prefix(['-', '−'])
        .is_some_and(|rest| rest.trim_start().starts_with(|c: char| c.is_ascii_digit()))
    {
        return Err(invalid_quantity());
    }

    let (quantity, range, unit, name) = match split_leading_quantity(line) {
        Some(leading) => {
            let (unit, name) = split_leading_unit(leading.rest);
            (leading.quantity, leading.range, unit, name)
        }
        None => split_trailing_quantity(line)
            .unwrap_or_else(|| (String::new(), false, None, line.to_string())),
    };
    // "1/0": a quantity that cannot be read is refused, not stored as none
    if !quantity.is_empty() && parse_quantity(&quantity).is_none() {
        return Err(invalid_quantity());
    }
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
        // A range keeps its lower bound, to check, like the extraction does
        requires_quantity_confirmation: range,
    };
    validate_quantity_range(&ingredient)?;
    Ok(ingredient)
}

/// "1½ cup" → "1 1/2 cup", "1⁄2" → "1/2", inner spaces collapsed.
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
            // Fraction slash
            None if c == '⁄' => spelled.push('/'),
            None => spelled.push(c),
        }
    }
    spelled.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "- 2 œufs", "•200 g farine", "1) 2 œufs" → the line without its list bullet or number. A
/// hyphen or minus sign is a bullet only before a space ("-2 g" is a negative quantity).
fn without_bullet(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix(['–', '—', '•', '·', '▪', '*']) {
        return rest.trim_start();
    }
    match line.split_once(' ') {
        Some(("-" | "−", rest)) => rest,
        Some((number, rest))
            if number.len() > 1
                && number.ends_with(['.', ')'])
                && number[..number.len() - 1]
                    .bytes()
                    .all(|b| b.is_ascii_digit()) =>
        {
            rest
        }
        _ => line,
    }
}

/// The quantity a typed line starts with.
struct LeadingQuantity<'a> {
    /// As typed, with a dot for a decimal comma; the lower bound of a range.
    quantity: String,
    /// "2-3", "2 à 3": a range.
    range: bool,
    /// What follows the quantity.
    rest: &'a str,
}

/// "250 g farine" → "250" and "g farine", "2,5 kg" → "2.5", "2 à 3 tomates" → "2" (a range);
/// `None` without a number first.
fn split_leading_quantity(line: &str) -> Option<LeadingQuantity<'_>> {
    if let Some(found) = LEADING_RANGE.captures(line) {
        let (whole, low, high) = (found.get(0)?, found.get(1)?, found.get(2)?);
        let rest = line[whole.end()..].trim_start();
        // "1-1/2 cups": the American way to write 1 1/2, not a range
        let mixed_number = line[low.end()..high.start()].trim() == "-"
            && low.as_str().bytes().all(|b| b.is_ascii_digit())
            && !high.as_str().contains(' ')
            && parse_quantity(high.as_str()).is_some_and(|fraction| fraction < 1.0);
        return Some(LeadingQuantity {
            quantity: if mixed_number {
                format!("{} {}", low.as_str(), high.as_str())
            } else {
                low.as_str().replace(',', ".")
            },
            range: !mixed_number,
            rest,
        });
    }
    let found = LEADING_QUANTITY.find(line)?;
    Some(LeadingQuantity {
        quantity: found.as_str().replace(',', "."),
        range: false,
        rest: line[found.end()..].trim_start(),
    })
}

/// "de farine" → "farine": the article after a unit; `None` without one.
fn without_article(name: &str) -> Option<&str> {
    ["de ", "d'", "d’", "of "].iter().find_map(|article| {
        name.get(..article.len())
            .filter(|start| start.eq_ignore_ascii_case(article))
            .map(|_| name[article.len()..].trim_start())
    })
}

/// "g de farine" → (Some("g"), "farine"); "oeufs" → (None, "oeufs"); "bouquet garni" →
/// (None, "bouquet garni").
fn split_leading_unit(rest: &str) -> (Option<String>, String) {
    let words: Vec<&str> = rest.split(' ').filter(|word| !word.is_empty()).collect();
    if let Some(unit) = crate::units::leading_unit(&words) {
        let after = words[unit.words..].join(" ");
        match without_article(&after) {
            Some(name) => return (Some(words[..unit.words].join(" ")), name.to_string()),
            None if !unit.needs_article => return (Some(words[..unit.words].join(" ")), after),
            None => {}
        }
    }
    (None, rest.to_string())
}

/// "Farine : 200 g" → ("200", Some("g"), "Farine"): a quantity, maybe with a unit, ending the
/// line after a name. "Épices 4 saisons" ("saisons" is no unit) and "Farine type 00" (no
/// quantity of 0) are names.
fn split_trailing_quantity(line: &str) -> Option<(String, bool, Option<String>, String)> {
    let words: Vec<&str> = line.split(' ').collect();
    (1..words.len()).find_map(|start| {
        let tail = words[start..].join(" ");
        let trailing = split_leading_quantity(&tail)?;
        if !trailing.rest.is_empty() && !crate::units::is_unit(trailing.rest) {
            return None;
        }
        if !parse_quantity(&trailing.quantity).is_some_and(|q| q > 0.0 && q <= MAX_QUANTITY) {
            return None;
        }
        let unit = (!trailing.rest.is_empty()).then(|| trailing.rest.to_string());
        let name = words[..start].join(" ");
        let name = name.trim_end_matches([':', ',', ' ']);
        (!name.is_empty()).then(|| (trailing.quantity, trailing.range, unit, name.to_string()))
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

/// A quantity answer with its unit ("250 g", "20 cl", "2 gousses", "½ cup"): the quantity
/// formatted like the database column and the unit stored like a typed line's; `None` for
/// anything else (no unit, an unknown one, a range, out of range).
pub fn parse_quantity_with_unit(input: &str) -> Option<(String, String)> {
    let line = spell_out_fractions(input.trim());
    let leading = split_leading_quantity(&line)?;
    if leading.range || !crate::units::is_unit(leading.rest) {
        return None;
    }
    let (quantity, unit) =
        normalize_typed_measurement(leading.quantity, Some(leading.rest.to_string()));
    let quantity = parse_quantity_answer(&quantity).filter(|quantity| !quantity.is_empty())?;
    Some((quantity, unit?))
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
        ("2 cloves garlic", "2", Some("cloves"), "garlic"),
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
        ("− 2 œufs", "2", None, "œufs"),
        ("• 200 g farine", "200", Some("g"), "farine"),
        ("•200 g farine", "200", Some("g"), "farine"),
        ("— 2 œufs", "2", None, "œufs"),
        ("1) 2 œufs", "2", None, "œufs"),
        ("1. 200 g farine", "200", Some("g"), "farine"),
        ("1⁄2 citron", "1/2", None, "citron"),
        ("1-1/2 cups flour", "1 1/2", Some("cup"), "flour"),
        ("1 can tomatoes", "1", Some("can"), "tomatoes"),
        ("2 gousses ail", "2", Some("gousses"), "ail"),
        ("1 pincée sel", "1", Some("pincée"), "sel"),
        ("1 pkg cream cheese", "1", Some("pkg"), "cream cheese"),
        ("1 bâton de cannelle", "1", Some("bâton"), "cannelle"),
        ("2 filets de saumon", "2", Some("filets"), "saumon"),
        ("1 filet mignon", "1", None, "filet mignon"),
        ("Farine 00", "", None, "Farine 00"),
        ("farine type 00", "", None, "farine type 00"),
        ("Fromage blanc 0", "", None, "Fromage blanc 0"),
        ("500 g farine 00", "500", Some("g"), "farine 00"),
        ("Sel : 1 pincée", "1", Some("pincée"), "Sel"),
        ("２ œufs", "", None, "２ œufs"),
    ];

    /// (typed range, lower bound, unit, name): flagged ⚠️ to check.
    const TYPED_RANGES: &[(&str, &str, Option<&str>, &str)] = &[
        ("2-3 gousses d'ail", "2", Some("gousses"), "ail"),
        ("2 à 3 tomates", "2", None, "tomates"),
        ("2 à 3 c. à soupe de sucre", "2", Some("tbsp"), "sucre"),
        ("100 à 150 g de farine", "100", Some("g"), "farine"),
        ("100-150 g de sucre", "100", Some("g"), "sucre"),
        ("100 – 150 g de sucre", "100", Some("g"), "sucre"),
        ("2 ou 3 oeufs", "2", None, "oeufs"),
        ("2 to 3 cups flour", "2", Some("cup"), "flour"),
        ("1,5-2 kg pommes", "1.5", Some("kg"), "pommes"),
        ("Farine : 100-150 g", "100", Some("g"), "Farine"),
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
        for (line, quantity, unit, name) in TYPED_RANGES {
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
            assert!(parsed.requires_quantity_confirmation, "{line:?} is a range");
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
        for line in ["−2 g sucre", "- −2 g sucre", "−", "−sel", "• −"] {
            // U+2212 is 3 bytes: no slicing inside it
            let _ = parse_ingredient_from_text(line);
        }
        assert_eq!(refused("−2 g sucre"), "edit-invalid-quantity");
        assert_eq!(refused("1/0 oignon"), "edit-invalid-quantity");
        assert_eq!(refused("1 2/0 oignon"), "edit-invalid-quantity");
        assert_eq!(
            refused("0-2 g sel"),
            "edit-invalid-quantity",
            "lower bound 0"
        );
        assert_eq!(refused("2 œufs\n100 g sucre"), "edit-one-line");
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
