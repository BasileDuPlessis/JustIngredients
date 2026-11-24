//! Scaling a recipe's quantities (`specs/tasks/recipe-scaling.md`): a view, the recipe unchanged.
//!
//! [`parse`] reads what the user asks: a factor ("x2", "÷2", "2/3", "1,5", "150 %", "double"),
//! an ingredient's new quantity ("4 oeufs", "400 g de farine"), dishes' sizes by their area
//! ("20x30 → 30x40", "Ø24 → Ø28", "rond 24 → 20x30"), or servings ("de 4 à 6 personnes").
//! [`scaled`] rounds a scaled quantity as a cook would: grams to the gram under 100 g then to 5 g,
//! spoons and cups to a quarter or a third, eggs and other counts to whole units (at least one).
//! A factor is a [`Ratio`] of small integers, so that it travels in a button's data.

use std::sync::LazyLock;

use regex::Regex;

use crate::units::{CanonicalUnit, canonicalize_unit, normalize_measurement};

/// The smallest and largest factors accepted.
pub const MIN_FACTOR: f64 = 0.1;
pub const MAX_FACTOR: f64 = 10.0;

/// A factor `num / den`, in lowest terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    pub num: u32,
    pub den: u32,
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Ratio {
    /// `num / den` in lowest terms; `None` for a zero.
    pub fn new(num: u32, den: u32) -> Option<Self> {
        if num == 0 || den == 0 {
            return None;
        }
        let divisor = gcd(num, den);
        Some(Self {
            num: num / divisor,
            den: den / divisor,
        })
    }

    /// The simplest ratio close to `value`: a denominator up to 12 within 0.5 %, else thousandths.
    pub fn approximate(value: f64) -> Option<Self> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        for den in 1..=12_u32 {
            let num = (value * f64::from(den)).round();
            if num >= 1.0 && (num / f64::from(den) - value).abs() <= value * 0.005 {
                return Self::new(num as u32, den);
            }
        }
        Self::new((value * 1000.0).round() as u32, 1000)
    }

    pub fn value(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// The ratio as in a button's data: "4/3".
    pub fn encode(self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    /// A ratio from [`Self::encode`], within the accepted factors.
    pub fn decode(text: &str) -> Option<Self> {
        let (num, den) = text.split_once('/')?;
        let ratio = Self::new(num.parse().ok()?, den.parse().ok()?)?;
        (MIN_FACTOR..=MAX_FACTOR)
            .contains(&ratio.value())
            .then_some(ratio)
    }

    pub const fn is_one(self) -> bool {
        self.num == self.den
    }

    /// "× 2", "÷ 2", "× 4/3", "× 3/2", "× 1,36" (fractions in halves, thirds and quarters only).
    pub fn label(self, french: bool) -> String {
        if self.den == 1 {
            format!("× {}", self.num)
        } else if self.num == 1 {
            format!("÷ {}", self.den)
        } else if self.den <= 4 {
            format!("× {}/{}", self.num, self.den)
        } else {
            let text = format!("{:.2}", self.value());
            let text = text.trim_end_matches('0').trim_end_matches('.');
            format!(
                "× {}",
                if french {
                    text.replace('.', ",")
                } else {
                    text.to_string()
                }
            )
        }
    }
}

/// Why a recipe is scaled, for the card's line.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// A factor, as given.
    Factor,
    /// One ingredient's new quantity: its name, the quantity it has and the one asked.
    Ingredient {
        name: String,
        from: String,
        to: String,
    },
    /// Another dish: "20×30" → "30×40", "Ø24" → "Ø28".
    Dishes { from: String, to: String },
    /// Other servings.
    Servings { from: u32, to: u32 },
}

/// A scale: the factor and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    pub ratio: Ratio,
    pub reason: Reason,
}

impl Scale {
    /// A plain factor (the buttons, a linked recipe opened at a card's scale).
    pub const fn factor(ratio: Ratio) -> Self {
        Self {
            ratio,
            reason: Reason::Factor,
        }
    }

    /// The card's line: "× 4/3 (oeufs : 3 → 4)", "× 2 (plat 20×30 → 30×40)".
    pub fn describe(&self, french: bool) -> String {
        let factor = self.ratio.label(french);
        match &self.reason {
            Reason::Factor => factor,
            Reason::Ingredient { name, from, to } => format!("{factor} ({name} : {from} → {to})"),
            Reason::Dishes { from, to } => {
                let dish = if french { "plat" } else { "dish" };
                format!("{factor} ({dish} {from} → {to})")
            }
            Reason::Servings { from, to } => {
                let people = if french { "personnes" } else { "servings" };
                format!("{factor} ({from} → {to} {people})")
            }
        }
    }
}

