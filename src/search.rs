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

/// Words that say nothing about a recipe ("tarte aux pommes" is found by "tarte pommes").
const SMALL_WORDS: [&str; 21] = [
    "a", "au", "aux", "d", "de", "des", "du", "en", "et", "l", "la", "le", "les", "un", "une",
    "avec", "of", "the", "and", "with", "an",
];

/// Lowercase, without accents, ligatures spelled out ("crème brûlée" → "creme brulee",
/// "œuf" → "oeuf"), anything else than letters and digits turned into spaces.
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
            c if c.is_alphanumeric() => folded.push(c),
            _ => folded.push(' '),
        }
    }
    folded
}

/// The words of a query worth looking for, folded, plurals reduced ("pommes" → "pomme",
/// "choux" → "chou").
pub fn query_words(query: &str) -> Vec<String> {
    fold(query)
        .split_whitespace()
        .filter(|word| !SMALL_WORDS.contains(word))
        .map(|word| {
            let singular = word
                .strip_suffix('s')
                .or_else(|| word.strip_suffix('x'))
                .filter(|stem| stem.chars().count() >= 3);
            singular.unwrap_or(word).to_string()
        })
        .collect()
}

/// The recipes that contain every word of `query` in their name or an ingredient, best first:
/// a word in the name counts more than in an ingredient, then the newest first. No word (an
/// empty query, or only small words) finds nothing.
pub fn search<'a>(recipes: &'a [SearchableRecipe], query: &str) -> Vec<&'a SearchableRecipe> {
    let words = query_words(query);
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u32, &SearchableRecipe)> = recipes
        .iter()
        .filter_map(|recipe| {
            let name = fold(&recipe.name);
            let ingredients: Vec<String> = recipe.ingredients.iter().map(|i| fold(i)).collect();
            let mut score = 0;
            for word in &words {
                if name.contains(word.as_str()) {
                    score += 3;
                } else if ingredients
                    .iter()
                    .any(|ingredient| ingredient.contains(word.as_str()))
                {
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
    fn names_rank_above_ingredients_then_the_newest_first() {
        let recipes = [
            recipe(1, "Pain", 1, &["farine"]),
            recipe(2, "Farine maison", 2, &["blé"]),
            recipe(3, "Crêpes", 3, &["farine"]),
        ];
        assert_eq!(ids(search(&recipes, "farine")), vec![2, 3, 1]);
    }
}
