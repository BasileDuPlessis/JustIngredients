//! Reading recipes: the list, the search, one recipe in a unit view, one conversion.
//!
//! The values returned are what every door shows: the Telegram card is rendered from a
//! [`RecipeView`], the MCP tools return them as JSON.

use serde::Serialize;
use sqlx::PgPool;
use tracing::debug;

use super::{EngineError, Lang, Owner};
use crate::unit_view::{self, UnitView};

/// The most recipes [`list`] returns: a user has a few hundred at most.
pub const MAX_LISTED: usize = 500;

/// A recipe in a list: its id, its name and, in [`list`], its memo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeEntry {
    pub id: i64,
    pub name: String,
    #[serde(rename = "memo", skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An ingredient of a recipe: as saved, and as the unit view shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShownIngredient {
    pub name: String,
    pub stored: StoredMeasure,
    pub shown: Shown,
}

/// A measure as saved: the quantity and the normalized unit, either missing ("sel").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredMeasure {
    pub quantity: Option<f64>,
    pub unit: Option<String>,
}

/// A measure as a card shows it ("180 g", "½ cuil. à café", "2"; empty without quantity nor unit),
/// and whether the view converted it from the saved unit. Text only: there is no numeric
/// converted value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub text: String,
    pub converted: bool,
}

/// One recipe as a card shows it, in a unit view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecipeView {
    pub id: i64,
    /// `None` for a recipe saved without a name (versions before naming was required).
    pub name: Option<String>,
    #[serde(rename = "memo")]
    pub note: Option<String>,
    pub unit_view: UnitView,
    pub ingredients: Vec<ShownIngredient>,
    /// The other recipes of the same photo (UX plan U9), alphabetical.
    pub linked: Vec<RecipeEntry>,
    /// The quantities shown are scaled (`engine::scaling`): the factor and why. The recipe as
    /// saved is in each ingredient's `stored`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<ScaleShown>,
}

/// A scale as a view tells it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScaleShown {
    pub factor: f64,
    /// "× 4/3 (œufs : 3 → 4)".
    pub text: String,
    /// The factor as a button carries it: "4/3".
    #[serde(skip)]
    pub ratio: super::scaling::Ratio,
}

/// What a [`RecipeView`] is made of, as read from the database.
#[derive(Debug, Clone, Copy)]
pub struct ReadRecipe<'a> {
    pub id: i64,
    pub name: Option<&'a str>,
    pub note: Option<&'a str>,
    pub ingredients: &'a [crate::db::Ingredient],
    pub linked: &'a [RecipeEntry],
    /// The recipe is written in English: "as in the recipe" names its units in English.
    pub english_recipe: bool,
}

/// A user's recipes as [`list`] gives them: at most [`MAX_LISTED`], and how many there are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeList {
    pub recipes: Vec<RecipeEntry>,
    pub total: usize,
}

/// The owner's named recipes in the order of the bot's list (case and accents ignored, numbers
/// by value, then oldest first), with their memos; at most [`MAX_LISTED`] of `total`.
pub async fn list(pool: &PgPool, owner: Owner) -> Result<RecipeList, EngineError> {
    let all = crate::db::alphabetical_recipes(pool, owner.telegram_id()).await?;
    let total = all.len();
    let recipes = all
        .into_iter()
        .take(MAX_LISTED)
        .map(|recipe| RecipeEntry {
            id: recipe.id,
            name: recipe.name,
            note: recipe.note,
        })
        .collect();
    Ok(RecipeList { recipes, total })
}

/// The owner's named recipes that the words of `query` find, best first, as the bot's search
/// ranks them: by words of the recipe's language ("poireaux" finds "Tarte aux poireaux",
/// "leeks" does not).
pub async fn search(
    pool: &PgPool,
    owner: Owner,
    query: &str,
) -> Result<Vec<RecipeEntry>, EngineError> {
    let recipes = crate::db::searchable_recipes(pool, owner.telegram_id()).await?;
    Ok(crate::search::search(&recipes, query)
        .into_iter()
        .map(|recipe| RecipeEntry {
            id: recipe.id,
            name: recipe.name.clone(),
            note: None,
        })
        .collect())
}

