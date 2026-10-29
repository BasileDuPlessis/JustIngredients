//! The unit views of recipe cards (UX plan U8, `specs/tasks/measure-conversion.md`): each user
//! sees measures as in the recipe, all in weight (g), in French units (g, cl) or in US units
//! (cups, oz). Saved recipes never change: only what a card shows does.
//!
//! Rounding is the kitchen's: grams to the gram under 100 g and to 5 g above, cl to the half
//! under 10 cl, cups to the nearest quarter or third. Spoons and cups take fractions ("½ c. à
//! c.", "1 ½ cups"), metric units a decimal comma in French ("1,5 kg").

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::conversion::{find_ingredient, unit_amount, Amount, Ingredient, CUP_ML};
use crate::units::{canonicalize_unit, normalize_measurement, CanonicalUnit};

/// How a user's recipe cards show measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitView {
    /// As read or typed, unit names in the user's language.
    Recipe,
    /// Everything that can be weighed in grams, liquids included: for a kitchen scale.
    Weight,
    /// Weights in g, liquids in cl and l, spoons kept (the default).
    #[default]
    French,
    /// Cups, tablespoons, teaspoons, ounces and pounds, in fractions.
    Us,
}

impl UnitView {
    pub const ALL: [Self; 4] = [Self::Recipe, Self::Weight, Self::French, Self::Us];

    /// The value stored in `users.unit_view` and in callback data.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recipe => "recipe",
            Self::Weight => "weight",
            Self::French => "french",
            Self::Us => "us",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.as_str() == value)
    }
}

/// The measure of a saved ingredient as a card shows it: quantity and unit ("180 g", "½ c. à
/// c.", "2"), empty without either, and whether the view converted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownMeasure {
    pub text: String,
    pub converted: bool,
}

/// Under this weight a measure stays as saved: a kitchen scale weighs to the gram.
const SMALLEST_WEIGHED_GRAMS: f64 = 1.0;

/// The measure of a saved ingredient in `view`, for a French-speaking user or not. Units saved
/// before normalization ("G", "cl", "cuillères à soupe") are read like normalized ones.
pub fn show_measure(
    quantity: Option<f64>,
    unit: Option<&str>,
    name: &str,
    view: UnitView,
    french: bool,
) -> ShownMeasure {
    let normalized = normalize_measurement(quantity, unit);
    let canonical = normalized.unit.as_deref().and_then(canonicalize_unit);
    let (Some(quantity), Some(canonical)) = (
        normalized.quantity.filter(|q| q.is_finite() && *q > 0.0),
        canonical,
    ) else {
        // No quantity, or a count unit ("2 gousses", "1 pincée", "3"): as saved
        return unchanged(&[
            normalized
                .quantity
                .map(|q| decimal(q, french))
                .unwrap_or_default(),
            normalized.unit.unwrap_or_default(),
        ]);
    };
    let labels = if view == UnitView::Us || !french {
        Labels::English
    } else {
        Labels::French
    };
    let as_saved = || ShownMeasure {
        text: measure_text(quantity, canonical, labels),
        converted: false,
    };
    let amount = unit_amount(canonical).times(quantity);
    let ingredient = find_ingredient(name);
    let shown = match view {
        UnitView::Recipe => None,
        UnitView::Weight => weight_view(amount, ingredient),
        UnitView::French => french_view(amount, canonical, ingredient, french),
        UnitView::Us => us_view(amount, canonical, ingredient),
    };
    match shown {
        Some(text) => {
            let converted = text != as_saved().text;
            ShownMeasure { text, converted }
        }
        None => as_saved(),
    }
}

fn unchanged(parts: &[String]) -> ShownMeasure {
    ShownMeasure {
        text: parts
            .iter()
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        converted: false,
    }
}