/// An ingredient of the recipe, as stored, for "4 oeufs".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line<'a> {
    pub name: &'a str,
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// Why a scale was not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScaleError {
    #[error("not understood")]
    NotUnderstood,
    /// Below a tenth or above ten times.
    #[error("out of range")]
    OutOfRange,
    /// "4 oeufs" in a recipe without eggs.
    #[error("no such ingredient: {0}")]
    NoSuchIngredient(String),
    /// "4 sucre" in a recipe with sugar and icing sugar.
    #[error("ambiguous: {0:?}")]
    Ambiguous(Vec<String>),
    /// The ingredient has no quantity to scale from ("sel").
    #[error("no quantity: {0}")]
    NoQuantity(String),
    /// "2 tasses de farine" for flour in grams: another kind of unit.
    #[error("another unit: {0}")]
    OtherUnit(String),
}

const NUMBER: &str = r"(\d+(?:[.,]\d+)?)";

/// The words between two dishes or two servings: "→", "->", "à", "en", "vers", "to", "pour".
const BETWEEN: &str = r"\s*(?:→|->|=>|>|\bà\b|\ba\b|\ben\b|\bvers\b|\bto\b|\bpour\b)\s*";

/// Servings' words.
const PEOPLE: &str = r"(?:personnes?|pers\.?|parts?|portions?|servings?|people)";

// The patterns are constants, compiled by the tests of this module: they cannot fail at run time.
/// "20x30 → 30x40", "de 20 x 30 cm à 30x40", "ø24 → ø28", "rond 24 cm en 28".
#[allow(clippy::expect_used)]
static DISHES: LazyLock<Regex> = LazyLock::new(|| {
    let shape = r"((?:ø|diam(?:etre)?|rond)\s*\d+(?:[.,]\d+)?|\d+(?:[.,]\d+)?\s*x\s*\d+(?:[.,]\d+)?|\d+(?:[.,]\d+)?)\s*(?:cm)?";
    Regex::new(&format!(r"^(?:de\s+|from\s+)?{shape}{BETWEEN}{shape}$"))
        .expect("Invalid dishes regex pattern")
});

/// "de 4 à 6 personnes", "4 → 6 personnes", "pour 6 pour 8 personnes".
#[allow(clippy::expect_used)]
static SERVINGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?:de\s+|from\s+|pour\s+)?(\d+)\s*{PEOPLE}?{BETWEEN}(\d+)\s*{PEOPLE}$"
    ))
    .expect("Invalid servings regex pattern")
});

#[allow(clippy::expect_used)]
static MULTIPLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?:x\s*{NUMBER}|{NUMBER}\s*x|fois\s*{NUMBER}|{NUMBER}\s*fois)$"
    ))
    .expect("Invalid multiply regex pattern")
});

#[allow(clippy::expect_used)]
static DIVIDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?:/\s*{NUMBER}|divis(?:e|er|ee)?\s+par\s+{NUMBER}|divided\s+by\s+{NUMBER})$"
    ))
    .expect("Invalid divide regex pattern")
});

#[allow(clippy::expect_used)]
static FRACTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)\s*/\s*(\d+)$").expect("Invalid fraction regex pattern"));

#[allow(clippy::expect_used)]
static PERCENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^{NUMBER}\s*%$")).expect("Invalid percent regex pattern")
});

#[allow(clippy::expect_used)]
static PLAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^{NUMBER}$")).expect("Invalid number regex pattern"));

