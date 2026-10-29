//! Recipe search (UX plan U4): the user's words are looked for in recipe names and ingredient
//! names, ignoring case, French accents, plurals and small words ("de", "aux"…).
//!
//! A user has at most a few hundred recipes, so their names and ingredients are read in one
//! query and matched here: no Postgres extension (`unaccent` needs rights the app may not have)
//! and no index to keep.

use chrono::{DateTime, Utc};

/// A recipe as the search sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchableRecipe {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub ingredients: Vec<String>,
}

/// Words that say nothing about a recipe ("tarte aux pommes" is found by "tarte pommes",
/// "recette de crêpes" by "crêpes"). Not "the": "thé" folds to it.
const SMALL_WORDS: [&str; 24] = [
    "a", "au", "aux", "d", "de", "des", "du", "en", "et", "l", "la", "le", "les", "un", "une",
    "avec", "of", "and", "with", "an", "recette", "recettes", "recipe", "recipes",
];

/// Query words at least this long also find longer words they start ("chocol" finds
/// "chocolat"); shorter ones must be whole words ("poire" does not find "poireau").
const PREFIX_MIN_CHARS: usize = 6;

/// Longest typed text taken as a search: a longer one, or several lines, is rather a recipe
/// pasted or typed out, which the bot only reads from a photo.
pub const MAX_QUERY_CHARS: usize = 80;

/// Whether a text typed outside an action is a search.
pub fn is_query(text: &str) -> bool {
    let text = text.trim();
    !text.contains('\n') && text.chars().count() <= MAX_QUERY_CHARS
}

/// Lowercase, without accents (also when stored as separate combining marks), ligatures spelled
/// out ("crème brûlée" → "creme brulee", "œuf" → "oeuf"), anything else than letters and
/// digits turned into spaces.
pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => folded.push('a'),
            'é' | 'è' | 'ê' | 'ë' => folded.push('e'),
            'î' | 'ï' | 'í' | 'ì' => folded.push('i'),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => folded.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => folded.push('u'),
            'ç' => folded.push('c'),
            'ñ' => folded.push('n'),
            'ÿ' => folded.push('y'),
            'œ' => folded.push_str("oe"),
            'æ' => folded.push_str("ae"),
            // Combining accents of decomposed text ("e" + U+0300): dropped
            '\u{0300}'..='\u{036F}' => {}
            c if c.is_alphanumeric() => folded.push(c),
            _ => folded.push(' '),
        }
    }
    folded
}

/// A folded word without its plural ending, applied to the query and to the recipes alike:
/// "pommes" → "pomme", "choux" → "chou", "gateaux" → "gateau", "tomatoes" → "tomato",
/// "cherries" → "cherry". Short words stay as they are ("riz", "noix" → "noi" on both sides).
fn singular(word: &str) -> String {
    if word.chars().count() <= 3 {
        return word.to_string();
    }
    if let Some(stem) = word.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if let Some(stem) = word.strip_suffix("oes") {
        return format!("{stem}o");
    }
    if word.ends_with("ss") {
        return word.to_string();
    }
    word.strip_suffix('s')
        .or_else(|| word.strip_suffix('x'))
        .unwrap_or(word)
        .to_string()
}

/// The words of a text, folded and singular.
fn words_of(text: &str) -> Vec<String> {
    fold(text).split_whitespace().map(singular).collect()
}

/// The words of a query worth looking for: folded, singular, without small words and single
/// letters.
pub fn query_words(query: &str) -> Vec<String> {
    fold(query)
        .split_whitespace()
        .filter(|word| !SMALL_WORDS.contains(word) && word.chars().count() > 1)
        .map(singular)
        .collect()
}

fn word_matches(query_word: &str, text_words: &[String]) -> bool {
    text_words.iter().any(|word| {
        word == query_word
            || (query_word.chars().count() >= PREFIX_MIN_CHARS && word.starts_with(query_word))
    })
}

