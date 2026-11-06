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
    let list = recipes::list(&pool, Owner::telegram(ALICE)).await?;
    assert_eq!(list.total, 3);
    let listed = list.recipes;
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
        recipes::list(&pool, Owner::telegram(999))
            .await?
            .recipes
            .is_empty(),
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

/// Tokens on a real database: created for the owner only, found by the secret only while
/// active, refused across realms, five at most, revocable by their owner only.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn tokens_speak_for_their_owner_until_revoked(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::tokens::{self, Realm};
    init_database_schema(&pool).await?;
    let alice = Owner::telegram(ALICE);
    let bob = Owner::telegram(BOB);

    let token = tokens::create(&pool, alice, Realm::Production).await?;
    assert!(token.secret.starts_with("ji_") && token.secret.len() == 46);
    assert!(token.secret.starts_with(&token.hint));
    let found = tokens::authenticate(&pool, Realm::Production, &token.secret)
        .await?
        .context("Alice's token")?;
    assert_eq!(found.owner, alice);
    assert!(found.allows(tokens::SCOPE_RECIPES_READ));
    assert!(!found.allows("recipes:write"));

    // Only the exact secret, only in its realm
    let mut altered = token.secret.clone();
    altered.pop();
    altered.push(if token.secret.ends_with('A') {
        'B'
    } else {
        'A'
    });
    assert!(
        tokens::authenticate(&pool, Realm::Production, &altered)
            .await?
            .is_none()
    );
    assert!(
        tokens::authenticate(&pool, Realm::Staging, &token.secret)
            .await?
            .is_none()
    );
    let staging = tokens::create(&pool, alice, Realm::Staging).await?;
    assert!(staging.secret.starts_with("ji_stg_"));
    assert!(
        tokens::authenticate(&pool, Realm::Production, &staging.secret)
            .await?
            .is_none()
    );
    assert!(
        tokens::authenticate(&pool, Realm::Staging, &staging.secret)
            .await?
            .is_some()
    );

    // Listed by hint, its use noted
    let listed = tokens::list(&pool, alice).await?;
    assert_eq!(listed.len(), 2);
    assert!(
        listed.iter().all(|key| key.last_used_at.is_some()),
        "both were used"
    );
    assert!(tokens::list(&pool, bob).await?.is_empty());

    // Bob cannot revoke Alice's; Alice can, once
    let id = found.token_id;
    assert!(matches!(
        tokens::revoke(&pool, bob, id).await,
        Err(EngineError::NotFound)
    ));
    assert!(
        tokens::authenticate(&pool, Realm::Production, &token.secret)
            .await?
            .is_some()
    );
    tokens::revoke(&pool, alice, id).await?;
    assert!(
        tokens::authenticate(&pool, Realm::Production, &token.secret)
            .await?
            .is_none()
    );
    assert!(matches!(
        tokens::revoke(&pool, alice, id).await,
        Err(EngineError::NotFound)
    ));

    // Five active at most; a revoked one frees a place
    for _ in 0..4 {
        tokens::create(&pool, alice, Realm::Production).await?;
    }
    assert_eq!(tokens::list(&pool, alice).await?.len(), 5);
    assert!(matches!(
        tokens::create(&pool, alice, Realm::Production).await,
        Err(EngineError::LimitReached)
    ));
    let oldest = tokens::list(&pool, alice)
        .await?
        .first()
        .context("a key")?
        .id;
    tokens::revoke(&pool, alice, oldest).await?;
    tokens::create(&pool, alice, Realm::Production).await?;

    // The secret is not stored
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM api_tokens WHERE position($1::bytea in token_hash) > 0 \
         OR hint = $2",
    )
    .bind(token.secret.as_bytes())
    .bind(&token.secret)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored, 0);
    Ok(())
}

/// A token's use is written at most every 5 minutes.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_tokens_use_is_noted_at_most_every_five_minutes(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::tokens::{self, Realm};
    init_database_schema(&pool).await?;
    let token = tokens::create(&pool, Owner::telegram(ALICE), Realm::Production).await?;
    let last_used = |pool: PgPool| async move {
        Ok::<_, anyhow::Error>(
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT last_used_at FROM api_tokens",
            )
            .fetch_one(&pool)
            .await?,
        )
    };
    tokens::authenticate(&pool, Realm::Production, &token.secret).await?;
    let first = last_used(pool.clone()).await?.context("noted")?;
    tokens::authenticate(&pool, Realm::Production, &token.secret).await?;
    assert_eq!(
        last_used(pool.clone()).await?,
        Some(first),
        "not written again"
    );
    sqlx::query("UPDATE api_tokens SET last_used_at = now() - interval '6 minutes'")
        .execute(&pool)
        .await?;
    tokens::authenticate(&pool, Realm::Production, &token.secret).await?;
    let later = last_used(pool.clone()).await?.context("noted")?;
    assert!(
        later > first - chrono::Duration::minutes(1),
        "written again after 5 minutes"
    );
    Ok(())
}