/// One recipe of the owner, its measures in `view` (the owner's own view when `None`), the
/// texts in `lang`. [`EngineError::NotFound`] when it is gone or another user's. The owner's view
/// or the linked recipes failing to read do not fail the recipe: the default view, no links.
pub async fn recipe(
    pool: &PgPool,
    owner: Owner,
    recipe_id: i64,
    view: Option<UnitView>,
    lang: Lang,
) -> Result<RecipeView, EngineError> {
    scaled_recipe(pool, owner, recipe_id, view, lang, None).await
}

/// The scale `asked` for a recipe of the owner ("x2", "4 oeufs", "20x30 → 30x40"), read against
/// its ingredients; [`EngineError::Scale`] with why it is refused, [`EngineError::NotFound`] for
/// a recipe that is not the owner's.
pub async fn scale_of(
    pool: &PgPool,
    owner: Owner,
    recipe_id: i64,
    asked: &str,
) -> Result<super::scaling::Scale, EngineError> {
    let telegram_id = owner.telegram_id();
    if crate::db::read_recipe_with_name(pool, telegram_id, recipe_id)
        .await?
        .is_none()
    {
        return Err(EngineError::NotFound);
    }
    let ingredients = crate::db::get_recipe_ingredients(pool, telegram_id, recipe_id).await?;
    let lines: Vec<super::scaling::Line<'_>> = ingredients
        .iter()
        .map(|ingredient| super::scaling::Line {
            name: &ingredient.name,
            quantity: ingredient.quantity,
            unit: ingredient.unit.as_deref(),
        })
        .collect();
    super::scaling::parse(asked, &lines).map_err(EngineError::Scale)
}

/// [`recipe`], its quantities times `scale`, rounded as a cook would (`engine::scaling`); the
/// recipe is not changed.
pub async fn scaled_recipe(
    pool: &PgPool,
    owner: Owner,
    recipe_id: i64,
    view: Option<UnitView>,
    lang: Lang,
    scale: Option<&super::scaling::Scale>,
) -> Result<RecipeView, EngineError> {
    let telegram_id = owner.telegram_id();
    let Some(recipe) = crate::db::read_recipe_with_name(pool, telegram_id, recipe_id).await? else {
        return Err(EngineError::NotFound);
    };
    let ingredients = crate::db::get_recipe_ingredients(pool, telegram_id, recipe_id).await?;
    let view = match view {
        Some(view) => view,
        None => crate::db::unit_view(pool, telegram_id)
            .await
            .unwrap_or_else(|error| {
                debug!(error = %error, "Could not read the unit view");
                UnitView::default()
            }),
    };
    // "As in the recipe" names units in the recipe's language
    let english_recipe = unit_view::recipe_is_english(
        &recipe.content,
        ingredients
            .iter()
            .map(|ingredient| ingredient.name.as_str()),
    );
    let linked: Vec<RecipeEntry> = crate::db::linked_recipes(pool, telegram_id, recipe_id)
        .await
        .unwrap_or_else(|error| {
            debug!(error = %error, "Could not read the linked recipes");
            Vec::new()
        })
        .into_iter()
        .map(|linked| RecipeEntry {
            id: linked.id,
            name: linked.name,
            note: None,
        })
        .collect();
    Ok(build_view(
        ReadRecipe {
            id: recipe.id,
            name: recipe.recipe_name.as_deref(),
            note: recipe.note.as_deref(),
            ingredients: &ingredients,
            linked: &linked,
            english_recipe,
        },
        view,
        lang,
        scale,
    ))
}

