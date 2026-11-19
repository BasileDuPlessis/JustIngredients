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
    let Some(id) = found.credential.key_id() else {
        anyhow::bail!("a key");
    };
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

/// The events of a user's recipes after `cursor`, as (kind, recipe id, origin).
async fn events_since(pool: &PgPool, cursor: i64) -> Result<Vec<(String, Option<i64>, String)>> {
    let (events, _) = just_ingredients::engine::events::after(pool, cursor).await?;
    Ok(events
        .into_iter()
        .map(|event| {
            (
                event.kind.as_str().to_string(),
                event.recipe_id,
                event.origin.as_str().to_string(),
            )
        })
        .collect())
}

/// Every write leaves its event in the same transaction, in order, with its door; a refused
/// write leaves none.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn every_write_leaves_its_event(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{IngredientChanges, NewIngredient, RecipePart};
    use just_ingredients::engine::events::Origin;
    use just_ingredients::engine::writes::{self, Actor};
    init_database_schema(&pool).await?;
    let alice = Owner::telegram(ALICE);
    let telegram = Actor::telegram(alice);
    let assistant = Actor {
        owner: alice,
        origin: Origin::Mcp,
        credential: Some(just_ingredients::engine::tokens::Credential::Key(7)),
    };

    let saved = writes::create(
        &pool,
        telegram,
        Some("fr"),
        "",
        &[
            RecipePart {
                name: "Tarte".to_string(),
                ingredients: vec![NewIngredient {
                    name: "farine",
                    quantity: Some(200.0),
                    unit: Some("g"),
                }],
            },
            RecipePart {
                name: "Tarte (sauce)".to_string(),
                ingredients: Vec::new(),
            },
        ],
    )
    .await?;
    let tart = saved.first().context("the tart")?.recipe_id;
    let sauce = saved.get(1).context("the sauce")?.recipe_id;
    assert_eq!(
        writes::rename(&pool, assistant, tart, "Tarte fine").await?,
        "Tarte fine"
    );
    assert_eq!(
        writes::set_memo(&pool, assistant, tart, Some("  Four 180 °C \n")).await?,
        Some("Four 180 °C".to_string()),
        "cleaned as in the bot"
    );
    writes::edit_ingredients(
        &pool,
        telegram,
        tart,
        Some("fr"),
        IngredientChanges {
            updates: &[],
            additions: &[NewIngredient {
                name: "sel",
                quantity: None,
                unit: None,
            }],
            deletions: &[],
        },
    )
    .await?;
    let deleted = writes::delete(&pool, telegram, sauce).await?;
    assert_eq!(deleted.name.as_deref(), Some("Tarte (sauce)"));
    assert_eq!(deleted.linked, [tart]);

    let t = |kind: &str, id: i64, origin: &str| (kind.to_string(), Some(id), origin.to_string());
    assert_eq!(
        events_since(&pool, 0).await?,
        [
            t("recipe.created", tart, "telegram"),
            t("recipe.created", sauce, "telegram"),
            t("recipe.renamed", tart, "mcp"),
            t("recipe.memo_changed", tart, "mcp"),
            t("recipe.ingredients_changed", tart, "telegram"),
            t("recipe.deleted", sauce, "telegram"),
        ]
    );

    // Refused or not the owner's: nothing written, no event
    let bob = Actor::telegram(Owner::telegram(BOB));
    let before = events_since(&pool, 0).await?.len();
    assert!(matches!(
        writes::rename(&pool, bob, tart, "Mine").await,
        Err(EngineError::NotFound)
    ));
    assert!(matches!(
        writes::rename(&pool, assistant, tart, "   ").await,
        Err(EngineError::Invalid(_))
    ));
    assert!(matches!(
        writes::set_memo(&pool, assistant, tart, Some(&"x".repeat(201))).await,
        Err(EngineError::Invalid(_))
    ));
    assert!(matches!(
        writes::delete(&pool, bob, tart).await,
        Err(EngineError::NotFound)
    ));
    assert_eq!(
        events_since(&pool, 0).await?.len(),
        before,
        "no event for a refused write"
    );
    let name: Option<String> = sqlx::query_scalar("SELECT recipe_name FROM recipes WHERE id = $1")
        .bind(tart)
        .fetch_one(&pool)
        .await?;
    assert_eq!(name.as_deref(), Some("Tarte fine"));
    Ok(())
}

/// A consumer starts at the last event (not the past), reads in order after its cursor, and
/// keeps its place.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_consumer_reads_after_its_cursor(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::events;
    use just_ingredients::engine::writes::{self, Actor};
    init_database_schema(&pool).await?;
    let actor = Actor::telegram(Owner::telegram(ALICE));
    let first = save_recipe(&pool, ALICE, Some("fr"), "", "Gratin", &[]).await?;
    writes::rename(&pool, actor, first.recipe_id, "Gratin dauphinois").await?;

    let start = events::cursor(&pool, "test").await?;
    assert!(
        events::after(&pool, start).await?.0.is_empty(),
        "the past is not read"
    );
    writes::set_memo(&pool, actor, first.recipe_id, Some("Four 200 °C")).await?;
    let (read, last) = events::after(&pool, start).await?;
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].kind, events::EventKind::MemoChanged);
    assert_eq!(read[0].owner, Owner::telegram(ALICE));
    events::advance(&pool, "test", last).await?;
    assert_eq!(
        events::cursor(&pool, "test").await?,
        last,
        "kept across restarts"
    );
    assert!(events::after(&pool, last).await?.0.is_empty());
    Ok(())
}

/// A consumer that started before any event reads the first ones (2026-11-06 review: a cursor
/// at 0 was taken for a new consumer and moved past them).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_consumer_started_before_any_event_reads_the_first(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::events;
    use just_ingredients::engine::writes::{self, Actor};
    init_database_schema(&pool).await?;
    let gratin = save_recipe(&pool, ALICE, Some("fr"), "", "Gratin", &[]).await?;
    assert_eq!(events::cursor(&pool, "fresh").await?, 0, "no event yet");
    writes::rename(
        &pool,
        Actor::telegram(Owner::telegram(ALICE)),
        gratin.recipe_id,
        "Gratin dauphinois",
    )
    .await?;
    let start = events::cursor(&pool, "fresh").await?;
    assert_eq!(start, 0, "its place is kept, not moved past the new event");
    let (read, _) = events::after(&pool, start).await?;
    assert_eq!(read.len(), 1);
    Ok(())
}

