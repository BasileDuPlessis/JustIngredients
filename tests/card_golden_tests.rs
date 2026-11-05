//! The exact text of recipe cards, frozen before the card is built from the engine
//! (`specs/tasks/engine-api.md`, tranche 1): four unit views, French and English users, a French
//! and an English recipe, with a memo, linked recipes, converted and unconverted lines.
//!
//! The expected cards are the files of `tests/golden/cards/`. After a deliberate change of the
//! card, write them again with `JI_UPDATE_GOLDEN=1` and review the diff.
//!
//! Like `db_tests`, they need Postgres: `cargo test --test card_golden_tests -- --include-ignored`
//! with `DATABASE_URL` set to a throwaway database.

use anyhow::{Context, Result};
use just_ingredients::bot::cards::card_html;
use just_ingredients::db::{
    NewIngredient, RecipePart, init_database_schema, save_recipe_parts, set_recipe_note,
    set_unit_view,
};
use just_ingredients::localization::create_localization_manager;
use just_ingredients::unit_view::UnitView;
use sqlx::PgPool;

const OWNER: i64 = 4242;

const VIEWS: [(UnitView, &str); 4] = [
    (UnitView::Recipe, "recipe"),
    (UnitView::Weight, "weight"),
    (UnitView::French, "french"),
    (UnitView::Us, "us"),
];

fn ingredient<'a>(
    quantity: Option<f64>,
    unit: Option<&'a str>,
    name: &'a str,
) -> NewIngredient<'a> {
    NewIngredient {
        name,
        quantity,
        unit,
    }
}

/// Compares a card with its golden file, or writes the file with `JI_UPDATE_GOLDEN=1`.
fn check(name: &str, card: &str) -> Result<()> {
    let path = format!(
        "{}/tests/golden/cards/{name}.html",
        env!("CARGO_MANIFEST_DIR")
    );
    if std::env::var_os("JI_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, card).with_context(|| format!("writing {path}"))?;
        return Ok(());
    }
    let expected = std::fs::read_to_string(&path)
        .with_context(|| format!("{path} is missing: run with JI_UPDATE_GOLDEN=1"))?;
    assert_eq!(card, expected, "the card {name} changed");
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn recipe_cards_keep_their_text(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let localization = create_localization_manager()?;

    // A French recipe in two parts: weighable, liquid, spoon, count and quantity-less lines
    let french = save_recipe_parts(
        &pool,
        OWNER,
        Some("fr"),
        "Tarte aux poireaux\n2 tasses de farine\n25 cl de lait",
        &[
            RecipePart {
                name: "Tarte aux poireaux".to_string(),
                ingredients: vec![
                    ingredient(Some(2.0), Some("cup"), "farine"),
                    ingredient(Some(250.0), Some("ml"), "lait"),
                    ingredient(Some(100.0), Some("g"), "beurre <doux> & salé"),
                    ingredient(Some(1.0), Some("tbsp"), "huile d'olive"),
                    ingredient(Some(3.0), None, "poireaux"),
                    ingredient(None, None, "sel"),
                ],
            },
            RecipePart {
                name: "sauce".to_string(),
                ingredients: vec![ingredient(Some(20.0), Some("cl"), "crème")],
            },
        ],
    )
    .await?;
    let tart = french.first().context("the tart")?.recipe_id;
    set_recipe_note(
        &pool,
        OWNER,
        tart,
        Some("Four 180 °C, 35 min <chaleur tournante>"),
    )
    .await?;

    // An English recipe, without memo nor link
    let english = save_recipe_parts(
        &pool,
        OWNER,
        Some("en"),
        "Pancakes\n1 1/2 cups flour\n1 cup milk",
        &[RecipePart {
            name: "Pancakes".to_string(),
            ingredients: vec![
                ingredient(Some(1.5), Some("cup"), "flour"),
                ingredient(Some(1.0), Some("cup"), "milk"),
                ingredient(Some(2.0), Some("tbsp"), "sugar"),
                ingredient(Some(1.0), None, "egg"),
            ],
        }],
    )
    .await?;
    let pancakes = english.first().context("the pancakes")?.recipe_id;

    for (view, view_name) in VIEWS {
        set_unit_view(&pool, OWNER, Some("fr"), view).await?;
        for lang in ["fr", "en"] {
            for (recipe, recipe_name) in [(tart, "tart"), (pancakes, "pancakes")] {
                let card = card_html(&pool, OWNER, recipe, Some(lang), &localization)
                    .await?
                    .context("the card")?;
                check(&format!("{recipe_name}_{view_name}_{lang}"), &card)?;
            }
        }
    }

    // Another user's recipe has no card
    assert!(
        card_html(&pool, OWNER + 1, tart, Some("fr"), &localization)
            .await?
            .is_none()
    );
    Ok(())
}