/// A recipe as read, its measures in `view` (times `scale`, if any), the texts in `lang`.
pub fn build_view(
    read: ReadRecipe<'_>,
    view: UnitView,
    lang: Lang,
    scale: Option<&super::scaling::Scale>,
) -> RecipeView {
    let factor = scale
        .map(|scale| scale.ratio)
        .filter(|ratio| !ratio.is_one())
        .map(super::scaling::Ratio::value);
    let ingredients = read
        .ingredients
        .iter()
        .map(|ingredient| {
            let show = |quantity: Option<f64>| {
                unit_view::show_measure(
                    quantity,
                    ingredient.unit.as_deref(),
                    &ingredient.name,
                    view,
                    lang.is_french(),
                    read.english_recipe,
                )
            };
            let measure = match (ingredient.quantity, factor) {
                (Some(quantity), Some(factor)) if quantity > 0.0 => {
                    // A view that converts rounds the exact scaled value itself; otherwise the
                    // scaled value is rounded in its own unit, as a cook would
                    let converted = show(Some(quantity * factor));
                    if converted.converted {
                        converted
                    } else {
                        show(Some(super::scaling::scaled(
                            quantity,
                            ingredient.unit.as_deref(),
                            factor,
                        )))
                    }
                }
                _ => show(ingredient.quantity),
            };
            ShownIngredient {
                name: ingredient.name.clone(),
                stored: StoredMeasure {
                    quantity: ingredient.quantity,
                    unit: ingredient.unit.clone(),
                },
                shown: Shown {
                    text: measure.text,
                    converted: measure.converted,
                },
            }
        })
        .collect();
    RecipeView {
        id: read.id,
        name: read.name.map(str::to_string),
        note: read.note.map(str::to_string),
        unit_view: view,
        ingredients,
        linked: read.linked.to_vec(),
        scale: scale
            .filter(|scale| !scale.ratio.is_one())
            .map(|scale| ScaleShown {
                factor: scale.ratio.value(),
                text: scale.describe(lang.is_french()),
                ratio: scale.ratio,
            }),
    }
}