/// The bot saving a photo and an assistant renaming, both for the same user at the same time,
/// do not deadlock (2026-11-06 review: locks were taken in opposite orders).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn concurrent_writes_of_one_user_do_not_deadlock(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{NewIngredient, RecipePart};
    use just_ingredients::engine::events::Origin;
    use just_ingredients::engine::writes::{self, Actor};
    init_database_schema(&pool).await?;
    let alice = Owner::telegram(ALICE);
    let existing = save_recipe(&pool, ALICE, Some("fr"), "", "Gratin", &[]).await?;
    let assistant = Actor {
        owner: alice,
        origin: Origin::Mcp,
        credential: Some(just_ingredients::engine::tokens::Credential::Key(1)),
    };
    let mut tasks = Vec::new();
    for round in 0..20 {
        let pool_a = pool.clone();
        tasks.push(tokio::spawn(async move {
            writes::create(
                &pool_a,
                Actor::telegram(alice),
                Some("fr"),
                "",
                &[RecipePart {
                    name: format!("Tarte {round}"),
                    ingredients: vec![NewIngredient {
                        name: "farine",
                        quantity: Some(200.0),
                        unit: Some("g"),
                    }],
                }],
            )
            .await
            .map(|_| ())
        }));
        let pool_b = pool.clone();
        let id = existing.recipe_id;
        tasks.push(tokio::spawn(async move {
            writes::rename(&pool_b, assistant, id, &format!("Gratin {round}"))
                .await
                .map(|_| ())
        }));
    }
    for task in tasks {
        task.await?
            .map_err(|error| anyhow::anyhow!("a write failed: {:#}", anyhow::Error::from(error)))?;
    }
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM events")
        .fetch_one(&pool)
        .await?;
    assert_eq!(events, 40, "every write and its event");
    Ok(())
}

// OAuth grants (`specs/tasks/oauth.md`): the sign-in's state, our codes, the tokens' rotation

mod oauth {
    use std::sync::atomic::{AtomicU64, Ordering};

    use anyhow::{Context, Result, bail};
    use just_ingredients::db::init_database_schema;
    use just_ingredients::engine::oauth::{
        self, Callback, CodeExchange, Decision, GrantError, Issued, SignInRequest, SignInSecrets,
    };
    use just_ingredients::engine::tokens::{self, Checked, Credential, Realm};
    use just_ingredients::engine::{EngineError, Owner};
    use just_ingredients::secret::SecretString;
    use sqlx::PgPool;

    use super::{ALICE, BOB};

    const CLAUDE: &str = "https://claude.ai/oauth/mcp-oauth-client-metadata";
    const CHATGPT: &str = "https://chatgpt.com/oauth/client.json";
    const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
    const RESOURCE: &str = "https://just-ingredients.fly.dev/mcp";
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

    /// A different state and binding for each sign-in of a test.
    static SIGN_INS: AtomicU64 = AtomicU64::new(0);

    fn request(client: &str) -> SignInRequest {
        SignInRequest {
            client_id: client.to_string(),
            redirect_uri: CALLBACK.to_string(),
            client_state: Some("client-state".to_string()),
            code_challenge: oauth::pkce_challenge(VERIFIER),
            scopes: vec![tokens::SCOPE_RECIPES_READ.to_string()],
            resource: RESOURCE.to_string(),
        }
    }

    /// A whole sign-in of `telegram_id` for `client`, up to our code.
    async fn code_for(
        pool: &PgPool,
        telegram_id: i64,
        client: &str,
        write: bool,
    ) -> Result<String> {
        let state = format!("state-{}", SIGN_INS.fetch_add(1, Ordering::Relaxed));
        let binding = format!("binding-{state}");
        let verifier = SecretString::new("telegram-verifier");
        oauth::start_sign_in(
            pool,
            &request(client),
            &SignInSecrets {
                state: &state,
                binding: &binding,
                verifier: &verifier,
                nonce: "nonce",
            },
        )
        .await?;
        let Callback::Pending(pending) = oauth::find_sign_in(pool, &state, Some(&binding)).await?
        else {
            bail!("the sign-in waits");
        };
        assert!(
            oauth::signed_in(
                pool,
                pending.id,
                Owner::telegram(telegram_id),
                Some("Alice")
            )
            .await?
        );
        let consent = oauth::open_consent(pool, &binding, "form")
            .await?
            .context("consent")?;
        assert_eq!(consent.owner, Owner::telegram(telegram_id));
        assert_eq!(
            consent.telegram_name.as_deref(),
            Some("Alice"),
            "for the page"
        );
        let decided = oauth::decide(pool, &binding, "form", Decision::Allow { write })
            .await?
            .context("decided")?;
        Ok(decided.code.context("a code")?.expose().to_string())
    }

