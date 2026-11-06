//! # Units
//!
//! Canonical measurement units, as defined by the unit contract in
//! `specs/tasks/measurement-standardization-tasks.md` (Task 1). Every unit that is stored or
//! shown goes through [`normalize_measurement`], whether it comes from the extraction model or
//! from a user edit, so `g`, `gr`, `GR` and `grammes` all end up as `g`.
//!
//! Rules:
//! - matching ignores case, surrounding spaces, dots and French accents (`c. à s.` = `cas`);
//! - `cl` and `dl` are not spellings of `ml`: they are converted, scaling the quantity;
//! - `cc` is a French teaspoon (`cuillère à café`), not a cubic centimetre;
//! - unknown units (`pincée`, `gousse`, …) are kept, lowercased and trimmed.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt;

/// A unit with a single canonical spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalUnit {
    Gram,
    Kilogram,
    Milligram,
    Pound,
    Ounce,
    Cup,
    Teaspoon,
    Tablespoon,
    Pint,
    Quart,
    Gallon,
    Liter,
    Milliliter,
}

impl CanonicalUnit {
    /// Every canonical unit.
    pub const ALL: [Self; 13] = [
        Self::Gram,
        Self::Kilogram,
        Self::Milligram,
        Self::Pound,
        Self::Ounce,
        Self::Cup,
        Self::Teaspoon,
        Self::Tablespoon,
        Self::Pint,
        Self::Quart,
        Self::Gallon,
        Self::Liter,
        Self::Milliliter,
    ];

    /// The canonical spelling stored in the database and shown to users.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gram => "g",
            Self::Kilogram => "kg",
            Self::Milligram => "mg",
            Self::Pound => "lb",
            Self::Ounce => "oz",
            Self::Cup => "cup",
            Self::Teaspoon => "tsp",
            Self::Tablespoon => "tbsp",
            Self::Pint => "pint",
            Self::Quart => "quart",
            Self::Gallon => "gallon",
            Self::Liter => "l",
            Self::Milliliter => "ml",
        }
    }
}

impl fmt::Display for CanonicalUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Spellings of each canonical unit, compared after [`match_key`].
const ALIASES: &[(CanonicalUnit, &[&str])] = &[
    (
        CanonicalUnit::Gram,
        &["g", "gr", "gram", "grams", "gramme", "grammes"],
    ),
    (
        CanonicalUnit::Kilogram,
        &[
            "kg",
            "kilo",
            "kilos",
            "kilogram",
            "kilograms",
            "kilogramme",
            "kilogrammes",
        ],
    ),
    (
        CanonicalUnit::Milligram,
        &[
            "mg",
            "milligram",
            "milligrams",
            "milligramme",
            "milligrammes",
        ],
    ),
    (CanonicalUnit::Pound, &["lb", "lbs", "pound", "pounds"]),
    (CanonicalUnit::Ounce, &["oz", "ounce", "ounces"]),
    (CanonicalUnit::Cup, &["cup", "cups"]),
    (
        CanonicalUnit::Teaspoon,
        &[
            "tsp",
            "tsps",
            "teaspoon",
            "teaspoons",
            "cuillère à café",
            "cuillères à café",
            "cuillerée à café",
            "cuillerées à café",
            "cuil à café",
            "cuil. à café",
            "cuill. à café",
            "c. à café",
            // Québec: the teaspoon is "à thé"
            "cuillère à thé",
            "cuillères à thé",
            "cuillerée à thé",
            "cuil. à thé",
            "c. à thé",
            "c. à c.",
            "cc",
        ],
    ),
    (
        CanonicalUnit::Tablespoon,
        &[
            "tbsp",
            "tbsps",
            "tbs",
            "tbl",
            "tablespoon",
            "tablespoons",
            "cuillère à soupe",
            "cuillères à soupe",
            "cuillerée à soupe",
            "cuillerées à soupe",
            "cuil à soupe",
            "cuil. à soupe",
            "cuill. à soupe",
            "c. à soupe",
            "c. à s.",
            "cs",
        ],
    ),
    (CanonicalUnit::Pint, &["pint", "pints"]),
    (CanonicalUnit::Quart, &["quart", "quarts"]),
    (CanonicalUnit::Gallon, &["gallon", "gallons"]),
    (
        CanonicalUnit::Liter,
        &["l", "liter", "liters", "litre", "litres"],
    ),
    (
        CanonicalUnit::Milliliter,
        &[
            "ml",
            "milliliter",
            "milliliters",
            "millilitre",
            "millilitres",
            "cm3",
            "cm³",
        ],
    ),
];

