//! The photo upload page (`specs/tasks/mcp-photos.md`, task 3) over a real socket, a fake Gemini
//! and a throwaway database: the form, the reading in the background, every refusal.
//!
//! They need Postgres: `cargo test --test photo_tests -- --include-ignored` with `DATABASE_URL`
//! set to a throwaway database.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Result, bail};
use just_ingredients::db::init_database_schema;
use just_ingredients::doors::photos::{self, PhotoPages};
use just_ingredients::engine::Owner;
use just_ingredients::engine::photos::{self as engine_photos, ImportState, NewImport};
use just_ingredients::engine::tokens::{self, Access, Credential, Realm};
use just_ingredients::extraction::{
    ExtractedIngredient, Extraction, ExtractionFuture, ImageInput, IngredientExtractor,
    RecipeLanguage, SharedExtractor,
};
use sqlx::PgPool;

const ALICE: i64 = 701;

/// Reads every photo as a tart, after `delay`, counting its calls.
#[derive(Debug, Default)]
struct Fake {
    calls: AtomicUsize,
    delay: Duration,
}

impl IngredientExtractor for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn extract<'a>(&'a self, _image: &'a ImageInput) -> ExtractionFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            Ok(Extraction {
                is_recipe: true,
                title: Some("Tarte <b>aux</b> poireaux".to_string()),
                language: RecipeLanguage::Fr,
                transcription: "250 g farine\n3 poireaux".to_string(),
                ingredients: vec![
                    ExtractedIngredient {
                        name: "farine".to_string(),
                        quantity: Some(250.0),
                        unit: Some("g".to_string()),
                        needs_review: false,
                        source_text: "250 g farine".to_string(),
                        section: None,
                    },
                    ExtractedIngredient {
                        name: "poireaux".to_string(),
                        quantity: Some(3.0),
                        unit: None,
                        needs_review: true,
                        source_text: "3 poireaux".to_string(),
                        section: None,
                    },
                ],
            })
        })
    }
}

/// The pages on a free port, with `fake` as Gemini.
async fn serve(pool: &PgPool, fake: Arc<Fake>) -> Result<SocketAddr> {
    let extractor: SharedExtractor = fake;
    let app = photos::router(PhotoPages::new(pool.clone(), extractor));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok(address)
}

/// Alice's read-write key and an import started with it.
async fn import(pool: &PgPool) -> Result<(Credential, NewImport)> {
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
    let started = engine_photos::start_import(pool, Owner::telegram(ALICE), credential).await?;
    Ok((credential, started))
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

struct Answer {
    status: u16,
    headers: reqwest::header::HeaderMap,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

async fn answer(response: reqwest::Response) -> Result<Answer> {
    Ok(Answer {
        status: response.status().as_u16(),
        headers: response.headers().clone(),
        body: response.text().await?,
    })
}

async fn get(address: SocketAddr, path: &str) -> Result<Answer> {
    answer(
        client()?
            .get(format!("http://{address}{path}"))
            .header("Accept-Language", "fr-FR")
            .send()
            .await?,
    )
    .await
}

/// Posts the form with `field` holding `bytes`.
async fn post_photo(address: SocketAddr, path: &str, field: &str, bytes: &[u8]) -> Result<Answer> {
    let boundary = "ji-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; \
         filename=\"tarte.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    answer(
        client()?
            .post(format!("http://{address}{path}"))
            .header("Accept-Language", "fr-FR")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await?,
    )
    .await
}

fn jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]
}