    fn exchange(code: &str) -> CodeExchange<'_> {
        CodeExchange {
            code,
            client_id: CLAUDE,
            redirect_uri: CALLBACK,
            verifier: VERIFIER,
            resource: Some(RESOURCE),
        }
    }

    async fn grant(pool: &PgPool, telegram_id: i64, write: bool) -> Result<Issued> {
        let code = code_for(pool, telegram_id, CLAUDE, write).await?;
        Ok(oauth::redeem_code(pool, Realm::Production, exchange(&code)).await?)
    }

    async fn check(pool: &PgPool, token: &SecretString) -> Result<Checked> {
        Ok(tokens::check(pool, Realm::Production, token.expose(), Some(RESOURCE)).await?)
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_sign_in_becomes_a_grant_its_tokens_open_the_door(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let (state, binding) = ("the-state", "the-binding");
        let verifier = SecretString::new("v");
        let started = oauth::start_sign_in(
            &pool,
            &request(CLAUDE),
            &SignInSecrets {
                state,
                binding,
                verifier: &verifier,
                nonce: "n",
            },
        )
        .await?;

        // Telegram's answer: in the browser that started, never another
        assert!(matches!(
            oauth::find_sign_in(&pool, "made-up", Some(binding)).await?,
            Callback::Unknown
        ));
        for other in [None, Some("another-browser")] {
            assert!(matches!(
                oauth::find_sign_in(&pool, state, other).await?,
                Callback::OtherBrowser
            ));
        }
        let Callback::Pending(pending) = oauth::find_sign_in(&pool, state, Some(binding)).await?
        else {
            bail!("pending");
        };
        assert_eq!(pending.id, started);
        assert_eq!(pending.verifier.expose(), "v");
        assert_eq!(pending.request, request(CLAUDE));
        assert!(
            oauth::open_consent(&pool, binding, "early")
                .await?
                .is_none(),
            "nobody signed in yet"
        );
        assert!(oauth::signed_in(&pool, started, Owner::telegram(ALICE), None).await?);
        assert!(
            !oauth::signed_in(&pool, started, Owner::telegram(BOB), None).await?,
            "once"
        );
        assert!(
            matches!(
                oauth::find_sign_in(&pool, state, Some(binding)).await?,
                Callback::SignedIn
            ),
            "Telegram's answer counts once; reloaded, it leads to the consent page"
        );

        // Consent: the current form only, decided once
        assert!(
            oauth::open_consent(&pool, "another-browser", "f")
                .await?
                .is_none()
        );
        oauth::open_consent(&pool, binding, "first")
            .await?
            .context("consent")?;
        oauth::open_consent(&pool, binding, "second")
            .await?
            .context("reloaded")?;
        assert!(
            oauth::decide(&pool, binding, "first", Decision::Allow { write: true })
                .await?
                .is_none(),
            "an older form"
        );
        let decided = oauth::decide(&pool, binding, "second", Decision::Allow { write: false })
            .await?
            .context("decided")?;
        assert_eq!(
            decided.request.client_state.as_deref(),
            Some("client-state")
        );
        let code = decided.code.context("a code")?;
        assert!(
            oauth::decide(&pool, binding, "second", Decision::Deny)
                .await?
                .is_none(),
            "decided once"
        );

        // The code for a grant and its first tokens
        let issued = oauth::redeem_code(&pool, Realm::Production, exchange(code.expose())).await?;
        assert!(issued.access_token.expose().starts_with("jio_"));
        assert!(issued.refresh_token.expose().starts_with("jir_"));
        assert_eq!(issued.expires_in, 3600);
        assert_eq!(issued.scopes, [tokens::SCOPE_RECIPES_READ]);
        assert_eq!(issued.owner, Owner::telegram(ALICE));
        assert_eq!(issued.new_grant.map(|grant| grant.replaced), Some(false));

        let Checked::Valid(authenticated) = check(&pool, &issued.access_token).await? else {
            bail!("the access token opens the door");
        };
        assert_eq!(authenticated.owner, Owner::telegram(ALICE));
        assert_eq!(authenticated.credential, Credential::Grant(issued.grant_id));
        assert_eq!(authenticated.client.as_deref(), Some(CLAUDE));
        assert!(
            !authenticated.allows(tokens::SCOPE_RECIPES_WRITE),
            "writing not ticked"
        );
        let access = issued.access_token.expose();
        assert_eq!(
            tokens::check(
                &pool,
                Realm::Production,
                access,
                Some("https://other.example/mcp")
            )
            .await?,
            Checked::Known,
            "for another resource"
        );
        assert_eq!(
            tokens::check(&pool, Realm::Production, access, None).await?,
            Checked::Unknown,
            "a door without OAuth"
        );
        assert_eq!(
            tokens::check(&pool, Realm::Staging, access, Some(RESOURCE)).await?,
            Checked::Unknown,
            "a production token on staging"
        );
        assert_eq!(
            check(&pool, &issued.refresh_token).await?,
            Checked::Unknown,
            "a refresh token does not open the door"
        );

        // Listed for Alice only, revoked by her only
        let listed = oauth::list_grants(&pool, Owner::telegram(ALICE)).await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].client_id, CLAUDE);
        assert!(!listed[0].can_write);
        assert!(
            oauth::list_grants(&pool, Owner::telegram(BOB))
                .await?
                .is_empty()
        );
        assert!(matches!(
            oauth::revoke_grant(&pool, Owner::telegram(BOB), issued.grant_id).await,
            Err(EngineError::NotFound)
        ));
        oauth::revoke_grant(&pool, Owner::telegram(ALICE), issued.grant_id).await?;
        assert_eq!(check(&pool, &issued.access_token).await?, Checked::Known);
        assert!(
            oauth::list_grants(&pool, Owner::telegram(ALICE))
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_code_is_used_once_and_its_reuse_revokes_the_grant(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let code = code_for(&pool, ALICE, CLAUDE, true).await?;
        let issued = oauth::redeem_code(&pool, Realm::Production, exchange(&code)).await?;
        assert_eq!(
            issued.scopes,
            [tokens::SCOPE_RECIPES_READ, tokens::SCOPE_RECIPES_WRITE]
        );
        assert!(matches!(
            oauth::redeem_code(&pool, Realm::Production, exchange(&code)).await,
            Err(GrantError::Reused { grant_id }) if grant_id == issued.grant_id
        ));
        assert_eq!(check(&pool, &issued.access_token).await?, Checked::Known);
        assert!(matches!(
            oauth::refresh(
                &pool,
                Realm::Production,
                issued.refresh_token.expose(),
                CLAUDE,
                |_| true
            )
            .await,
            Err(GrantError::Refused(_))
        ));

        // Each mismatch refuses, and burns the code
        let wrong_verifier = "x".repeat(43);
        let mismatches = [
            (
                "client",
                CodeExchange {
                    client_id: CHATGPT,
                    ..exchange("")
                },
            ),
            (
                "redirect",
                CodeExchange {
                    redirect_uri: "https://claude.ai/other",
                    ..exchange("")
                },
            ),
            (
                "verifier",
                CodeExchange {
                    verifier: &wrong_verifier,
                    ..exchange("")
                },
            ),
            (
                "short verifier",
                CodeExchange {
                    verifier: "short",
                    ..exchange("")
                },
            ),
            (
                "resource",
                CodeExchange {
                    resource: Some("https://other.example/mcp"),
                    ..exchange("")
                },
            ),
        ];
        for (what, mismatch) in mismatches {
            let code = code_for(&pool, BOB, CLAUDE, false).await?;
            let presented = CodeExchange {
                code: &code,
                ..mismatch
            };
            assert!(
                matches!(
                    oauth::redeem_code(&pool, Realm::Production, presented).await,
                    Err(GrantError::Refused(_))
                ),
                "{what}"
            );
            assert!(
                oauth::redeem_code(&pool, Realm::Production, exchange(&code))
                    .await
                    .is_err(),
                "{what}: burnt"
            );
        }
        let code = code_for(&pool, BOB, CLAUDE, false).await?;
        let without_resource = CodeExchange {
            resource: None,
            ..exchange(&code)
        };
        oauth::redeem_code(&pool, Realm::Production, without_resource)
            .await
            .context("no resource sent: the code's")?;

        // An expired code
        let code = code_for(&pool, BOB, CLAUDE, false).await?;
        sqlx::query(
            "UPDATE oauth_codes SET expires_at = now() - interval '1 second' \
             WHERE used_at IS NULL",
        )
        .execute(&pool)
        .await?;
        assert!(matches!(
            oauth::redeem_code(&pool, Realm::Production, exchange(&code)).await,
            Err(GrantError::Refused("unknown or expired code"))
        ));
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_refresh_token_rotates_with_a_grace_then_its_reuse_revokes(
        pool: PgPool,
    ) -> Result<()> {
        init_database_schema(&pool).await?;
        let first = grant(&pool, ALICE, false).await?;
        let refresh = |token: SecretString| {
            let pool = pool.clone();
            async move {
                oauth::refresh(&pool, Realm::Production, token.expose(), CLAUDE, |_| true).await
            }
        };

        let second = refresh(first.refresh_token.clone()).await?;
        assert_eq!(second.grant_id, first.grant_id);
        assert!(second.new_grant.is_none());
        assert_ne!(second.refresh_token.expose(), first.refresh_token.expose());
        assert!(matches!(
            check(&pool, &second.access_token).await?,
            Checked::Valid(_)
        ));
        assert!(
            matches!(check(&pool, &first.access_token).await?, Checked::Valid(_)),
            "the previous access token lives its hour"
        );

        // A retry within the grace period: a fresh pair, both chains work
        let retried = refresh(first.refresh_token.clone()).await?;
        assert_ne!(
            retried.refresh_token.expose(),
            second.refresh_token.expose()
        );
        let third = refresh(second.refresh_token.clone()).await?;
        assert!(matches!(
            check(&pool, &third.access_token).await?,
            Checked::Valid(_)
        ));

        // Later, the reuse of a rotated token revokes the grant
        sqlx::query(
            "UPDATE oauth_tokens SET rotated_at = now() - interval '2 minutes' \
             WHERE rotated_at IS NOT NULL",
        )
        .execute(&pool)
        .await?;
        assert!(matches!(
            refresh(first.refresh_token.clone()).await,
            Err(GrantError::Reused { grant_id }) if grant_id == first.grant_id
        ));
        assert_eq!(check(&pool, &third.access_token).await?, Checked::Known);
        assert!(matches!(
            refresh(third.refresh_token.clone()).await,
            Err(GrantError::Refused(_))
        ));
        assert!(
            oauth::list_grants(&pool, Owner::telegram(ALICE))
                .await?
                .is_empty()
        );

        // Another client, a made-up token, an access token, a user no longer allowed
        let other = grant(&pool, BOB, false).await?;
        for (presented, client) in [
            (other.refresh_token.expose().to_string(), CHATGPT),
            (format!("jir_{}", "A".repeat(43)), CLAUDE),
            (other.access_token.expose().to_string(), CLAUDE),
        ] {
            assert!(matches!(
                oauth::refresh(&pool, Realm::Production, &presented, client, |_| true).await,
                Err(GrantError::Refused(_))
            ));
        }
        let bob = Owner::telegram(BOB);
        assert!(matches!(
            oauth::refresh(
                &pool,
                Realm::Production,
                other.refresh_token.expose(),
                CLAUDE,
                |owner| owner != bob
            )
            .await,
            Err(GrantError::Refused(_))
        ));
        assert_eq!(
            check(&pool, &other.access_token).await?,
            Checked::Known,
            "revoked"
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_new_grant_replaces_the_previous_one_of_the_client(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let first = grant(&pool, ALICE, false).await?;
        let second = grant(&pool, ALICE, true).await?;
        assert_eq!(second.new_grant.map(|grant| grant.replaced), Some(true));
        assert_eq!(check(&pool, &first.access_token).await?, Checked::Known);
        let listed = oauth::list_grants(&pool, Owner::telegram(ALICE)).await?;
        assert_eq!(listed.len(), 1);
        assert!(listed[0].can_write);

        // Another client is another grant
        let code = code_for(&pool, ALICE, CHATGPT, false).await?;
        let issued = oauth::redeem_code(
            &pool,
            Realm::Production,
            CodeExchange {
                client_id: CHATGPT,
                ..exchange(&code)
            },
        )
        .await?;
        assert_eq!(issued.new_grant.map(|grant| grant.replaced), Some(false));
        assert_eq!(
            oauth::list_grants(&pool, Owner::telegram(ALICE))
                .await?
                .len(),
            2
        );

        // Two codes of one user and client redeemed at once: one grant stays
        let one = code_for(&pool, BOB, CLAUDE, false).await?;
        let two = code_for(&pool, BOB, CLAUDE, false).await?;
        let (a, b) = tokio::join!(
            oauth::redeem_code(&pool, Realm::Production, exchange(&one)),
            oauth::redeem_code(&pool, Realm::Production, exchange(&two)),
        );
        assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
        assert_eq!(
            oauth::list_grants(&pool, Owner::telegram(BOB)).await?.len(),
            1
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_change_through_a_grant_names_it_in_its_event(pool: PgPool) -> Result<()> {
        use just_ingredients::engine::events::{self, Origin};
        use just_ingredients::engine::writes::{self, Actor};
        init_database_schema(&pool).await?;
        let issued = grant(&pool, ALICE, true).await?;
        let Checked::Valid(authenticated) = check(&pool, &issued.access_token).await? else {
            bail!("valid");
        };
        let recipe = writes::create_from_lines(
            &pool,
            Actor::token(&authenticated, Origin::Mcp),
            "Crêpes",
            &["250 g de farine".to_string()],
        )
        .await?;
        let row: (Option<i64>, Option<i64>) =
            sqlx::query_as("SELECT token_id, grant_id FROM events WHERE recipe_id = $1")
                .bind(recipe.recipe_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(row, (None, Some(issued.grant_id)));
        let (read, _) = events::after(&pool, 0).await?;
        assert_eq!(
            read.last().and_then(|event| event.credential),
            Some(Credential::Grant(issued.grant_id))
        );

        // The grant itself was announced, written with it
        let connected = read
            .iter()
            .find(|event| event.kind == events::EventKind::AssistantConnected)
            .context("the oauth.connected event")?;
        assert_eq!(connected.owner, Owner::telegram(ALICE));
        assert_eq!(
            connected.credential,
            Some(Credential::Grant(issued.grant_id))
        );
        assert_eq!(connected.recipe_id, None);
        assert_eq!(connected.payload["client_id"], CLAUDE);
        assert_eq!(connected.payload["write"], true);
        assert_eq!(connected.payload["replaced"], false);
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn what_can_no_longer_be_used_is_purged(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let issued = grant(&pool, ALICE, false).await?;
        assert_eq!(oauth::purge(&pool).await?, 0, "everything is recent");
        for table in ["oauth_requests", "oauth_codes", "oauth_tokens"] {
            sqlx::query(&format!(
                "UPDATE {table} SET expires_at = now() - interval '2 days'"
            ))
            .execute(&pool)
            .await?;
        }
        assert_eq!(
            oauth::purge(&pool).await?,
            4,
            "a sign-in, a code, two tokens"
        );
        assert_eq!(check(&pool, &issued.access_token).await?, Checked::Unknown);
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn renewals_are_bounded_retries_chains_and_rate(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let first = grant(&pool, ALICE, false).await?;
        let refresh = |token: SecretString| {
            let pool = pool.clone();
            async move {
                oauth::refresh(&pool, Realm::Production, token.expose(), CLAUDE, |_| true).await
            }
        };
        let live = |pool: PgPool, grant_id: i64| async move {
            Ok::<i64, anyhow::Error>(
                sqlx::query_scalar(
                    "SELECT count(*) FROM oauth_tokens WHERE grant_id = $1 AND kind = 'refresh' \
                     AND rotated_at IS NULL AND revoked_at IS NULL",
                )
                .bind(grant_id)
                .fetch_one(&pool)
                .await?,
            )
        };

        // One retry within the grace period, not two; the grant is kept
        let second = refresh(first.refresh_token.clone()).await?;
        let retried = refresh(first.refresh_token.clone()).await?;
        assert!(matches!(
            refresh(first.refresh_token.clone()).await,
            Err(GrantError::Refused("already renewed twice"))
        ));
        assert!(matches!(
            check(&pool, &second.access_token).await?,
            Checked::Valid(_)
        ));

        // At most three live chains: the oldest goes
        let s2 = refresh(second.refresh_token.clone()).await?;
        let s2b = refresh(second.refresh_token.clone()).await?;
        assert_eq!(live(pool.clone(), first.grant_id).await?, 3);
        let r2 = refresh(retried.refresh_token.clone()).await?;
        assert_eq!(live(pool.clone(), first.grant_id).await?, 3, "s2, s2b, r2");
        refresh(retried.refresh_token.clone()).await?;
        assert_eq!(
            live(pool.clone(), first.grant_id).await?,
            3,
            "a fourth: s2 goes"
        );
        assert!(
            refresh(s2.refresh_token.clone()).await.is_err(),
            "the oldest live token was revoked"
        );
        assert!(refresh(s2b.refresh_token.clone()).await.is_ok());

        // Renewed too often: refused for now, the grant and its tokens kept
        let mut latest = r2.refresh_token.clone();
        let mut too_often = false;
        for _ in 0..12 {
            match refresh(latest.clone()).await {
                Ok(issued) => latest = issued.refresh_token,
                Err(GrantError::TooOften) => {
                    too_often = true;
                    break;
                }
                Err(other) => bail!("{other}"),
            }
        }
        assert!(too_often, "10 renewals in 10 minutes at most");
        assert!(matches!(
            check(&pool, &r2.access_token).await?,
            Checked::Valid(_)
        ));
        assert_eq!(
            oauth::list_grants(&pool, Owner::telegram(ALICE))
                .await?
                .len(),
            1
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn telegrams_answer_is_claimed_once_and_a_late_failure_keeps_the_sign_in(
        pool: PgPool,
    ) -> Result<()> {
        init_database_schema(&pool).await?;
        let verifier = SecretString::new("v");
        oauth::start_sign_in(
            &pool,
            &request(CLAUDE),
            &SignInSecrets {
                state: "s",
                binding: "b",
                verifier: &verifier,
                nonce: "n",
            },
        )
        .await?;
        let Callback::Pending(pending) = oauth::find_sign_in(&pool, "s", Some("b")).await? else {
            bail!("pending");
        };
        assert!(oauth::claim_sign_in(&pool, pending.id).await?);
        assert!(!oauth::claim_sign_in(&pool, pending.id).await?, "once");
        assert_eq!(oauth::sign_in_state(&pool, pending.id).await?, Some(false));
        assert!(oauth::signed_in(&pool, pending.id, Owner::telegram(ALICE), Some("Alice")).await?);
        assert_eq!(oauth::sign_in_state(&pool, pending.id).await?, Some(true));
        assert!(
            oauth::abandon_sign_in(&pool, pending.id).await?.is_none(),
            "a late failure does not end a signed-in sign-in"
        );
        assert!(oauth::open_consent(&pool, "b", "f").await?.is_some());
        Ok(())
    }
}

// Photo readings: the daily count in the database (`specs/tasks/mcp-photos.md`, task 1)

mod photos {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::Result;
    use just_ingredients::db::init_database_schema;
    use just_ingredients::engine::photos::StoredDailyCalls;
    use just_ingredients::extraction::guard::GuardedExtractor;
    use just_ingredients::extraction::{
        ExtractedIngredient, Extraction, ExtractionError, ExtractionFuture, ImageInput,
        IngredientExtractor, RecipeLanguage,
    };
    use sqlx::PgPool;

    use super::{ALICE, BOB};

    /// Reads every photo as salt, counting its calls; fails as asked.
    #[derive(Debug, Default)]
    struct Fake {
        calls: AtomicUsize,
        refuse_before_calling: bool,
    }

    impl IngredientExtractor for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn extract<'a>(&'a self, _image: &'a ImageInput) -> ExtractionFuture<'a> {
            Box::pin(async move {
                if self.refuse_before_calling {
                    return Err(ExtractionError::CircuitOpen);
                }
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(Extraction {
                    is_recipe: true,
                    title: None,
                    language: RecipeLanguage::Fr,
                    transcription: "sel".to_string(),
                    ingredients: vec![ExtractedIngredient {
                        name: "sel".to_string(),
                        quantity: None,
                        unit: None,
                        needs_review: false,
                        source_text: "sel".to_string(),
                        section: None,
                    }],
                })
            })
        }
    }

    fn photo(seed: u8, user: i64) -> Result<ImageInput> {
        Ok(ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, seed])?.requested_by(user))
    }

    fn guarded(pool: &PgPool, fake: Arc<Fake>, limit: u32) -> GuardedExtractor {
        GuardedExtractor::new(fake, 2)
            .with_daily_limit(limit)
            .with_daily_calls(Arc::new(StoredDailyCalls::new(pool.clone())))
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn the_daily_limit_is_kept_in_the_database_across_deploys(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let fake = Arc::new(Fake::default());
        let first = guarded(&pool, Arc::clone(&fake), 2);
        first.extract(&photo(1, ALICE)?).await?;
        first.extract(&photo(2, ALICE)?).await?;
        assert!(matches!(
            first.extract(&photo(3, ALICE)?).await,
            Err(ExtractionError::DailyLimitReached { limit: 2 })
        ));

        // A deploy: a new guard (an empty cache), the same database, the same count
        let after_deploy = guarded(&pool, Arc::clone(&fake), 2);
        assert!(matches!(
            after_deploy.extract(&photo(4, ALICE)?).await,
            Err(ExtractionError::DailyLimitReached { .. })
        ));
        assert_eq!(
            fake.calls.load(Ordering::SeqCst),
            2,
            "nothing paid past the limit"
        );
        assert_eq!(
            just_ingredients::db::photo_calls_today(&pool, ALICE).await?,
            2
        );

        // Bob has his own count; a photo already read is free (the cache of that guard)
        after_deploy.extract(&photo(4, BOB)?).await?;
        after_deploy.extract(&photo(4, BOB)?).await?;
        assert_eq!(
            just_ingredients::db::photo_calls_today(&pool, BOB).await?,
            1
        );

        // A call refused before Gemini (the breaker open) is given back
        let refusing = Arc::new(Fake {
            refuse_before_calling: true,
            ..Fake::default()
        });
        let broken = guarded(&pool, refusing, 2);
        assert!(broken.extract(&photo(9, BOB)?).await.is_err());
        assert_eq!(
            just_ingredients::db::photo_calls_today(&pool, BOB).await?,
            1
        );

        // Yesterday's count is another day's, and past days are purged
        sqlx::query("UPDATE user_photo_days SET day = day - 1 WHERE telegram_id = $1")
            .bind(ALICE)
            .execute(&pool)
            .await?;
        first.extract(&photo(5, ALICE)?).await?;
        sqlx::query("UPDATE user_photo_days SET day = day - 30 WHERE calls = 2")
            .execute(&pool)
            .await?;
        assert_eq!(just_ingredients::engine::photos::purge(&pool).await?, 1);
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn readings_at_once_never_pass_the_limit(pool: PgPool) -> Result<()> {
        init_database_schema(&pool).await?;
        let fake = Arc::new(Fake::default());
        let guard = Arc::new(guarded(&pool, Arc::clone(&fake), 3));
        let mut readings = Vec::new();
        for seed in 0..10_u8 {
            let guard = Arc::clone(&guard);
            readings.push(tokio::spawn(async move {
                let image = photo(seed, ALICE)?;
                anyhow::Ok(guard.extract(&image).await.is_ok())
            }));
        }
        let mut read = 0;
        for reading in readings {
            if reading.await?? {
                read += 1;
            }
        }
        assert_eq!(read, 3);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 3);
        assert_eq!(
            just_ingredients::db::photo_calls_today(&pool, ALICE).await?,
            3
        );
        Ok(())
    }
}

// Photo imports: link, reading, draft, save (`specs/tasks/mcp-photos.md`, task 2)

mod photo_imports {
    use std::sync::{Arc, Mutex, PoisonError};

    use anyhow::{Context, Result, bail};
    use just_ingredients::db::init_database_schema;
    use just_ingredients::engine::photos::{self, ImportState, ReadingFailure, SaveError};
    use just_ingredients::engine::tokens::{self, Access, Credential, Realm};
    use just_ingredients::engine::writes::Actor;
    use just_ingredients::engine::{EngineError, Owner};
    use just_ingredients::extraction::{
        ExtractedIngredient, Extraction, ExtractionError, ExtractionFuture, ImageInput,
        IngredientExtractor, RecipeLanguage, SharedExtractor,
    };
    use sqlx::PgPool;

    use super::{ALICE, BOB};

    /// Answers what it is given, once per call.
    #[derive(Debug)]
    struct Fake {
        answer: Mutex<Result<Extraction, ExtractionError>>,
    }

    impl IngredientExtractor for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn extract<'a>(&'a self, _image: &'a ImageInput) -> ExtractionFuture<'a> {
            let answer = self
                .answer
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            Box::pin(async move { answer })
        }
    }

    fn line(
        name: &str,
        quantity: Option<f64>,
        unit: Option<&str>,
        section: Option<&str>,
    ) -> ExtractedIngredient {
        ExtractedIngredient {
            name: name.to_string(),
            quantity,
            unit: unit.map(str::to_string),
            needs_review: false,
            source_text: format!("{} {name}", quantity.unwrap_or_default()),
            section: section.map(str::to_string),
        }
    }

    /// A tart in two parts, with a title.
    fn tart() -> Extraction {
        Extraction {
            is_recipe: true,
            title: Some("Tarte aux poireaux".to_string()),
            language: RecipeLanguage::Fr,
            transcription: "Pâte\n250 g farine\nGarniture\n3 poireaux".to_string(),
            ingredients: vec![
                line("farine", Some(250.0), Some("g"), Some("Pâte")),
                line("beurre", Some(125.0), Some("g"), Some("Pâte")),
                line("poireaux", Some(3.0), None, Some("Garniture")),
                line("crème", Some(20.0), Some("cl"), Some("Garniture")),
            ],
        }
    }

    fn extractor(answer: Result<Extraction, ExtractionError>) -> (SharedExtractor, Arc<Fake>) {
        let fake = Arc::new(Fake {
            answer: Mutex::new(answer),
        });
        (fake.clone(), fake)
    }

    fn photo() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0x01]
    }

    /// Alice's read-write key, and an import started with it.
    async fn import(pool: &PgPool) -> Result<(Credential, photos::NewImport)> {
        init_database_schema(pool).await?;
        just_ingredients::db::get_or_create_user(pool, ALICE, Some("fr")).await?;
        let key = tokens::create_with(
            pool,
            Owner::telegram(ALICE),
            Realm::Production,
            Access::ReadWrite,
        )
        .await?;
        let credential = Credential::Key(key.id);
        let started = photos::start_import(pool, Owner::telegram(ALICE), credential).await?;
        Ok((credential, started))
    }

    fn actor(credential: Credential) -> Actor {
        Actor {
            owner: Owner::telegram(ALICE),
            origin: just_ingredients::engine::events::Origin::Mcp,
            credential: Some(credential),
        }
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_photo_becomes_a_draft_then_linked_recipes_once(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let secret = started.secret.expose();
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Waiting { last_failure: None })
        );
        assert_eq!(photos::link_state(&pool, "made-up").await?, None);

        // Claimed once: a double tap claims nothing
        let claimed = photos::claim(&pool, secret).await?.context("claimed")?;
        assert_eq!(claimed.id, started.id);
        assert!(photos::claim(&pool, secret).await?.is_none(), "once");
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Reading)
        );

        let (extractor, _) = extractor(Ok(tart()));
        photos::read(&pool, &extractor, claimed, photo()).await;
        let Some(ImportState::Ready {
            title,
            parts,
            lines,
        }) = photos::import_state(&pool, Owner::telegram(ALICE), started.id).await?
        else {
            bail!("a draft");
        };
        assert_eq!(title.as_deref(), Some("Tarte aux poireaux"));
        assert_eq!(
            parts,
            ["Tarte aux poireaux", "Tarte aux poireaux (garniture)"]
        );
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].line, 1);
        assert_eq!(lines[0].quantity.as_deref(), Some("250"));
        assert_eq!(lines[2].part.as_deref(), Some("Garniture"));
        assert!(
            photos::import_state(&pool, Owner::telegram(BOB), started.id)
                .await?
                .is_none(),
            "Alice's only"
        );

        // Saved as two linked recipes, with the text read; saved again: the same recipes
        let saved = photos::save(&pool, actor(credential), started.id, None, &[]).await?;
        assert_eq!(saved.recipes.len(), 2);
        assert_eq!(saved.recipes[0].1, "Tarte aux poireaux");
        let again = photos::save(&pool, actor(credential), started.id, None, &[]).await?;
        assert_eq!(again, saved, "saved once");
        let (content, links): (String, i64) = sqlx::query_as(
            "SELECT r.content, (SELECT count(*) FROM recipe_links l WHERE r.id IN (l.recipe_a, l.recipe_b)) \
             FROM recipes r WHERE r.id = $1",
        )
        .bind(saved.recipes[0].0)
        .fetch_one(&pool)
        .await?;
        assert!(content.contains("250 g farine"), "the text read: {content}");
        assert_eq!(links, 1, "linked to its part");
        let events: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT origin, token_id FROM events WHERE kind = 'recipe.created' ORDER BY id",
        )
        .fetch_all(&pool)
        .await?;
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|(origin, token)| origin == "mcp" && *token == credential.key_id())
        );
        assert!(matches!(
            photos::import_state(&pool, Owner::telegram(ALICE), started.id).await?,
            Some(ImportState::Saved { .. })
        ));
        assert!(
            photos::claim(&pool, secret).await?.is_none(),
            "a saved import takes no photo"
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn the_bots_review_rules_hold_before_saving(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let mut unsure = line("huile", Some(2.0), Some("tbsp"), None);
        unsure.needs_review = true;
        let (extractor, _) = extractor(Ok(Extraction {
            is_recipe: true,
            title: None,
            language: RecipeLanguage::Fr,
            transcription: "farine\n2-3 c. à soupe d'huile".to_string(),
            ingredients: vec![line("farine", Some(200.0), Some("g"), None), unsure],
        }));
        let claimed = photos::claim(&pool, started.secret.expose())
            .await?
            .context("claimed")?;
        photos::read(&pool, &extractor, claimed, photo()).await;

        let save = |name: Option<&'static str>, corrections: Vec<(usize, String)>| {
            let pool = pool.clone();
            async move { photos::save(&pool, actor(credential), started.id, name, &corrections).await }
        };
        match save(None, Vec::new()).await {
            Err(SaveError::ToConfirm(lines)) => assert_eq!(lines, [(2, "huile".to_string())]),
            other => bail!("{other:?}"),
        }
        assert!(
            matches!(
                save(None, vec![(1, "300 g farine".to_string())]).await,
                Err(SaveError::NotFlagged(1))
            ),
            "only flagged lines change"
        );
        assert!(matches!(
            save(None, vec![(9, "1 g sel".to_string())]).await,
            Err(SaveError::NotFlagged(9))
        ));
        assert!(matches!(
            save(None, vec![(2, "0 g huile".to_string())]).await,
            Err(SaveError::InvalidLine { line: 2, .. })
        ));
        let fixed = vec![(2, "2 cuil. à soupe d'huile".to_string())];
        assert!(matches!(
            save(None, fixed.clone()).await,
            Err(SaveError::NeedsName)
        ));
        let saved = save(Some("Vinaigrette"), fixed).await?;
        assert_eq!(saved.recipes.len(), 1);
        let oil: (Option<f64>, Option<String>) = sqlx::query_as(
            "SELECT quantity::float8, unit FROM ingredients WHERE recipe_id = $1 AND name LIKE '%huile%'",
        )
        .bind(saved.recipes[0].0)
        .fetch_one(&pool)
        .await?;
        assert_eq!(oil, (Some(2.0), Some("tbsp".to_string())));
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_link_works_only_while_its_key_may_write(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let Some(key) = credential.key_id() else {
            bail!("a key");
        };
        tokens::revoke(&pool, Owner::telegram(ALICE), key).await?;
        assert!(
            photos::claim(&pool, started.secret.expose())
                .await?
                .is_none(),
            "revoked"
        );
        assert_eq!(
            photos::link_state(&pool, started.secret.expose()).await?,
            Some(ImportState::Expired),
            "the page says so, instead of offering the form again"
        );

        let reader = tokens::create(&pool, Owner::telegram(ALICE), Realm::Production).await?;
        let read_only =
            photos::start_import(&pool, Owner::telegram(ALICE), Credential::Key(reader.id)).await?;
        assert!(
            photos::claim(&pool, read_only.secret.expose())
                .await?
                .is_none(),
            "a read-only key"
        );
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_reading_without_a_draft_leaves_the_link_open_but_the_daily_limit(
        pool: PgPool,
    ) -> Result<()> {
        let (_, started) = import(&pool).await?;
        let secret = started.secret.expose();
        let mut not_a_recipe = tart();
        not_a_recipe.is_recipe = false;
        let (extractor, fake) = extractor(Ok(not_a_recipe));
        let claimed = photos::claim(&pool, secret).await?.context("claimed")?;
        photos::read(&pool, &extractor, claimed, photo()).await;
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Waiting {
                last_failure: Some(ReadingFailure::NotARecipe)
            })
        );

        // Not an image: unreadable, the link still open
        let claimed = photos::claim(&pool, secret)
            .await?
            .context("claimed again")?;
        photos::read(&pool, &extractor, claimed, b"%PDF-1.4".to_vec()).await;
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Waiting {
                last_failure: Some(ReadingFailure::Unreadable)
            })
        );

        // The daily limit ends the link
        *fake.answer.lock().unwrap_or_else(PoisonError::into_inner) =
            Err(ExtractionError::DailyLimitReached { limit: 50 });
        let claimed = photos::claim(&pool, secret)
            .await?
            .context("claimed a third time")?;
        photos::read(&pool, &extractor, claimed, photo()).await;
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Failed {
                failure: ReadingFailure::DailyLimit
            })
        );
        assert!(photos::claim(&pool, secret).await?.is_none());
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn interrupted_expired_discarded_and_purged(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let secret = started.secret.expose();

        // A reading cut by a restart: the link takes the photo again
        photos::claim(&pool, secret).await?.context("claimed")?;
        sqlx::query("UPDATE photo_imports SET reading_started_at = now() - interval '20 minutes'")
            .execute(&pool)
            .await?;
        assert_eq!(
            photos::link_state(&pool, secret).await?,
            Some(ImportState::Waiting {
                last_failure: Some(ReadingFailure::Interrupted)
            })
        );
        assert!(photos::claim(&pool, secret).await?.is_some(), "sent again");

        // An expired link
        let late = photos::start_import(&pool, Owner::telegram(ALICE), credential).await?;
        sqlx::query(
            "UPDATE photo_imports SET link_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(late.id)
        .execute(&pool)
        .await?;
        assert!(photos::claim(&pool, late.secret.expose()).await?.is_none());

        // Discarded: its draft goes, it cannot be saved
        let dropped = photos::start_import(&pool, Owner::telegram(ALICE), credential).await?;
        photos::discard(&pool, Owner::telegram(ALICE), dropped.id).await?;
        assert!(matches!(
            photos::discard(&pool, Owner::telegram(BOB), dropped.id).await,
            Err(EngineError::NotFound)
        ));
        assert_eq!(
            photos::import_state(&pool, Owner::telegram(ALICE), dropped.id).await?,
            Some(ImportState::Discarded)
        );

        // A restart: the readings it cut are sent again, told why
        let cut = photos::start_import(&pool, Owner::telegram(ALICE), credential).await?;
        photos::claim(&pool, cut.secret.expose())
            .await?
            .context("claimed")?;
        assert_eq!(
            photos::recover_interrupted(&pool).await?,
            2,
            "this one and the first"
        );
        assert_eq!(
            photos::link_state(&pool, cut.secret.expose()).await?,
            Some(ImportState::Waiting {
                last_failure: Some(ReadingFailure::Interrupted)
            })
        );

        sqlx::query("UPDATE photo_imports SET expires_at = now() - interval '2 days'")
            .execute(&pool)
            .await?;
        assert_eq!(photos::purge(&pool).await?, 4);
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn a_draft_keeps_the_text_read_only_until_saved(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let (extractor, _) = extractor(Ok(tart()));
        let claimed = photos::claim(&pool, started.secret.expose())
            .await?
            .context("claimed")?;
        photos::read(&pool, &extractor, claimed, photo()).await;

        // Lines that cannot be read back: no empty recipe, the draft unusable
        let kept: serde_json::Value =
            sqlx::query_scalar("SELECT ingredients FROM photo_imports WHERE id = $1")
                .bind(started.id)
                .fetch_one(&pool)
                .await?;
        sqlx::query("UPDATE photo_imports SET ingredients = '\"broken\"'::jsonb WHERE id = $1")
            .bind(started.id)
            .execute(&pool)
            .await?;
        assert!(matches!(
            photos::import_state(&pool, Owner::telegram(ALICE), started.id).await?,
            Some(ImportState::Failed {
                failure: ReadingFailure::Unavailable
            })
        ));
        assert!(matches!(
            photos::save(&pool, actor(credential), started.id, None, &[]).await,
            Err(SaveError::NotReady(_))
        ));
        sqlx::query("UPDATE photo_imports SET ingredients = $2 WHERE id = $1")
            .bind(started.id)
            .bind(&kept)
            .execute(&pool)
            .await?;

        // Saved: the recipes hold the text, the import no longer does
        photos::save(&pool, actor(credential), started.id, None, &[]).await?;
        let left: (Option<serde_json::Value>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT ingredients, transcription, title FROM photo_imports WHERE id = $1",
        )
        .bind(started.id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(left, (None, None, None));
        Ok(())
    }

    #[sqlx::test(migrations = false)]
    #[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
    async fn two_saves_at_once_make_one_set_of_recipes(pool: PgPool) -> Result<()> {
        let (credential, started) = import(&pool).await?;
        let (extractor, _) = extractor(Ok(tart()));
        let claimed = photos::claim(&pool, started.secret.expose())
            .await?
            .context("claimed")?;
        photos::read(&pool, &extractor, claimed, photo()).await;
        let (a, b) = tokio::join!(
            photos::save(&pool, actor(credential), started.id, None, &[]),
            photos::save(&pool, actor(credential), started.id, None, &[]),
        );
        let (a, b) = (a?, b?);
        assert_eq!(a, b);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM recipes")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 2, "one set of recipes");
        Ok(())
    }
}
