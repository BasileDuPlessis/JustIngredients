//! The engine's reads on a real database, two users everywhere (`specs/tasks/engine-api.md`):
//! what every door (Telegram, MCP) gets.
//!
//! Like `db_tests`, they need Postgres: `cargo test --test engine_tests -- --include-ignored`
//! with `DATABASE_URL` set to a throwaway database.

use anyhow::{Context, Result};
use just_ingredients::db::{
    NewIngredient, RecipePart, create_recipe, get_or_create_user, init_database_schema,
    save_recipe, save_recipe_parts, set_recipe_note, set_unit_view,
};
use just_ingredients::engine::recipes::{self, RecipeEntry};
use just_ingredients::engine::{EngineError, Lang, Owner};
use just_ingredients::unit_view::UnitView;
use sqlx::PgPool;

const ALICE: i64 = 501;
const BOB: i64 = 502;

fn names(entries: &[RecipeEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

/// Alice: a tart in two parts (with a memo), crêpes, an unnamed recipe; Bob: a leek soup.
/// Returns the tart's id and the soup's.
async fn two_users(pool: &PgPool) -> Result<(i64, i64)> {
    init_database_schema(pool).await?;
    let tart = save_recipe_parts(
        pool,
        ALICE,
        Some("fr"),
        "Tarte aux poireaux",
        &[
            RecipePart {
                name: "Tarte aux poireaux".to_string(),
                ingredients: vec![
                    NewIngredient {
                        name: "farine",
                        quantity: Some(2.0),
                        unit: Some("cup"),
                    },
                    NewIngredient {
                        name: "poireaux",
                        quantity: Some(3.0),
                        unit: None,
                    },
                ],
            },
            RecipePart {
                name: "sauce".to_string(),
                ingredients: Vec::new(),
            },
        ],
    )
    .await?
    .first()
    .context("the tart")?
    .recipe_id;
    set_recipe_note(pool, ALICE, tart, Some("Four 180 °C")).await?;
    save_recipe(pool, ALICE, Some("fr"), "", "crêpes", &[]).await?;
    create_recipe(pool, ALICE, "a recipe saved without a name").await?;
    let soup = save_recipe(
        pool,
        BOB,
        Some("fr"),
        "",
        "Soupe aux poireaux",
        &[NewIngredient {
            name: "poireaux",
            quantity: Some(2.0),
            unit: None,
        }],
    )
    .await?
    .recipe_id;
    Ok((tart, soup))
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_list_is_the_owners_named_recipes_alphabetical_with_memos(pool: PgPool) -> Result<()> {
    let (tart, _) = two_users(&pool).await?;
    let listed = recipes::list(&pool, Owner::telegram(ALICE)).await?;
    assert_eq!(
        names(&listed),
        ["crêpes", "sauce", "Tarte aux poireaux"],
        "case and accents ignored; no unnamed recipe; not Bob's"
    );
    let with_memo: Vec<(i64, Option<&str>)> = listed
        .iter()
        .map(|entry| (entry.id, entry.note.as_deref()))
        .filter(|(_, note)| note.is_some())
        .collect();
    assert_eq!(with_memo, [(tart, Some("Four 180 °C"))]);
    assert!(
        recipes::list(&pool, Owner::telegram(999)).await?.is_empty(),
        "a stranger"
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_search_finds_the_owners_recipes_by_words_of_their_language(
    pool: PgPool,
) -> Result<()> {
    two_users(&pool).await?;
    let alice = Owner::telegram(ALICE);
    assert_eq!(
        names(&recipes::search(&pool, alice, "poireaux").await?),
        ["Tarte aux poireaux"],
        "by name or ingredient, not Bob's soup"
    );
    assert_eq!(
        names(&recipes::search(&pool, alice, "CREPE").await?),
        ["crêpes"]
    );
    assert!(
        recipes::search(&pool, alice, "leeks").await?.is_empty(),
        "no translation"
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_recipe_is_read_in_a_unit_view_by_its_owner_only(pool: PgPool) -> Result<()> {
    let (tart, soup) = two_users(&pool).await?;
    let alice = Owner::telegram(ALICE);

    let view = recipes::recipe(&pool, alice, tart, Some(UnitView::Weight), Lang::Fr).await?;
    assert_eq!(view.name.as_deref(), Some("Tarte aux poireaux"));
    assert_eq!(view.note.as_deref(), Some("Four 180 °C"));
    assert_eq!(view.unit_view, UnitView::Weight);
    assert_eq!(view.ingredients[0].shown.text, "240 g");
    assert_eq!(view.ingredients[0].stored.unit.as_deref(), Some("cup"));
    assert_eq!(names(&view.linked), ["sauce"]);

    // Without a view: the owner's own
    set_unit_view(&pool, ALICE, Some("fr"), UnitView::Us).await?;
    let own = recipes::recipe(&pool, alice, tart, None, Lang::En).await?;
    assert_eq!(own.unit_view, UnitView::Us);
    assert_eq!(own.ingredients[0].shown.text, "2 cups");

    // Bob's recipe and a missing one are not found, alike
    for id in [soup, tart + 1000] {
        assert!(
            matches!(
                recipes::recipe(&pool, alice, id, None, Lang::Fr).await,
                Err(EngineError::NotFound)
            ),
            "{id}"
        );
    }
    Ok(())
}

/// The engine writes texts in the user's language: the visit keeps it as Telegram gives it.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_visit_keeps_the_users_language_current(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{get_user_by_telegram_id, record_user_seen};
    init_database_schema(&pool).await?;
    get_or_create_user(&pool, ALICE, Some("fr")).await?;
    let language = |pool: PgPool| async move {
        Ok::<_, anyhow::Error>(
            get_user_by_telegram_id(&pool, ALICE)
                .await?
                .context("Alice")?
                .language_code,
        )
    };
    record_user_seen(&pool, ALICE, Some("en")).await?;
    assert_eq!(language(pool.clone()).await?, "en", "changed in Telegram");
    record_user_seen(&pool, ALICE, None).await?;
    assert_eq!(
        language(pool.clone()).await?,
        "en",
        "kept when Telegram gives none"
    );
    record_user_seen(&pool, BOB, None).await?;
    let bob = get_user_by_telegram_id(&pool, BOB).await?.context("Bob")?;
    assert_eq!(bob.language_code, "en", "a new user without a language");
    Ok(())
}