/// One measure of `ingredient` in `view`, as a card would show it (2 cups of flour in the
/// weight view: "240 g"). Unit names follow `lang`, also in the "as in the recipe" view. A
/// quantity that is not a positive number up to `validation::MAX_QUANTITY` is refused.
pub fn convert(
    quantity: f64,
    unit: &str,
    ingredient: &str,
    view: UnitView,
    lang: Lang,
) -> Result<Shown, EngineError> {
    if !(quantity.is_finite() && quantity > 0.0 && quantity <= crate::validation::MAX_QUANTITY) {
        return Err(EngineError::Invalid(
            crate::errors::ValidationError::InvalidQuantity,
        ));
    }
    let measure = unit_view::show_measure(
        Some(quantity),
        Some(unit),
        ingredient,
        view,
        lang.is_french(),
        !lang.is_french(),
    );
    Ok(Shown {
        text: measure.text,
        converted: measure.converted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversion_reads_like_a_card() {
        let shown = |quantity, unit, view, lang| {
            convert(quantity, unit, "farine", view, lang).map_err(|error| error.to_string())
        };
        assert_eq!(
            shown(2.0, "cup", UnitView::Weight, Lang::Fr),
            Ok(Shown {
                text: "240 g".to_string(),
                converted: true
            })
        );
        assert_eq!(
            shown(2.0, "cups", UnitView::Weight, Lang::En),
            Ok(Shown {
                text: "240 g".to_string(),
                converted: true
            }),
            "a plural spelling is read like the unit"
        );
        assert_eq!(
            shown(1.0, "tbsp", UnitView::Recipe, Lang::Fr).map(|shown| shown.text),
            Ok("1 cuil. à soupe".to_string())
        );
        assert_eq!(
            shown(1.0, "tbsp", UnitView::Recipe, Lang::En).map(|shown| shown.text),
            Ok("1 tbsp".to_string())
        );
        for refused in [0.0, -1.0, f64::NAN, f64::INFINITY, 20_000.0] {
            assert!(
                matches!(
                    convert(refused, "g", "farine", UnitView::Weight, Lang::Fr),
                    Err(EngineError::Invalid(
                        crate::errors::ValidationError::InvalidQuantity
                    ))
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_recipe_view_keeps_the_saved_measure_and_shows_the_converted_one() {
        let at = chrono::Utc::now();
        let flour = crate::db::Ingredient {
            id: 1,
            user_id: 1,
            recipe_id: Some(7),
            name: "farine".to_string(),
            quantity: Some(2.0),
            unit: Some("cup".to_string()),
            created_at: at,
            updated_at: at,
        };
        let salt = crate::db::Ingredient {
            name: "sel".to_string(),
            quantity: None,
            unit: None,
            ..flour
        };
        let linked = [RecipeEntry {
            id: 8,
            name: "Tarte (sauce)".to_string(),
            note: None,
        }];
        let view = build_view(
            ReadRecipe {
                id: 7,
                name: Some("Tarte"),
                note: Some("Four 180 °C"),
                ingredients: &[flour, salt],
                linked: &linked,
                english_recipe: false,
            },
            UnitView::Weight,
            Lang::Fr,
            None,
        );
        assert_eq!(view.ingredients[0].stored.quantity, Some(2.0));
        assert_eq!(view.ingredients[0].stored.unit.as_deref(), Some("cup"));
        assert_eq!(view.ingredients[0].shown.text, "240 g");
        assert!(view.ingredients[0].shown.converted);
        assert_eq!(view.ingredients[1].shown.text, "", "no measure to show");
        assert!(!view.ingredients[1].shown.converted);

        let json = serde_json::to_value(&view).map_err(|error| error.to_string());
        let json = json.as_ref().map_err(String::as_str);
        assert_eq!(
            json.map(|json| json["memo"].as_str()),
            Ok(Some("Four 180 °C")),
            "the memo is named as the user knows it"
        );
        assert_eq!(
            json.map(|json| json["unit_view"].as_str()),
            Ok(Some("weight"))
        );
        assert_eq!(
            json.map(|json| json["linked"][0].get("memo").is_none()),
            Ok(true),
            "no memo key without a memo"
        );
    }

    #[test]
    fn a_scaled_view_rounds_like_a_cook_and_keeps_the_saved_measures() {
        use crate::engine::scaling::{Ratio, Reason, Scale};
        let at = chrono::Utc::now();
        let line = |name: &str, quantity: Option<f64>, unit: Option<&str>| crate::db::Ingredient {
            id: 1,
            user_id: 1,
            recipe_id: Some(7),
            name: name.to_string(),
            quantity,
            unit: unit.map(str::to_string),
            created_at: at,
            updated_at: at,
        };
        let ingredients = [
            line("farine", Some(250.0), Some("g")),
            line("oeufs", Some(3.0), None),
            line("sel", Some(1.0), Some("tsp")),
            line("lait", Some(1.0), Some("cup")),
            line("poivre", None, None),
        ];
        let read = ReadRecipe {
            id: 7,
            name: Some("Tarte"),
            note: None,
            ingredients: &ingredients,
            linked: &[],
            english_recipe: false,
        };
        let scale = Scale {
            ratio: Ratio::new(4, 3).unwrap_or(Ratio { num: 1, den: 1 }),
            reason: Reason::Ingredient {
                name: "oeufs".to_string(),
                from: "3".to_string(),
                to: "4".to_string(),
            },
        };
        let view = build_view(read, UnitView::Recipe, Lang::Fr, Some(&scale));
        let shown: Vec<&str> = view
            .ingredients
            .iter()
            .map(|ingredient| ingredient.shown.text.as_str())
            .collect();
        assert_eq!(
            shown,
            ["335 g", "4", "1 ⅓ cuil. à café", "1 ⅓ tasse", ""],
            "grams to 5 g, eggs whole, spoons and cups to a third, no quantity unchanged"
        );
        assert_eq!(
            view.ingredients[0].stored.quantity,
            Some(250.0),
            "saved as is"
        );
        assert_eq!(
            view.scale.as_ref().map(|scale| scale.text.as_str()),
            Some("× 4/3 (oeufs : 3 → 4)")
        );

        // A view that converts rounds the exact scaled value itself: 4/3 cup of milk in grams
        let weight = build_view(read, UnitView::Weight, Lang::Fr, Some(&scale));
        assert_ne!(weight.ingredients[3].shown.text, "1 ⅓ tasse");
        assert!(weight.ingredients[3].shown.converted);

        // Times one is no scale at all
        let one = Scale::factor(Ratio { num: 1, den: 1 });
        assert!(
            build_view(read, UnitView::Recipe, Lang::Fr, Some(&one))
                .scale
                .is_none()
        );
    }
}