/// Units that are a multiple of a canonical unit: the quantity is scaled on conversion.
const SCALED_ALIASES: &[(&[&str], CanonicalUnit, f64)] = &[
    (
        &[
            "cl",
            "centilitre",
            "centilitres",
            "centiliter",
            "centiliters",
        ],
        CanonicalUnit::Milliliter,
        10.0,
    ),
    (
        &["dl", "décilitre", "décilitres", "deciliter", "deciliters"],
        CanonicalUnit::Milliliter,
        100.0,
    ),
];

/// Units that count things ("2 gousses d'ail", "1 pincée de sel"): no canonical spelling, they
/// are kept as typed.
const COUNT_UNITS: &[&str] = &[
    "tasse",
    "tasses",
    "verre",
    "verres",
    "cuillère",
    "cuillères",
    "pincée",
    "pincées",
    "poignée",
    "poignées",
    "sachet",
    "sachets",
    "paquet",
    "paquets",
    "boîte",
    "boîtes",
    "conserve",
    "conserves",
    "pot",
    "pots",
    "bouteille",
    "bouteilles",
    "tranche",
    "tranches",
    "morceau",
    "morceaux",
    "pièce",
    "pièces",
    "gousse",
    "gousses",
    "brin",
    "brins",
    "botte",
    "bottes",
    "bâton",
    "bâtons",
    "feuille",
    "feuilles",
    "goutte",
    "gouttes",
    "zeste",
    "zestes",
    "bouquet",
    "bouquets",
    "filet",
    "filets",
    "slice",
    "slices",
    "can",
    "cans",
    "bottle",
    "bottles",
    "stick",
    "sticks",
    "packet",
    "packets",
    "pkg",
    "bag",
    "bags",
    "dash",
    "dashes",
    "pinch",
    "pinches",
    "drop",
    "drops",
    "cube",
    "cubes",
    "piece",
    "pieces",
    "handful",
    "handfuls",
    "clove",
    "cloves",
    "sprig",
    "sprigs",
    "sheet",
    "sheets",
    "portion",
    "portions",
    "serving",
    "servings",
];

/// Count units that also start dish names ("1 bouquet garni", "1 filet mignon"): a unit only
/// before "de", "d'" or "of".
const NEEDS_ARTICLE: &[&str] = &["bouquet", "bouquets", "filet", "filets"];

/// Most words in a unit spelling ("cuillères à soupe", "c. à c.").
const MAX_UNIT_WORDS: usize = 3;

/// A unit read at the start of typed words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeadingUnit {
    /// How many words the unit spans.
    pub words: usize,
    /// A unit of [`NEEDS_ARTICLE`]: only a unit before "de", "d'" or "of".
    pub needs_article: bool,
}

fn is_measure(spelling: &str) -> bool {
    let key = match_key(spelling);
    canonicalize_unit(spelling).is_some()
        || SCALED_ALIASES
            .iter()
            .any(|(aliases, _, _)| aliases.iter().any(|alias| match_key(alias) == key))
}

fn is_count_unit(spelling: &str) -> bool {
    let key = match_key(spelling);
    COUNT_UNITS.iter().any(|unit| match_key(unit) == key)
}

/// Whether a spelling is a unit: canonical, converted (`cl`, `dl`) or counting (`gousses`).
pub fn is_unit(spelling: &str) -> bool {
    is_measure(spelling) || is_count_unit(spelling)
}

/// The unit that `words` start with, the longest spelling first ("c. à soupe" before "c").
pub fn leading_unit(words: &[&str]) -> Option<LeadingUnit> {
    (1..=MAX_UNIT_WORDS.min(words.len()))
        .rev()
        .find_map(|count| {
            let spelling = words[..count].join(" ");
            let key = match_key(&spelling);
            (is_measure(&spelling) || is_count_unit(&spelling)).then(|| LeadingUnit {
                words: count,
                needs_article: NEEDS_ARTICLE.iter().any(|unit| match_key(unit) == key),
            })
        })
}