/// Grams of what can be weighed: masses, and volumes of an ingredient of the table, from 1 g.
fn weight_view(amount: Amount, ingredient: Option<&Ingredient>) -> Option<String> {
    let grams = match amount {
        Amount::Mass(grams) => grams,
        Amount::Volume(millilitres) => ingredient?.grams(millilitres),
    };
    (grams >= SMALLEST_WEIGHED_GRAMS).then(|| format!("{} g", round_grams(grams)))
}

/// Masses in g (kg from 1 kg), liquids in cl and l, solids of the table in g; spoons, milligrams
/// and cups of an unknown ingredient as saved.
fn french_view(
    amount: Amount,
    unit: CanonicalUnit,
    ingredient: Option<&Ingredient>,
    french: bool,
) -> Option<String> {
    match amount {
        Amount::Mass(_) if unit == CanonicalUnit::Milligram => None,
        Amount::Mass(grams) => Some(metric_grams(grams, french)),
        Amount::Volume(_)
            if matches!(unit, CanonicalUnit::Tablespoon | CanonicalUnit::Teaspoon) =>
        {
            None
        }
        Amount::Volume(millilitres) => match ingredient {
            Some(ingredient) if !ingredient.liquid => {
                Some(metric_grams(ingredient.grams(millilitres), french))
            }
            Some(_) => Some(metric_volume(millilitres, french)),
            None if matches!(unit, CanonicalUnit::Milliliter | CanonicalUnit::Liter) => {
                Some(metric_volume(millilitres, french))
            }
            None => None,
        },
    }
}

/// Ingredients of the table in cups, tablespoons or teaspoons, other masses in oz and lb, metric
/// volumes in cups; US units as saved.
fn us_view(amount: Amount, unit: CanonicalUnit, ingredient: Option<&Ingredient>) -> Option<String> {
    match amount {
        Amount::Mass(grams) => match ingredient {
            Some(ingredient) => us_volume(ingredient.millilitres(grams)),
            None => us_mass(grams),
        },
        Amount::Volume(millilitres)
            if matches!(unit, CanonicalUnit::Milliliter | CanonicalUnit::Liter) =>
        {
            us_volume(millilitres)
        }
        Amount::Volume(_) => None,
    }
}

/// To the gram under 100 g, to 5 g above.
fn round_grams(grams: f64) -> i64 {
    if grams < 100.0 {
        (grams.round() as i64).max(1)
    } else {
        ((grams / 5.0).round() * 5.0) as i64
    }
}

/// "180 g", "1,25 kg" from 1 kg.
fn metric_grams(grams: f64, french: bool) -> String {
    let rounded = round_grams(grams);
    if rounded >= 1000 {
        format!("{} kg", decimal(rounded as f64 / 1000.0, french))
    } else {
        format!("{rounded} g")
    }
}

/// "8 ml" under 1 cl, "30 cl", "4,5 cl" (to the half under 10 cl), "1,3 l" from 1 l.
fn metric_volume(millilitres: f64, french: bool) -> String {
    if millilitres < 10.0 {
        format!("{} ml", millilitres.round().max(1.0))
    } else if millilitres < 1000.0 {
        let centilitres = millilitres / 10.0;
        let rounded = if centilitres < 10.0 {
            (centilitres * 2.0).round() / 2.0
        } else {
            centilitres.round()
        };
        format!("{} cl", decimal(rounded, french))
    } else {
        format!(
            "{} l",
            decimal((millilitres / 100.0).round() / 10.0, french)
        )
    }
}

/// Quarts from 4 cups, cups from ¼ cup, then tablespoons and teaspoons.
fn us_volume(millilitres: f64) -> Option<String> {
    let cups = millilitres / CUP_ML;
    let (value, unit) = if cups >= 4.0 {
        (to_quarter(millilitres / 946.353), CanonicalUnit::Quart)
    } else if cups >= 0.25 - 0.02 {
        (to_cup_fraction(cups), CanonicalUnit::Cup)
    } else if millilitres >= 15.0 - 1.0 {
        (
            (millilitres / 15.0 * 2.0).round() / 2.0,
            CanonicalUnit::Tablespoon,
        )
    } else {
        (
            to_quarter(millilitres / 5.0).max(0.125),
            CanonicalUnit::Teaspoon,
        )
    };
    Some(measure_text(value, unit, Labels::English))
}