/// The recipes that contain every word of `query` in their name or an ingredient, as whole
/// words, best first: a word in the name counts more than in an ingredient, then the newest
/// first. No word (an empty query, only small words) finds nothing.
pub fn search<'a>(recipes: &'a [SearchableRecipe], query: &str) -> Vec<&'a SearchableRecipe> {
    let words = query_words(query);
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u32, &SearchableRecipe)> = recipes
        .iter()
        .filter_map(|recipe| {
            let name = words_of(&recipe.name);
            let ingredients: Vec<String> = recipe
                .ingredients
                .iter()
                .flat_map(|ingredient| words_of(ingredient))
                .collect();
            let mut score = 0;
            for word in &words {
                if word_matches(word, &name) {
                    score += 3;
                } else if word_matches(word, &ingredients) {
                    score += 1;
                } else {
                    return None;
                }
            }
            Some((score, recipe))
        })
        .collect();
    found.sort_by(|(a_score, a), (b_score, b)| {
        b_score
            .cmp(a_score)
            .then(b.created_at.cmp(&a.created_at))
            .then(b.id.cmp(&a.id))
    });
    found.into_iter().map(|(_, recipe)| recipe).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn recipe(id: i64, name: &str, day: u32, ingredients: &[&str]) -> SearchableRecipe {
        SearchableRecipe {
            id,
            name: name.to_string(),
            created_at: Utc
                .with_ymd_and_hms(2026, 10, day, 12, 0, 0)
                .single()
                .unwrap_or_default(),
            ingredients: ingredients.iter().map(|i| i.to_string()).collect(),
        }
    }

    fn ids(found: Vec<&SearchableRecipe>) -> Vec<i64> {
        found.iter().map(|recipe| recipe.id).collect()
    }

    #[test]
    fn accents_case_and_ligatures_are_ignored() {
        assert_eq!(fold("Crème Brûlée"), "creme brulee");
        assert_eq!(fold("Œufs, sucre & farine"), "oeufs  sucre   farine");
        let recipes = [recipe(1, "Crème brûlée", 1, &["œufs", "sucre"])];
        assert_eq!(ids(search(&recipes, "creme")), vec![1]);
        assert_eq!(ids(search(&recipes, "BRULEE")), vec![1]);
        assert_eq!(
            ids(search(&recipes, "oeuf")),
            vec![1],
            "ingredient, ligature"
        );
    }

    #[test]
    fn every_word_must_be_found_small_words_and_plurals_do_not_matter() {
        let recipes = [
            recipe(1, "Tarte aux pommes", 1, &["pomme", "farine"]),
            recipe(2, "Tarte au citron", 2, &["citron", "farine"]),
            recipe(3, "Crêpes", 3, &["farine de blé", "lait"]),
        ];
        assert_eq!(ids(search(&recipes, "tarte pommes")), vec![1]);
        assert_eq!(ids(search(&recipes, "tarte aux pomme")), vec![1]);
        assert_eq!(ids(search(&recipes, "farine de blé")), vec![3]);
        assert!(search(&recipes, "tarte chocolat").is_empty());
        assert!(search(&recipes, "de la").is_empty(), "only small words");
        assert!(search(&recipes, "  ").is_empty());
    }

    #[test]
    fn whole_words_only_no_false_friends() {
        let recipes = [
            recipe(1, "Bœuf bourguignon", 1, &["bœuf", "vin rouge"]),
            recipe(2, "Omelette", 2, &["œufs", "sel"]),
            recipe(3, "Poêlée", 3, &["poireaux", "poivre", "poivron"]),
            recipe(4, "Velouté de petits pois", 4, &["pois", "eau"]),
            recipe(5, "Gâteau au chocolat noir", 5, &["chocolat noir", "noix"]),
            recipe(6, "Thé glacé", 6, &["thé", "citron"]),
            recipe(7, "Tomato soup", 7, &["tomatoes", "cherries"]),
            recipe(
                8,
                "Pain maison",
                8,
                &["fraises", "crème fraîche", "vanille", "volaille"],
            ),
            recipe(9, "Salade", 9, &["maïs", "fromage frais", "anis", "ail"]),
        ];
        assert_eq!(ids(search(&recipes, "œufs")), vec![2], "not boeuf");
        assert_eq!(
            ids(search(&recipes, "poires")),
            Vec::<i64>::new(),
            "not poireaux"
        );
        assert_eq!(ids(search(&recipes, "pois")), vec![4], "not poivre");
        assert_eq!(ids(search(&recipes, "noix")), vec![5]);
        assert_eq!(ids(search(&recipes, "eau")), vec![4], "not gâteau");
        assert_eq!(
            ids(search(&recipes, "frais")),
            vec![9],
            "not fraises, fraîche"
        );
        assert_eq!(ids(search(&recipes, "maïs")), vec![9], "not maison");
        assert_eq!(ids(search(&recipes, "anis")), vec![9], "not vanille");
        assert_eq!(ids(search(&recipes, "ail")), vec![9], "not volaille");
        assert_eq!(ids(search(&recipes, "fraises")), vec![8]);
        assert_eq!(ids(search(&recipes, "thé")), vec![6]);
        assert_eq!(ids(search(&recipes, "gateaux")), vec![5]);
        assert_eq!(ids(search(&recipes, "tomato")), vec![7]);
        assert_eq!(ids(search(&recipes, "cherry")), vec![7]);
        assert_eq!(
            ids(search(&recipes, "chocol")),
            vec![5],
            "a long start of a word"
        );
        assert_eq!(ids(search(&recipes, "recette de thé")), vec![6]);
        assert!(
            search(&recipes, "e").is_empty(),
            "single letters say nothing"
        );
        assert_eq!(ids(search(&recipes, "Cre\u{0300}me")), vec![8]);
        assert_eq!(
            fold("Cre\u{0300}me"),
            "creme",
            "decomposed accents fold too"
        );
    }

    #[test]
    fn a_pasted_recipe_is_not_a_query() {
        assert!(is_query("tarte aux pommes"));
        assert!(is_query("  farine \n"), "a trailing new line is trimmed");
        assert!(!is_query("200 g farine\n3 œufs"));
        assert!(!is_query(&"farine ".repeat(20)));
        assert!(
            is_query(&"é".repeat(MAX_QUERY_CHARS)),
            "characters, not bytes"
        );
    }

    #[test]
    fn names_rank_above_ingredients_then_the_newest_first() {
        let recipes = [
            recipe(1, "Pain", 1, &["farine"]),
            recipe(2, "Farine maison", 2, &["blé"]),
            recipe(3, "Crêpes", 3, &["farine"]),
        ];
        assert_eq!(ids(search(&recipes, "farine")), vec![2, 3, 1]);
    }
}