/// The page until it is no longer reading, at most `tries` times.
async fn until_read(address: SocketAddr, path: &str, tries: usize) -> Result<Answer> {
    for _ in 0..tries {
        let page = get(address, path).await?;
        if !page.body.contains("Lecture en cours") {
            return Ok(page);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("still reading")
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_photo_sent_through_the_page_becomes_a_draft(pool: PgPool) -> Result<()> {
    let (_, started) = import(&pool).await?;
    let fake = Arc::new(Fake {
        delay: Duration::from_millis(300),
        ..Fake::default()
    });
    let address = serve(&pool, Arc::clone(&fake)).await?;
    let path = format!("/photo/{}", started.secret.expose());

    // The form: a phone's camera or library, posting to us only
    let page = get(address, &path).await?;
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(
        page.body
            .contains("type=\"file\" name=\"photo\" accept=\"image/*\"")
    );
    assert!(page.body.contains("enctype=\"multipart/form-data\""));
    assert!(!page.body.contains("<script"));
    let policy = page.header("content-security-policy").unwrap_or_default();
    assert!(policy.contains("form-action 'self'"), "{policy}");
    assert!(policy.contains("frame-ancestors 'none'"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    assert_eq!(page.header("referrer-policy"), Some("no-referrer"));

    // A GET never uses the link
    for _ in 0..3 {
        get(address, &path).await?;
    }
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);

    // Sent: back to the page, which reads, reloading itself, then says so
    let sent = post_photo(address, &path, "photo", &jpeg()).await?;
    assert_eq!(sent.status, 303, "{}", sent.body);
    assert_eq!(sent.header("location"), Some(path.as_str()));
    let reading = get(address, &path).await?;
    assert!(
        reading.body.contains("Lecture en cours"),
        "{}",
        reading.body
    );
    assert!(reading.body.contains("http-equiv=\"refresh\""));
    let read = until_read(address, &path, 50).await?;
    assert!(read.body.contains("✅ Photo lue"), "{}", read.body);
    assert!(
        read.body.contains("Tarte &lt;b&gt;aux&lt;/b&gt; poireaux"),
        "the title, escaped: {}",
        read.body
    );
    assert!(read.body.contains("1 quantité(s) à confirmer"));
    assert!(read.body.contains("Retournez dans votre assistant"));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        engine_photos::import_state(&pool, Owner::telegram(ALICE), started.id).await?,
        Some(ImportState::Ready { .. })
    ));

    // Sent again: the link took its photo
    let again = post_photo(address, &path, "photo", &jpeg()).await?;
    assert_eq!(again.status, 303);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1, "not read twice");
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_double_tap_reads_once(pool: PgPool) -> Result<()> {
    let (_, started) = import(&pool).await?;
    let fake = Arc::new(Fake {
        delay: Duration::from_millis(200),
        ..Fake::default()
    });
    let address = serve(&pool, Arc::clone(&fake)).await?;
    let path = format!("/photo/{}", started.secret.expose());
    let photo = jpeg();
    let (first, second) = tokio::join!(
        post_photo(address, &path, "photo", &photo),
        post_photo(address, &path, "photo", &photo)
    );
    assert_eq!((first?.status, second?.status), (303, 303));
    until_read(address, &path, 50).await?;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn refusals_leave_the_link_usable_or_say_why(pool: PgPool) -> Result<()> {
    let (credential, started) = import(&pool).await?;
    let fake = Arc::new(Fake::default());
    let address = serve(&pool, Arc::clone(&fake)).await?;
    let path = format!("/photo/{}", started.secret.expose());

    // Unknown links
    assert_eq!(get(address, "/photo/short").await?.status, 404);
    assert_eq!(
        get(address, &format!("/photo/{}", "A".repeat(43)))
            .await?
            .status,
        404
    );

    // No photo, or one over 10 MB: the form again, the link still usable
    let missing = post_photo(address, &path, "other", &jpeg()).await?;
    assert_eq!(missing.status, 400);
    assert!(
        missing.body.contains("Choisissez une photo"),
        "{}",
        missing.body
    );
    let mut large = jpeg();
    large.resize(photos::MAX_PHOTO_BYTES + 1, 0);
    let too_large = post_photo(address, &path, "photo", &large).await?;
    assert_eq!(too_large.status, 413, "{}", too_large.body);
    assert!(too_large.body.contains("dépasse 10 Mo"));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        engine_photos::link_state(&pool, started.secret.expose()).await?,
        Some(ImportState::Waiting { .. })
    ));

    // The key revoked: the page says the link is no longer valid, and takes nothing
    let Some(key) = credential.key_id() else {
        bail!("a key");
    };
    tokens::revoke(&pool, Owner::telegram(ALICE), key).await?;
    let revoked = get(address, &path).await?;
    assert_eq!(revoked.status, 410, "{}", revoked.body);
    assert!(revoked.body.contains("n'est plus valable"));
    let refused = post_photo(address, &path, "photo", &jpeg()).await?;
    assert_eq!(refused.status, 303);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);

    // An expired link
    let key = tokens::create_with(
        &pool,
        Owner::telegram(ALICE),
        Realm::Production,
        Access::ReadWrite,
    )
    .await?;
    let late =
        engine_photos::start_import(&pool, Owner::telegram(ALICE), Credential::Key(key.id)).await?;
    sqlx::query(
        "UPDATE photo_imports SET link_expires_at = now() - interval '1 second', \
                 expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(late.id)
    .execute(&pool)
    .await?;
    let expired = get(address, &format!("/photo/{}", late.secret.expose())).await?;
    assert_eq!(expired.status, 410);
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn at_most_two_photos_are_read_at_once(pool: PgPool) -> Result<()> {
    let (credential, first) = import(&pool).await?;
    let fake = Arc::new(Fake {
        delay: Duration::from_millis(800),
        ..Fake::default()
    });
    let address = serve(&pool, Arc::clone(&fake)).await?;
    let mut links = vec![first];
    for _ in 0..2 {
        links.push(engine_photos::start_import(&pool, Owner::telegram(ALICE), credential).await?);
    }
    let paths: Vec<String> = links
        .iter()
        .map(|link| format!("/photo/{}", link.secret.expose()))
        .collect();
    let photo = jpeg();
    let (a, b) = tokio::join!(
        post_photo(address, &paths[0], "photo", &photo),
        post_photo(address, &paths[1], "photo", &photo)
    );
    assert_eq!((a?.status, b?.status), (303, 303));
    let third = post_photo(address, &paths[2], "photo", &photo).await?;
    assert_eq!(third.status, 503, "{}", third.body);
    assert!(third.body.contains("réessayez dans un instant"));
    assert!(
        matches!(
            engine_photos::link_state(&pool, links[2].secret.expose()).await?,
            Some(ImportState::Waiting { .. })
        ),
        "the third link is still usable"
    );
    until_read(address, &paths[0], 50).await?;
    until_read(address, &paths[1], 50).await?;
    let later = post_photo(address, &paths[2], "photo", &photo).await?;
    assert_eq!(later.status, 303, "a place again: {}", later.body);
    Ok(())
}