/// Ounces to the half under 1 lb, pounds to the quarter above; under half an ounce as saved.
fn us_mass(grams: f64) -> Option<String> {
    let ounces = grams / 28.3495;
    if ounces < 0.5 {
        None
    } else if ounces < 16.0 {
        Some(measure_text(
            (ounces * 2.0).round() / 2.0,
            CanonicalUnit::Ounce,
            Labels::English,
        ))
    } else {
        Some(measure_text(
            to_quarter(ounces / 16.0),
            CanonicalUnit::Pound,
            Labels::English,
        ))
    }
}

fn to_quarter(value: f64) -> f64 {
    (value * 4.0).round() / 4.0
}

/// The nearest whole number plus 0, ¼, ⅓, ½, ⅔, ¾ (2,08 cups → 2).
fn to_cup_fraction(cups: f64) -> f64 {
    let whole = cups.floor();
    [0.0, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75, 1.0]
        .into_iter()
        .map(|fraction| whole + fraction)
        .min_by(|a, b| (a - cups).abs().total_cmp(&(b - cups).abs()))
        .unwrap_or(cups)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Labels {
    French,
    English,
}

/// "1 ½ cups", "½ c. à c.", "1,5 kg", "200 ml": spoons, cups and US masses in fractions,
/// metric units in decimals.
fn measure_text(quantity: f64, unit: CanonicalUnit, labels: Labels) -> String {
    let french = labels == Labels::French;
    let number = match unit {
        CanonicalUnit::Gram
        | CanonicalUnit::Kilogram
        | CanonicalUnit::Milligram
        | CanonicalUnit::Milliliter
        | CanonicalUnit::Liter => decimal(quantity, french),
        _ => fraction(quantity).unwrap_or_else(|| decimal(quantity, french)),
    };
    format!("{number} {}", label(unit, quantity, labels))
}

fn label(unit: CanonicalUnit, quantity: f64, labels: Labels) -> &'static str {
    // French nouns take a plural from 2, English ones above 1
    let plural = match labels {
        Labels::French => quantity >= 2.0,
        Labels::English => quantity > 1.0,
    };
    let pick = |singular, plurals| if plural { plurals } else { singular };
    match (labels, unit) {
        (Labels::French, CanonicalUnit::Tablespoon) => "c. à s.",
        (Labels::French, CanonicalUnit::Teaspoon) => "c. à c.",
        (Labels::French, CanonicalUnit::Cup) => pick("tasse", "tasses"),
        (Labels::French, CanonicalUnit::Pound) => pick("livre", "livres"),
        (Labels::French, CanonicalUnit::Ounce) => pick("once", "onces"),
        (Labels::French, CanonicalUnit::Pint) => pick("pinte", "pintes"),
        (_, CanonicalUnit::Cup) => pick("cup", "cups"),
        (_, CanonicalUnit::Pint) => pick("pint", "pints"),
        (_, CanonicalUnit::Quart) => pick("quart", "quarts"),
        (_, CanonicalUnit::Gallon) => pick("gallon", "gallons"),
        (_, unit) => unit.as_str(),
    }
}

/// "1.5" or "1,5", at most 3 decimals.
fn decimal(value: f64, french: bool) -> String {
    let text = crate::extraction::mapping::format_quantity(value);
    if french {
        text.replace('.', ",")
    } else {
        text
    }
}