fn number(text: &str) -> Option<f64> {
    text.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

fn within(ratio: Ratio) -> Result<Ratio, ScaleError> {
    if (MIN_FACTOR..=MAX_FACTOR).contains(&ratio.value()) {
        Ok(ratio)
    } else {
        Err(ScaleError::OutOfRange)
    }
}

fn factor(value: f64) -> Result<Scale, ScaleError> {
    if !(MIN_FACTOR..=MAX_FACTOR).contains(&value) {
        return Err(ScaleError::OutOfRange);
    }
    let ratio = Ratio::approximate(value).ok_or(ScaleError::NotUnderstood)?;
    Ok(Scale::factor(within(ratio)?))
}

/// A dish's area, and how it is written on the card.
fn dish(text: &str, round_hint: bool) -> Option<(f64, String, bool)> {
    let text = text.trim();
    if let Some((width, length)) = text.split_once('x') {
        let (width, length) = (number(width.trim())?, number(length.trim())?);
        let show = |n: f64| format!("{n}").replace('.', ",");
        return Some((
            width * length,
            format!("{}×{}", show(width), show(length)),
            false,
        ));
    }
    let digits = text.trim_start_matches(|c: char| !c.is_ascii_digit());
    let diameter = number(digits)?;
    let round = round_hint || digits.len() != text.len();
    if !round {
        return None;
    }
    let shown = format!("Ø{}", format!("{diameter}").replace('.', ","));
    Some((
        std::f64::consts::PI * diameter * diameter / 4.0,
        shown,
        true,
    ))
}

fn dishes(captures: &regex::Captures<'_>) -> Result<Scale, ScaleError> {
    let (first, second) = (
        captures.get(1).map_or("", |m| m.as_str()),
        captures.get(2).map_or("", |m| m.as_str()),
    );
    let (from_area, from, round) = dish(first, false).ok_or(ScaleError::NotUnderstood)?;
    let (to_area, to, _) = dish(second, round).ok_or(ScaleError::NotUnderstood)?;
    let mut scale = factor(to_area / from_area)?;
    // Whole sizes of two dishes of the same shape give an exact ratio (two rectangles; two
    // circles: the squares of the diameters, π cancels out)
    if let (Some((a, a_round)), Some((b, b_round))) =
        (exact_area(first, false), exact_area(second, round))
        && a_round == b_round
        && let Some(ratio) = Ratio::new(b, a)
    {
        scale.ratio = within(ratio)?;
    }
    scale.reason = Reason::Dishes { from, to };
    Ok(scale)
}

/// A dish's area in whole units when its sizes are whole (a circle: its diameter squared), and
/// whether it is round.
fn exact_area(text: &str, round_hint: bool) -> Option<(u32, bool)> {
    let text = text.trim();
    if let Some((width, length)) = text.split_once('x') {
        return Some((
            width.trim().parse::<u32>().ok()? * length.trim().parse::<u32>().ok()?,
            false,
        ));
    }
    let digits = text.trim_start_matches(|c: char| !c.is_ascii_digit());
    let diameter: u32 = digits.parse().ok()?;
    (round_hint || digits.len() != text.len()).then_some((diameter * diameter, true))
}

/// A folded name without plurals, for "oeufs" against "œuf".
fn key(name: &str) -> Vec<String> {
    crate::search::fold(name)
        .split_whitespace()
        .filter(|word| {
            !matches!(
                *word,
                "de" | "d" | "du" | "des" | "la" | "le" | "les" | "of" | "the"
            )
        })
        .map(crate::search::singular)
        .collect()
}

/// The quantity of a measure in its canonical unit, or as a count with its unit's key.
enum Measure {
    Canonical(CanonicalUnit, f64),
    Count(String, f64),
}

fn measure(quantity: f64, unit: Option<&str>) -> Measure {
    let normalized = normalize_measurement(Some(quantity), unit);
    let quantity = normalized.quantity.unwrap_or(quantity);
    match normalized.unit.as_deref().and_then(canonicalize_unit) {
        Some(canonical) => Measure::Canonical(canonical, quantity),
        None => Measure::Count(
            normalized
                .unit
                .as_deref()
                .map(|unit| key(unit).join(" "))
                .unwrap_or_default(),
            quantity,
        ),
    }
}

fn ingredient(text: &str, lines: &[Line<'_>]) -> Result<Scale, ScaleError> {
    let typed = crate::validation::parse_ingredient_from_text(text)
        .map_err(|_| ScaleError::NotUnderstood)?;
    let wanted = crate::validation::parse_quantity(&typed.quantity)
        .filter(|quantity| *quantity > 0.0)
        .ok_or(ScaleError::NotUnderstood)?;
    let name = key(&typed.ingredient_name);
    if name.is_empty() {
        return Err(ScaleError::NotUnderstood);
    }
    let exact: Vec<&Line<'_>> = lines.iter().filter(|line| key(line.name) == name).collect();
    let found: Vec<&Line<'_>> = if exact.is_empty() {
        lines
            .iter()
            .filter(|line| {
                let words = key(line.name);
                name.iter().all(|word| words.contains(word))
            })
            .collect()
    } else {
        exact
    };
    let line = match found.as_slice() {
        [] => return Err(ScaleError::NoSuchIngredient(typed.ingredient_name)),
        [line] => *line,
        several => {
            return Err(ScaleError::Ambiguous(
                several.iter().map(|line| line.name.to_string()).collect(),
            ));
        }
    };
    let saved = line
        .quantity
        .filter(|quantity| *quantity > 0.0)
        .ok_or_else(|| ScaleError::NoQuantity(line.name.to_string()))?;
    let value = match (
        measure(saved, line.unit),
        measure(wanted, typed.measurement.as_deref()),
    ) {
        (Measure::Canonical(from_unit, from), Measure::Canonical(to_unit, to)) => {
            let (from, to) = (
                crate::conversion::unit_amount(from_unit).times(from),
                crate::conversion::unit_amount(to_unit).times(to),
            );
            match (from, to) {
                (crate::conversion::Amount::Mass(a), crate::conversion::Amount::Mass(b))
                | (crate::conversion::Amount::Volume(a), crate::conversion::Amount::Volume(b)) => {
                    b / a
                }
                _ => return Err(ScaleError::OtherUnit(line.name.to_string())),
            }
        }
        (Measure::Count(from_unit, from), Measure::Count(to_unit, to))
            if from_unit == to_unit || to_unit.is_empty() =>
        {
            to / from
        }
        _ => return Err(ScaleError::OtherUnit(line.name.to_string())),
    };
    let mut scale = factor(value)?;
    let show = |quantity: f64, unit: Option<&str>| {
        let number = format!("{}", (quantity * 1000.0).round() / 1000.0).replace('.', ",");
        match unit.map(str::trim).filter(|unit| !unit.is_empty()) {
            Some(unit) => format!("{number} {unit}"),
            None => number,
        }
    };
    scale.reason = Reason::Ingredient {
        name: line.name.to_string(),
        from: show(saved, line.unit),
        to: show(wanted, typed.measurement.as_deref().or(line.unit)),
    };
    Ok(scale)
}

/// Reads a scale typed by the user, for a recipe with `lines`.
pub fn parse(text: &str, lines: &[Line<'_>]) -> Result<Scale, ScaleError> {
    let cleaned: String = text
        .trim()
        .to_lowercase()
        .replace(['×', '*'], "x")
        .replace('÷', "/")
        .replace(['é', 'è'], "e")
        .replace('Ø', "ø")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim_end_matches(['.', '!']).trim().to_string();
    match cleaned.as_str() {
        "double" | "doubler" | "le double" | "twice" => return factor(2.0),
        "triple" | "tripler" | "le triple" => return factor(3.0),
        "moitie" | "la moitie" | "demi" | "half" | "diviser par deux" => return factor(0.5),
        _ => {}
    }
    if let Some(captures) = SERVINGS.captures(&cleaned) {
        let (from, to) = (
            captures.get(1).and_then(|m| m.as_str().parse::<u32>().ok()),
            captures.get(2).and_then(|m| m.as_str().parse::<u32>().ok()),
        );
        let (Some(from), Some(to)) = (from, to) else {
            return Err(ScaleError::NotUnderstood);
        };
        let ratio = within(Ratio::new(to, from).ok_or(ScaleError::OutOfRange)?)?;
        return Ok(Scale {
            ratio,
            reason: Reason::Servings { from, to },
        });
    }
    if let Some(captures) = DISHES.captures(&cleaned) {
        return dishes(&captures);
    }
    if let Some(captures) = MULTIPLY.captures(&cleaned) {
        let value = (1..=4)
            .find_map(|group| captures.get(group))
            .and_then(|m| number(m.as_str()))
            .ok_or(ScaleError::NotUnderstood)?;
        return factor(value);
    }
    if let Some(captures) = DIVIDE.captures(&cleaned) {
        let value = (1..=3)
            .find_map(|group| captures.get(group))
            .and_then(|m| number(m.as_str()))
            .filter(|n| *n > 0.0)
            .ok_or(ScaleError::NotUnderstood)?;
        return factor(1.0 / value);
    }
    if let Some(captures) = FRACTION.captures(&cleaned) {
        let (num, den) = (
            captures.get(1).and_then(|m| m.as_str().parse::<u32>().ok()),
            captures.get(2).and_then(|m| m.as_str().parse::<u32>().ok()),
        );
        let ratio = num
            .zip(den)
            .and_then(|(num, den)| Ratio::new(num, den))
            .ok_or(ScaleError::OutOfRange)?;
        return Ok(Scale::factor(within(ratio)?));
    }
    if let Some(captures) = PERCENT.captures(&cleaned) {
        let value = captures
            .get(1)
            .and_then(|m| number(m.as_str()))
            .ok_or(ScaleError::NotUnderstood)?;
        return factor(value / 100.0);
    }
    if let Some(captures) = PLAIN.captures(&cleaned) {
        let value = captures
            .get(1)
            .and_then(|m| number(m.as_str()))
            .ok_or(ScaleError::NotUnderstood)?;
        return factor(value);
    }
    ingredient(text.trim(), lines)
}

/// Rounds `value` to the nearest multiple of `step`, at least `step`.
fn to_step(value: f64, step: f64) -> f64 {
    ((value / step).round() * step).max(step)
}

/// A spoon or cup measure to the nearest quarter or third, at least `smallest`.
fn to_quarter_or_third(value: f64, smallest: f64) -> f64 {
    let quarter = (value * 4.0).round() / 4.0;
    let third = (value * 3.0).round() / 3.0;
    let best = if (quarter - value).abs() <= (third - value).abs() {
        quarter
    } else {
        third
    };
    best.max(smallest)
}

/// `quantity` (in `unit`, as stored) times `factor`, rounded as a cook would, in the same unit:
/// grams to the gram under 100 g then to 5 g; kilograms and litres to 0.05; millilitres to 1, 5,
/// 10 or 50 as they grow; spoons and cups to a quarter or a third; eggs and other counts to whole
/// units, at least one.
pub fn scaled(quantity: f64, unit: Option<&str>, factor: f64) -> f64 {
    let value = quantity * factor;
    if !value.is_finite() || value <= 0.0 {
        return quantity;
    }
    let normalized = normalize_measurement(Some(quantity), unit);
    // cl and dl are stored as written but read as millilitres: round them in millilitres
    let in_canonical = normalized.quantity.unwrap_or(quantity) * factor;
    let per_stored = if quantity > 0.0 {
        normalized.quantity.unwrap_or(quantity) / quantity
    } else {
        1.0
    };
    let rounded_canonical = match normalized.unit.as_deref().and_then(canonicalize_unit) {
        Some(CanonicalUnit::Gram) if in_canonical < 100.0 => to_step(in_canonical, 1.0),
        Some(CanonicalUnit::Gram) => to_step(in_canonical, 5.0),
        Some(CanonicalUnit::Kilogram | CanonicalUnit::Liter) => to_step(in_canonical, 0.05),
        Some(CanonicalUnit::Milligram) => to_step(in_canonical, 1.0),
        Some(CanonicalUnit::Milliliter) => {
            let step = match in_canonical {
                v if v < 10.0 => 1.0,
                v if v < 100.0 => 5.0,
                v if v < 1000.0 => 10.0,
                _ => 50.0,
            };
            to_step(in_canonical, step)
        }
        Some(CanonicalUnit::Teaspoon) => to_quarter_or_third(in_canonical, 0.125),
        Some(
            CanonicalUnit::Tablespoon
            | CanonicalUnit::Cup
            | CanonicalUnit::Ounce
            | CanonicalUnit::Pound
            | CanonicalUnit::Pint
            | CanonicalUnit::Quart
            | CanonicalUnit::Gallon,
        ) => to_quarter_or_third(in_canonical, 0.25),
        // Eggs, cloves, slices, pinches: whole units, at least one (the owner's choice)
        None => in_canonical.round().max(1.0),
    };
    // Back to the stored unit (a value stored in cl stays in cl)
    let back = rounded_canonical / per_stored;
    (back * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tart() -> Vec<Line<'static>> {
        vec![
            Line {
                name: "farine",
                quantity: Some(250.0),
                unit: Some("g"),
            },
            Line {
                name: "œufs",
                quantity: Some(3.0),
                unit: None,
            },
            Line {
                name: "sucre",
                quantity: Some(100.0),
                unit: Some("g"),
            },
            Line {
                name: "sucre glace",
                quantity: Some(20.0),
                unit: Some("g"),
            },
            Line {
                name: "lait",
                quantity: Some(25.0),
                unit: Some("cl"),
            },
            Line {
                name: "gousses d'ail",
                quantity: Some(2.0),
                unit: Some("gousses"),
            },
            Line {
                name: "sel",
                quantity: None,
                unit: None,
            },
        ]
    }

    fn ratio(text: &str) -> Option<(u32, u32)> {
        parse(text, &tart())
            .ok()
            .map(|scale| (scale.ratio.num, scale.ratio.den))
    }

    #[test]
    fn the_patterns_compile() {
        for pattern in [
            &DISHES, &SERVINGS, &MULTIPLY, &DIVIDE, &FRACTION, &PERCENT, &PLAIN,
        ] {
            assert!(!LazyLock::force(pattern).as_str().is_empty());
        }
    }

    #[test]
    fn factors_in_every_usual_form() {
        for (typed, expected) in [
            ("x2", (2, 1)),
            ("×2", (2, 1)),
            ("X 2", (2, 1)),
            ("*2", (2, 1)),
            ("2x", (2, 1)),
            ("2 fois", (2, 1)),
            ("double", (2, 1)),
            ("Doubler", (2, 1)),
            ("triple", (3, 1)),
            ("/2", (1, 2)),
            ("÷2", (1, 2)),
            ("diviser par 3", (1, 3)),
            ("moitié", (1, 2)),
            ("la moitié", (1, 2)),
            ("2/3", (2, 3)),
            ("1,5", (3, 2)),
            ("1.5", (3, 2)),
            ("x1,5", (3, 2)),
            ("150 %", (3, 2)),
            ("75%", (3, 4)),
            ("3", (3, 1)),
        ] {
            assert_eq!(ratio(typed), Some(expected), "{typed}");
        }
    }

    #[test]
    fn an_ingredients_new_quantity() {
        let scale = parse("4 oeufs", &tart());
        assert_eq!(
            scale.as_ref().map(|scale| scale.ratio),
            Ok(Ratio { num: 4, den: 3 })
        );
        assert_eq!(
            scale.map(|scale| scale.describe(true)),
            Ok("× 4/3 (œufs : 3 → 4)".to_string())
        );
        assert_eq!(ratio("400 g de farine"), Some((8, 5)));
        assert_eq!(ratio("0,5 kg de farine"), Some((2, 1)), "another mass unit");
        assert_eq!(ratio("50 cl de lait"), Some((2, 1)));
        assert_eq!(ratio("3 gousses d'ail"), Some((3, 2)));
        assert_eq!(
            ratio("150 g sucre glace"),
            Some((15, 2)),
            "the exact name first"
        );
        assert_eq!(
            parse("4 poireaux", &tart()),
            Err(ScaleError::NoSuchIngredient("poireaux".to_string()))
        );
        assert_eq!(
            parse("2 sel", &tart()),
            Err(ScaleError::NoQuantity("sel".to_string()))
        );
        assert_eq!(
            parse("2 cups farine", &tart()),
            Err(ScaleError::OtherUnit("farine".to_string())),
            "a volume for a mass"
        );
    }

    #[test]
    fn dishes_by_their_area() {
        let scale = parse("20x30 → 30x40", &[]);
        assert_eq!(
            scale.as_ref().map(|scale| scale.ratio),
            Ok(Ratio { num: 2, den: 1 })
        );
        assert_eq!(
            scale.map(|scale| scale.describe(true)),
            Ok("× 2 (plat 20×30 → 30×40)".to_string())
        );
        assert_eq!(ratio("de 20 x 30 cm à 30x40 cm"), Some((2, 1)));
        assert_eq!(ratio("20*30 -> 30*40"), Some((2, 1)));
        assert_eq!(ratio("Ø24 → Ø28"), Some((49, 36)));
        assert_eq!(
            ratio("rond 24 → 28"),
            Some((49, 36)),
            "the second shape follows"
        );
        assert_eq!(ratio("ø24 cm en ø28 cm"), Some((49, 36)));
        // A round dish of 24 (452 cm²) to a 20×30 one (600 cm²)
        let mixed = parse("Ø24 → 20x30", &[]).map(|scale| scale.ratio.value());
        assert!(
            mixed
                .as_ref()
                .is_ok_and(|factor| (factor - 600.0 / 452.389).abs() < 0.01),
            "{mixed:?}"
        );
    }

    #[test]
    fn servings_both_given() {
        assert_eq!(ratio("de 4 à 6 personnes"), Some((3, 2)));
        assert_eq!(ratio("4 → 6 personnes"), Some((3, 2)));
        assert_eq!(ratio("pour 6 pour 8 personnes"), Some((4, 3)));
        assert_eq!(ratio("4 to 2 servings"), Some((1, 2)));
        assert_eq!(
            parse("de 4 à 6 personnes", &[]).map(|scale| scale.describe(true)),
            Ok("× 3/2 (4 → 6 personnes)".to_string())
        );
    }

    #[test]
    fn what_is_refused() {
        for typed in ["", "beaucoup", "x", "/0", "abc 3"] {
            assert!(parse(typed, &tart()).is_err(), "{typed}");
        }
        for typed in ["x20", "/20", "1/50", "2000 %", "0"] {
            assert_eq!(
                parse(typed, &tart()).err(),
                Some(ScaleError::OutOfRange),
                "{typed}"
            );
        }
    }

    #[test]
    fn a_ratio_travels_in_a_button() {
        let ratio = Ratio::new(8, 6);
        assert_eq!(ratio, Some(Ratio { num: 4, den: 3 }));
        assert_eq!(ratio.map(Ratio::encode).as_deref(), Some("4/3"));
        assert_eq!(Ratio::decode("4/3"), ratio);
        assert_eq!(Ratio::decode("100/1"), None, "out of range");
        assert_eq!(Ratio::decode("0/3"), None);
        assert_eq!(Ratio::decode("abc"), None);
        assert!(
            Ratio::approximate(1.36).is_some_and(|ratio| (ratio.value() - 1.36).abs() < 0.007),
            "close enough"
        );
        assert_eq!(
            Ratio::approximate(0.5).map(Ratio::encode).as_deref(),
            Some("1/2")
        );
        assert_eq!(
            Ratio::new(15, 11).map(|ratio| ratio.label(true)).as_deref(),
            Some("× 1,36"),
            "a fraction a cook would not read is shown as a decimal"
        );
        assert_eq!(
            Ratio::new(1, 2).map(|ratio| ratio.label(true)).as_deref(),
            Some("÷ 2")
        );
    }

    #[test]
    fn a_cooks_rounding() {
        let close = |value: f64, expected: f64| (value - expected).abs() < 1e-3;
        // Grams
        assert!(close(scaled(250.0, Some("g"), 4.0 / 3.0), 335.0));
        assert!(close(scaled(30.0, Some("g"), 4.0 / 3.0), 40.0));
        assert!(
            close(scaled(2.0, Some("g"), 1.0 / 3.0), 1.0),
            "at least a gram"
        );
        assert!(close(scaled(1.2, Some("kg"), 4.0 / 3.0), 1.6));
        // Volumes, in the unit they are stored in
        assert!(close(scaled(25.0, Some("cl"), 4.0 / 3.0), 33.0), "330 ml");
        assert!(close(scaled(200.0, Some("ml"), 4.0 / 3.0), 270.0));
        assert!(close(scaled(1.0, Some("l"), 4.0 / 3.0), 1.35));
        // Spoons and cups: a quarter or a third
        assert!(
            close(scaled(1.0, Some("tsp"), 1.4), 4.0 / 3.0),
            "1 ⅓ is closer than 1 ½"
        );
        assert!(close(scaled(1.0, Some("tsp"), 1.45), 1.5));
        assert!(close(scaled(1.0, Some("tbsp"), 4.0 / 3.0), 4.0 / 3.0));
        assert!(close(scaled(1.0, Some("tsp"), 0.1), 0.125), "never zero");
        assert!(close(scaled(0.5, Some("cup"), 1.5), 0.75));
        // Eggs and counts: whole, at least one
        assert!(close(scaled(3.0, None, 4.0 / 3.0), 4.0));
        assert!(close(scaled(3.0, None, 1.5), 5.0), "4.5 rounds to 5");
        assert!(close(scaled(1.0, None, 0.5), 1.0), "at least one egg");
        assert!(close(scaled(2.0, Some("gousses"), 1.5), 3.0));
        assert!(close(scaled(1.0, Some("pincée"), 3.0), 3.0));
    }
}