/// The spoon a printed line measures with: "1 cuil. à café de levure" → teaspoon, "2 tbsp
/// oil" → tablespoon. `None` without a spoon, or when the line names both ("1 cuil. à soupe,
/// soit 3 cuil. à café"): then it says nothing sure.
pub fn spoon_in(line: &str) -> Option<CanonicalUnit> {
    let words: Vec<&str> = line
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .collect();
    let mut found = None;
    for start in 0..words.len() {
        let longest = MAX_UNIT_WORDS.min(words.len() - start);
        let spoon = (1..=longest).rev().find_map(|count| {
            match canonicalize_unit(&words[start..start + count].join(" ")) {
                Some(unit @ (CanonicalUnit::Teaspoon | CanonicalUnit::Tablespoon)) => Some(unit),
                _ => None,
            }
        });
        if let Some(spoon) = spoon {
            if found.is_some_and(|seen| seen != spoon) {
                return None;
            }
            found = Some(spoon);
        }
    }
    found
}

/// A quantity and unit after normalization.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedMeasurement {
    pub quantity: Option<f64>,
    /// Canonical spelling for known units, cleaned original otherwise; `None` when empty.
    pub unit: Option<String>,
}

/// Lowercases, trims, collapses inner whitespace and strips trailing `.` and `,`.
pub fn clean_unit(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_end_matches(['.', ','])
        .trim_end()
        .to_lowercase()
}