/// "1 ½", "¾", "2" for a whole number plus an eighth, a quarter or a third; `None` otherwise.
fn fraction(value: f64) -> Option<String> {
    const GLYPHS: [(f64, &str); 9] = [
        (0.0, ""),
        (0.125, "⅛"),
        (0.25, "¼"),
        (1.0 / 3.0, "⅓"),
        (0.5, "½"),
        (2.0 / 3.0, "⅔"),
        (0.75, "¾"),
        (0.875, "⅞"),
        (1.0, ""),
    ];
    let whole = value.floor();
    let (part, glyph) = GLYPHS
        .iter()
        .find(|(part, _)| ((value - whole) - part).abs() < 0.01)?;
    let whole = whole + if *part == 1.0 { 1.0 } else { 0.0 };
    Some(match (whole as i64, *glyph) {
        (0, "") => "0".to_string(),
        (0, glyph) => glyph.to_string(),
        (whole, "") => whole.to_string(),
        (whole, glyph) => format!("{whole} {glyph}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (quantity, unit, name) of a saved line.
    type Line = (Option<f64>, Option<&'static str>, &'static str);

    fn shown(lines: &[Line], view: UnitView, french: bool) -> Vec<String> {
        lines
            .iter()
            .map(|(quantity, unit, name)| {
                let measure = show_measure(*quantity, *unit, name, view, french);
                format!("{} {name}", measure.text).trim().to_string()
            })
            .collect()
    }

    const PANCAKES: [Line; 7] = [
        (Some(1.5), Some("cup"), "farine"),
        (Some(1.25), Some("cup"), "lait"),
        (Some(3.0), Some("tbsp"), "beurre"),
        (Some(1.0), Some("tbsp"), "sucre"),
        (Some(2.0), Some("tsp"), "levure chimique"),
        (Some(0.5), Some("tsp"), "sel"),
        (Some(1.0), None, "œuf"),
    ];

    const CREPES: [Line; 6] = [
        (Some(250.0), Some("g"), "farine"),
        (Some(500.0), Some("ml"), "lait"),
        (Some(50.0), Some("g"), "sucre"),
        (Some(30.0), Some("g"), "beurre"),
        (Some(3.0), None, "œufs"),
        (Some(1.0), Some("pincée"), "sel"),
    ];

    #[test]
    fn an_american_recipe_in_the_four_views() {
        assert_eq!(
            shown(&PANCAKES, UnitView::Recipe, true),
            [
                "1 ½ tasse farine",
                "1 ¼ tasse lait",
                "3 c. à s. beurre",
                "1 c. à s. sucre",
                "2 c. à c. levure chimique",
                "½ c. à c. sel",
                "1 œuf"
            ]
        );
        assert_eq!(
            shown(&PANCAKES, UnitView::Weight, true),
            [
                "180 g farine",
                "305 g lait",
                "43 g beurre",
                "13 g sucre",
                "8 g levure chimique",
                "3 g sel",
                "1 œuf"
            ]
        );
        assert_eq!(
            shown(&PANCAKES, UnitView::French, true),
            [
                "180 g farine",
                "30 cl lait",
                "3 c. à s. beurre",
                "1 c. à s. sucre",
                "2 c. à c. levure chimique",
                "½ c. à c. sel",
                "1 œuf"
            ]
        );
        assert_eq!(
            shown(&PANCAKES, UnitView::Us, true),
            [
                "1 ½ cups farine",
                "1 ¼ cups lait",
                "3 tbsp beurre",
                "1 tbsp sucre",
                "2 tsp levure chimique",
                "½ tsp sel",
                "1 œuf"
            ]
        );
    }

    #[test]
    fn a_french_recipe_in_the_four_views() {
        assert_eq!(
            shown(&CREPES, UnitView::Recipe, true),
            [
                "250 g farine",
                "500 ml lait",
                "50 g sucre",
                "30 g beurre",
                "3 œufs",
                "1 pincée sel"
            ]
        );
        assert_eq!(
            shown(&CREPES, UnitView::Weight, true),
            [
                "250 g farine",
                "515 g lait",
                "50 g sucre",
                "30 g beurre",
                "3 œufs",
                "1 pincée sel"
            ]
        );
        assert_eq!(
            shown(&CREPES, UnitView::French, true),
            [
                "250 g farine",
                "50 cl lait",
                "50 g sucre",
                "30 g beurre",
                "3 œufs",
                "1 pincée sel"
            ]
        );
        assert_eq!(
            shown(&CREPES, UnitView::Us, true),
            [
                "2 cups farine",
                "2 cups lait",
                "¼ cup sucre",
                "2 tbsp beurre",
                "3 œufs",
                "1 pincée sel"
            ]
        );
    }

    #[test]
    fn converted_says_whether_the_view_changed_the_measure() {
        let converted = |quantity, unit, name, view| {
            show_measure(Some(quantity), Some(unit), name, view, true).converted
        };
        assert!(!converted(250.0, "g", "farine", UnitView::French));
        assert!(converted(500.0, "ml", "lait", UnitView::French), "50 cl");
        assert!(!converted(3.0, "tbsp", "beurre", UnitView::French));
        assert!(converted(1.0, "cup", "farine", UnitView::Weight));
        assert!(!converted(1.0, "cup", "bonbons", UnitView::Weight));
        assert!(!converted(1.0, "cup", "farine", UnitView::Recipe));
    }

    #[test]
    fn units_saved_before_normalization_are_read() {
        let text = |quantity, unit: &'static str, view| {
            show_measure(Some(quantity), Some(unit), "lait", view, true).text
        };
        assert_eq!(text(8.0, "cl", UnitView::French), "8 cl");
        assert_eq!(text(5.0, "G", UnitView::Recipe), "5 g");
        assert_eq!(
            text(2.0, "cuillères à soupe", UnitView::Recipe),
            "2 c. à s."
        );
        assert_eq!(text(3.0, "dl", UnitView::Weight), "310 g", "309 g, to 5 g");
        assert_eq!(text(1.0, "litres", UnitView::French), "1 l");
    }

    #[test]
    fn kitchen_rounding_and_number_formats() {
        let text = |quantity, unit: &'static str, name, view, french| {
            show_measure(Some(quantity), Some(unit), name, view, french).text
        };
        assert_eq!(text(1.0, "cup", "beurre", UnitView::Weight, true), "225 g");
        assert_eq!(text(1.2, "kg", "farine", UnitView::French, true), "1,2 kg");
        assert_eq!(text(1.2, "kg", "farine", UnitView::French, false), "1.2 kg");
        assert_eq!(text(1.2, "kg", "farine", UnitView::Weight, true), "1200 g");
        assert_eq!(text(45.0, "ml", "lait", UnitView::French, true), "4,5 cl");
        assert_eq!(
            text(5.0, "ml", "extrait de vanille", UnitView::French, true),
            "5 ml"
        );
        assert_eq!(text(1.25, "l", "eau", UnitView::French, true), "1,3 l");
        assert_eq!(text(2.0, "l", "eau", UnitView::Us, true), "2 quarts");
        assert_eq!(text(1.0, "lb", "bœuf", UnitView::French, true), "455 g");
        assert_eq!(text(250.0, "g", "bœuf", UnitView::Us, true), "9 oz");
        assert_eq!(text(500.0, "g", "bœuf", UnitView::Us, true), "1 lb");
        assert_eq!(
            text(5.0, "g", "bœuf", UnitView::Us, true),
            "5 g",
            "under ½ oz"
        );
        assert_eq!(
            text(0.25, "tsp", "cannelle", UnitView::Weight, true),
            "¼ c. à c.",
            "under 1 g"
        );
        assert_eq!(
            text(2.0, "cup", "farine", UnitView::Recipe, true),
            "2 tasses"
        );
        assert_eq!(text(2.0, "cup", "flour", UnitView::Recipe, false), "2 cups");
        assert_eq!(text(500.0, "mg", "sel", UnitView::French, true), "500 mg");
    }

    #[test]
    fn views_round_trip_through_their_names() {
        for view in UnitView::ALL {
            assert_eq!(UnitView::parse(view.as_str()), Some(view));
        }
        assert_eq!(UnitView::parse("metric"), None);
        assert_eq!(UnitView::default(), UnitView::French);
    }
}