/// Key used to compare spellings: lowercase, no dots or spaces, French accents folded.
fn match_key(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .map(|c| match c {
            'à' | 'â' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' => 'o',
            'ù' | 'û' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Returns the canonical unit for a spelling, or `None` if the unit is unknown or scaled
/// (`cl`, `dl`: use [`normalize_measurement`], which also converts the quantity).
pub fn canonicalize_unit(raw: &str) -> Option<CanonicalUnit> {
    // US cookbooks abbreviate tablespoon as "T" and teaspoon as "t": the case is the meaning.
    match raw.trim().trim_end_matches('.') {
        "T" => return Some(CanonicalUnit::Tablespoon),
        "t" => return Some(CanonicalUnit::Teaspoon),
        _ => {}
    }
    let key = match_key(raw);
    ALIASES.iter().find_map(|(unit, aliases)| {
        aliases
            .iter()
            .any(|alias| match_key(alias) == key)
            .then_some(*unit)
    })
}

/// Normalizes a quantity and unit: canonical spelling for known units, `cl`/`dl` converted to
/// `ml` with the quantity scaled, unknown units kept as cleaned text, empty units dropped.
pub fn normalize_measurement(
    quantity: Option<f64>,
    raw_unit: Option<&str>,
) -> NormalizedMeasurement {
    let Some(raw_unit) = raw_unit.filter(|unit| !unit.trim().is_empty()) else {
        return NormalizedMeasurement {
            quantity,
            unit: None,
        };
    };

    if let Some(unit) = canonicalize_unit(raw_unit) {
        return NormalizedMeasurement {
            quantity,
            unit: Some(unit.as_str().to_string()),
        };
    }

    let key = match_key(raw_unit);
    for (aliases, unit, factor) in SCALED_ALIASES {
        if aliases.iter().any(|alias| match_key(alias) == key) {
            return NormalizedMeasurement {
                quantity: quantity.map(|q| q * factor),
                unit: Some(unit.as_str().to_string()),
            };
        }
    }

    NormalizedMeasurement {
        quantity,
        unit: Some(clean_unit(raw_unit)),
    }
}

/// Quantities from this value up do not fit `ingredients.quantity DECIMAL(10,3)`.
const UNSTORABLE_QUANTITY: f64 = 10_000_000.0;

/// What a saved quantity and unit become once normalized like new ones (the cleanup of units
/// saved before normalization): `None` when there is nothing to change, or nothing safe to
/// write. Safe means a finite quantity that fits the column, and a result that normalizing again
/// would not change ("T," would become "t", then "tsp": left as saved).
pub fn renormalized(quantity: Option<f64>, unit: &str) -> Option<NormalizedMeasurement> {
    if quantity.is_some_and(|q| !q.is_finite()) {
        return None;
    }
    let normalized = normalize_measurement(quantity, Some(unit));
    if normalized.unit.as_deref() == Some(unit) && normalized.quantity == quantity {
        return None;
    }
    let again = normalize_measurement(normalized.quantity, normalized.unit.as_deref());
    let storable = normalized
        .quantity
        .is_none_or(|q| q.is_finite() && q.abs() < UNSTORABLE_QUANTITY);
    (again == normalized && storable).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_units_are_renormalized_only_when_safe() {
        let after = |quantity, unit| {
            renormalized(quantity, unit).map(|normalized| (normalized.quantity, normalized.unit))
        };
        assert_eq!(
            after(Some(5.0), "G"),
            Some((Some(5.0), Some("g".to_string())))
        );
        assert_eq!(
            after(Some(8.0), "cl"),
            Some((Some(80.0), Some("ml".to_string())))
        );
        assert_eq!(after(None, "cl"), Some((None, Some("ml".to_string()))));
        assert_eq!(
            after(Some(1.0), "PINCÉES"),
            Some((Some(1.0), Some("pincées".to_string())))
        );
        assert_eq!(
            after(Some(2.0), ""),
            Some((Some(2.0), None)),
            "blank: no unit"
        );
        assert_eq!(after(Some(250.0), "g"), None, "already normalized");
        assert_eq!(after(Some(2.0), "gousse"), None);
        assert_eq!(after(Some(1.0), "T,"), None, "unstable: t, then tsp");
        assert_eq!(after(Some(f64::NAN), "G"), None);
        assert_eq!(
            after(Some(100_000.0), "dl"),
            None,
            "10 000 000 ml does not fit"
        );
    }

    #[test]
    fn spellings_map_to_canonical_units() {
        let cases = [
            ("g", "g"),
            ("gr", "g"),
            ("GR", "g"),
            ("g.", "g"),
            ("grammes", "g"),
            ("KILOGRAMME", "kg"),
            (" Kg ", "kg"),
            ("lbs", "lb"),
            ("Tablespoons", "tbsp"),
            ("teaspoon", "tsp"),
            ("cuillère à soupe", "tbsp"),
            ("cuillères à café", "tsp"),
            ("c. à s.", "tbsp"),
            ("c.à.s", "tbsp"),
            ("c à c", "tsp"),
            ("cuillere a soupe", "tbsp"),
            ("cc", "tsp"),
            ("cs", "tbsp"),
            ("Litres", "l"),
            ("millilitres", "ml"),
            ("cm3", "ml"),
            ("cm³", "ml"),
            ("T", "tbsp"),
            ("t.", "tsp"),
            ("tbs", "tbsp"),
            ("c. à soupe", "tbsp"),
            ("cuillerée à café", "tsp"),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                canonicalize_unit(raw).map(CanonicalUnit::as_str),
                Some(expected),
                "{raw}"
            );
        }
    }

    #[test]
    fn no_spelling_belongs_to_two_units() {
        let mut seen = std::collections::HashMap::new();
        let canonical = ALIASES
            .iter()
            .flat_map(|(unit, aliases)| aliases.iter().map(move |alias| (alias, unit.as_str())));
        let scaled = SCALED_ALIASES.iter().flat_map(|(aliases, _, factor)| {
            aliases
                .iter()
                .map(move |alias| (alias, if *factor == 10.0 { "cl" } else { "dl" }))
        });
        for (alias, unit) in canonical.chain(scaled) {
            if let Some(previous) = seen.insert(match_key(alias), unit) {
                assert_eq!(
                    previous, unit,
                    "'{alias}' is listed for {previous} and {unit}"
                );
            }
        }
    }

    #[test]
    fn unknown_and_scaled_units_have_no_canonical_unit() {
        for raw in ["pincée", "gousse", "fl oz", "cl", "dl", ""] {
            assert_eq!(canonicalize_unit(raw), None, "{raw}");
        }
    }

    #[test]
    fn centilitres_and_decilitres_convert_to_millilitres() {
        assert_eq!(
            normalize_measurement(Some(20.0), Some("cl")),
            NormalizedMeasurement {
                quantity: Some(200.0),
                unit: Some("ml".to_string())
            }
        );
        assert_eq!(
            normalize_measurement(Some(2.5), Some("dL")),
            NormalizedMeasurement {
                quantity: Some(250.0),
                unit: Some("ml".to_string())
            }
        );
        assert_eq!(
            normalize_measurement(None, Some("cl")),
            NormalizedMeasurement {
                quantity: None,
                unit: Some("ml".to_string())
            }
        );
    }

    #[test]
    fn unknown_units_are_kept_cleaned() {
        assert_eq!(
            normalize_measurement(Some(2.0), Some("  Gousses. ")),
            NormalizedMeasurement {
                quantity: Some(2.0),
                unit: Some("gousses".to_string())
            }
        );
    }

    #[test]
    fn empty_units_are_dropped() {
        for raw in [None, Some(""), Some("   ")] {
            assert_eq!(
                normalize_measurement(Some(3.0), raw),
                NormalizedMeasurement {
                    quantity: Some(3.0),
                    unit: None
                }
            );
        }
    }

    #[test]
    fn typed_words_start_with_the_longest_unit() {
        let unit = |line: &str| leading_unit(&line.split(' ').collect::<Vec<_>>());
        let measure = |words| {
            Some(LeadingUnit {
                words,
                needs_article: false,
            })
        };
        assert_eq!(unit("c. à soupe d'huile"), measure(3));
        assert_eq!(unit("cuillères à café de sel"), measure(3));
        assert_eq!(unit("g farine"), measure(1));
        assert_eq!(unit("cl de crème"), measure(1), "converted units too");
        assert_eq!(unit("gousses d'ail"), measure(1), "count units too");
        assert_eq!(unit("can tomatoes"), measure(1));
        assert_eq!(
            unit("bouquet garni"),
            Some(LeadingUnit {
                words: 1,
                needs_article: true
            })
        );
        assert_eq!(unit("oeufs"), None);
        assert_eq!(unit("grosses carottes"), None);
        assert!(is_unit("Pincées") && is_unit("dl") && is_unit("kg") && !is_unit("farine"));
    }

    #[test]
    fn count_units_are_not_measures() {
        for unit in COUNT_UNITS {
            assert!(!is_measure(unit), "{unit} is also a measure");
        }
    }

    #[test]
    fn known_units_keep_the_quantity() {
        assert_eq!(
            normalize_measurement(Some(150.0), Some("Grammes")),
            NormalizedMeasurement {
                quantity: Some(150.0),
                unit: Some("g".to_string())
            }
        );
    }
    #[test]
    fn the_spoon_of_a_printed_line_is_read() {
        for (line, spoon) in [
            (
                "1 cuil. à café de levure chimique",
                Some(CanonicalUnit::Teaspoon),
            ),
            ("1 'cuil. à café de levure", Some(CanonicalUnit::Teaspoon)),
            ("1 cuil. à soupe de lait.", Some(CanonicalUnit::Tablespoon)),
            ("2 c. à c. de sel", Some(CanonicalUnit::Teaspoon)),
            ("2 c.à.s d'huile", Some(CanonicalUnit::Tablespoon)),
            ("1 càc de cannelle", Some(CanonicalUnit::Teaspoon)),
            ("1 cuillère à thé de sel", Some(CanonicalUnit::Teaspoon)),
            (
                "1 cuillères à soupe rases de sucre",
                Some(CanonicalUnit::Tablespoon),
            ),
            ("(1 cuil. à café)", Some(CanonicalUnit::Teaspoon)),
            ("2 tbsp olive oil", Some(CanonicalUnit::Tablespoon)),
            ("1 teaspoon salt", Some(CanonicalUnit::Teaspoon)),
            ("250 g de farine", None),
            ("1 cuillère de miel", None),
            ("1 cuil. à soupe, soit 3 cuil. à café", None),
            ("", None),
        ] {
            assert_eq!(spoon_in(line), spoon, "{line}");
        }
    }

    #[test]
    fn abbreviated_and_quebec_spoons_are_known() {
        for teaspoon in ["cuill. à café", "cuillère à thé", "c. à thé", "cuil. à thé"] {
            assert_eq!(
                canonicalize_unit(teaspoon),
                Some(CanonicalUnit::Teaspoon),
                "{teaspoon}"
            );
        }
        assert_eq!(
            canonicalize_unit("cuill. à soupe"),
            Some(CanonicalUnit::Tablespoon)
        );
    }
}
