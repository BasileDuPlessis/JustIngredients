//! The photo upload page (`specs/tasks/mcp-photos.md`, §2 and §7): an assistant's
//! `import_photo` gives the user a link, `https://…/photo/{secret}`; the page takes one photo
//! (on a phone: the camera or the library), which the bot reads with its own Gemini pipeline in
//! the background (`engine::photos::read`), then says "read: go back to your assistant".
//!
//! - The link is a capability: whoever holds it can add a photo to the asker's account (not
//!   read, not change). It lives 15 minutes, takes one photo that gives a draft, and works only
//!   while the key or connection that asked may still write. A GET never uses it (link previews,
//!   an assistant fetching it); the secret is in the path and there is no cookie, so another
//!   site's form gains nothing.
//! - The link is checked before the body is read; the photo is read in chunks, 10 MB at most;
//!   at most [`MAX_READINGS`] readings at once (Gemini's slots are the bot's too), beyond: "busy,
//!   try again", the link still usable.
//! - The form answers 303 to the page, which reloads itself while the photo is read: the reading
//!   goes on whatever becomes of the page (a phone going back to the assistant).
//! - Pages: no script, never framed nor cached, no referrer, the form posting to us only.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use sqlx::PgPool;
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::{info, warn};

use super::oauth::page::{PageOptions, escape, html_page, lang_of, see_other, text};
use crate::engine::Lang;
use crate::engine::photos::{self, ImportState, ReadingFailure};
use crate::extraction::SharedExtractor;
use crate::observability::server::RateLimiter;

/// The upload pages' path: `/photo/{secret}`.
pub const PATH: &str = "/photo";

/// The largest photo taken (as the bot takes from Telegram).
pub const MAX_PHOTO_BYTES: usize = 10 * 1024 * 1024;

/// The largest body read: the photo and the form's framing.
const MAX_BODY_BYTES: usize = MAX_PHOTO_BYTES + 64 * 1024;

/// Photos read at once from the pages (the bot's own photos share Gemini's slots).
pub const MAX_READINGS: usize = 2;

/// Page requests handled at once (each may hold a photo in memory).
const MAX_AT_ONCE: usize = 3;

/// How long a request waits for a place.
const WAIT_FOR_PLACE: Duration = Duration::from_secs(2);

/// How often the page reloads while the photo is read.
const REFRESH_SECS: u32 = 3;

/// The upload link of an import, for the assistant to give the user.
pub fn link(host: &str, secret: &str) -> String {
    format!("https://{host}{PATH}/{secret}")
}

/// What the pages need, shared by their requests.
#[derive(Debug)]
pub struct PhotoPages {
    pool: PgPool,
    extractor: SharedExtractor,
    readings: Arc<Semaphore>,
    at_once: Semaphore,
    /// Photos sent per address.
    posts: RateLimiter,
}

impl PhotoPages {
    pub fn new(pool: PgPool, extractor: SharedExtractor) -> Arc<Self> {
        Arc::new(Self {
            pool,
            extractor,
            readings: Arc::new(Semaphore::new(MAX_READINGS)),
            at_once: Semaphore::new(MAX_AT_ONCE),
            posts: RateLimiter::new(20, 60),
        })
    }

    async fn place(&self) -> Option<SemaphorePermit<'_>> {
        tokio::time::timeout(WAIT_FOR_PLACE, self.at_once.acquire())
            .await
            .ok()?
            .ok()
    }
}

/// The upload pages' routes.
pub fn router(pages: Arc<PhotoPages>) -> Router {
    Router::new()
        .route(&format!("{PATH}/{{secret}}"), get(show).post(upload))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(pages)
}

/// Whether `secret` has the shape of a link's secret: 43 base64url characters.
fn has_secret_shape(secret: &str) -> bool {
    secret.len() == 43
        && secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

// ---------------------------------------------------------------------------------------------
// Pages

fn page(lang: Lang, status: StatusCode, title: &str, body: &str, options: PageOptions) -> Response {
    html_page(lang, status, title, body, options)
}

fn unknown_link(lang: Lang) -> Response {
    page(
        lang,
        StatusCode::NOT_FOUND,
        text(lang, "Lien inconnu", "Unknown link"),
        text(
            lang,
            "<p>Ce lien n'existe pas. Demandez-en un nouveau à votre assistant.</p>",
            "<p>This link does not exist. Ask your assistant for a new one.</p>",
        ),
        PageOptions::default(),
    )
}

fn busy(lang: Lang) -> Response {
    page(
        lang,
        StatusCode::SERVICE_UNAVAILABLE,
        text(lang, "Un instant", "One moment"),
        text(
            lang,
            "<p>Trop de photos en cours : réessayez dans un instant.</p>",
            "<p>Too many photos at once: try again in a moment.</p>",
        ),
        PageOptions::default(),
    )
}

/// The upload form, with why the last photo gave nothing (`notice`, already escaped).
fn form(lang: Lang, status: StatusCode, secret: &str, notice: Option<&str>) -> Response {
    let notice = notice
        .map(|notice| format!("<p>⚠️ {notice}</p>"))
        .unwrap_or_default();
    let body = format!(
        "{notice}<p>{intro}</p>\
         <form method=\"post\" enctype=\"multipart/form-data\" action=\"{PATH}/{secret}\">\
         <p><input type=\"file\" name=\"photo\" accept=\"image/*\" required></p>\
         <p><button type=\"submit\">{send}</button></p></form><p><small>{small}</small></p>",
        intro = text(
            lang,
            "Choisissez la photo de la recette (ou prenez-la). JustIngredients la lira, puis \
             votre assistant vous montrera les ingrédients avant de l'enregistrer.",
            "Choose the recipe's photo (or take it). JustIngredients will read it, then your \
             assistant will show you the ingredients before saving it."
        ),
        send = text(lang, "Envoyer la photo", "Send the photo"),
        small = text(
            lang,
            "Une photo, 10 Mo au plus. Elle n'est pas conservée.",
            "One photo, 10 MB at most. It is not kept."
        ),
        secret = escape(secret),
    );
    page(
        lang,
        status,
        text(lang, "Envoyer une photo de recette", "Send a recipe photo"),
        &body,
        PageOptions {
            form: true,
            refresh_secs: None,
        },
    )
}

fn failure_text(lang: Lang, failure: ReadingFailure) -> &'static str {
    match failure {
        ReadingFailure::NotARecipe => text(
            lang,
            "Cette photo ne montre pas de recette. Essayez une autre photo.",
            "This photo shows no recipe. Try another photo.",
        ),
        ReadingFailure::NoIngredients => text(
            lang,
            "Aucun ingrédient n'a pu être lu sur cette photo. Essayez une photo plus nette.",
            "No ingredient could be read on this photo. Try a sharper photo.",
        ),
        ReadingFailure::Unreadable => text(
            lang,
            "Ce fichier n'a pas pu être lu (JPEG, PNG ou WebP, 10 Mo au plus).",
            "This file could not be read (JPEG, PNG or WebP, 10 MB at most).",
        ),
        ReadingFailure::DailyLimit => text(
            lang,
            "Vous avez atteint le nombre de photos lues pour aujourd'hui. Réessayez demain.",
            "You have reached today's number of photos. Please try again tomorrow.",
        ),
        ReadingFailure::Unavailable => text(
            lang,
            "La lecture n'a pas abouti. Réessayez dans un instant.",
            "The reading did not succeed. Try again in a moment.",
        ),
        ReadingFailure::Interrupted => text(
            lang,
            "La lecture a été interrompue. Envoyez la photo à nouveau.",
            "The reading was interrupted. Send the photo again.",
        ),
    }
}

/// The page for where the link's import stands.
fn page_for(lang: Lang, secret: &str, state: &ImportState) -> Response {
    let back = text(
        lang,
        "Retournez dans votre assistant : il vous montrera les ingrédients avant de les \
         enregistrer.",
        "Go back to your assistant: it will show you the ingredients before saving them.",
    );
    match state {
        ImportState::Waiting { last_failure } => form(
            lang,
            StatusCode::OK,
            secret,
            last_failure.map(|failure| failure_text(lang, failure)),
        ),
        ImportState::Reading => page(
            lang,
            StatusCode::OK,
            text(lang, "Lecture en cours…", "Reading…"),
            text(
                lang,
                "<p>JustIngredients lit la photo (jusqu'à une minute). Cette page se met à jour \
                 toute seule.</p>",
                "<p>JustIngredients is reading the photo (up to a minute). This page updates by \
                 itself.</p>",
            ),
            PageOptions {
                form: false,
                refresh_secs: Some(REFRESH_SECS),
            },
        ),
        ImportState::Ready { title, lines, .. } => {
            let to_confirm = lines.iter().filter(|line| line.to_confirm).count();
            let name = title.as_deref().map(escape);
            let summary = match (lang.is_french(), name) {
                (true, Some(name)) => {
                    format!("<strong>{name}</strong>, {} ingrédients", lines.len())
                }
                (true, None) => format!("{} ingrédients (recette sans titre)", lines.len()),
                (false, Some(name)) => {
                    format!("<strong>{name}</strong>, {} ingredients", lines.len())
                }
                (false, None) => format!("{} ingredients (untitled recipe)", lines.len()),
            };
            let confirm = match (to_confirm, lang.is_french()) {
                (0, _) => String::new(),
                (n, true) => format!(" ; {n} quantité(s) à confirmer"),
                (n, false) => format!("; {n} quantity(ies) to confirm"),
            };
            page(
                lang,
                StatusCode::OK,
                text(lang, "✅ Photo lue", "✅ Photo read"),
                &format!("<p>{summary}{confirm}.</p><p>{back}</p>"),
                PageOptions::default(),
            )
        }
        ImportState::Failed { failure } => page(
            lang,
            StatusCode::OK,
            text(lang, "Photo non lue", "Photo not read"),
            &format!("<p>{}</p>", failure_text(lang, *failure)),
            PageOptions::default(),
        ),
        ImportState::Saved { .. } => page(
            lang,
            StatusCode::OK,
            text(lang, "Déjà enregistrée", "Already saved"),
            text(
                lang,
                "<p>Cette recette est enregistrée : elle est dans votre liste du bot.</p>",
                "<p>This recipe is saved: it is in your list in the bot.</p>",
            ),
            PageOptions::default(),
        ),
        ImportState::Discarded | ImportState::Expired => page(
            lang,
            StatusCode::GONE,
            text(lang, "Lien expiré", "Link expired"),
            text(
                lang,
                "<p>Ce lien n'est plus valable. Demandez-en un nouveau à votre assistant.</p>",
                "<p>This link is no longer valid. Ask your assistant for a new one.</p>",
            ),
            PageOptions::default(),
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// Handlers

async fn show(
    State(pages): State<Arc<PhotoPages>>,
    Path(secret): Path<String>,
    headers: HeaderMap,
) -> Response {
    let lang = lang_of(&headers);
    if !has_secret_shape(&secret) {
        return unknown_link(lang);
    }
    let Some(_place) = pages.place().await else {
        return busy(lang);
    };
    match photos::link_state(&pages.pool, &secret).await {
        Ok(Some(state)) => page_for(lang, &secret, &state),
        Ok(None) => unknown_link(lang),
        Err(error) => {
            warn!(error = %error, "Photo page: could not read the link");
            busy(lang)
        }
    }
}

/// Why a photo was not taken from the form.
enum Received {
    Photo(Vec<u8>),
    Missing,
    TooLarge,
    Broken,
}

/// The form's `photo` field, read in chunks up to [`MAX_PHOTO_BYTES`].
async fn receive(mut multipart: Multipart) -> Received {
    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => return Received::Missing,
            Err(_) => return Received::Broken,
        };
        if field.name() != Some("photo") {
            continue;
        }
        let mut bytes = Vec::new();
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if bytes.len() + chunk.len() > MAX_PHOTO_BYTES {
                        return Received::TooLarge;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) if bytes.is_empty() => return Received::Missing,
                Ok(None) => return Received::Photo(bytes),
                // A body over the route's limit ends here too
                Err(_) => return Received::TooLarge,
            }
        }
    }
}

async fn upload(
    State(pages): State<Arc<PhotoPages>>,
    Path(secret): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Response {
    let lang = lang_of(&headers);
    if !has_secret_shape(&secret) {
        return unknown_link(lang);
    }
    if !pages
        .posts
        .is_allowed(&super::address_key(&headers, peer.ip()))
    {
        return busy(lang);
    }
    let Some(_place) = pages.place().await else {
        return busy(lang);
    };
    let here = format!("{PATH}/{secret}");
    // The link first: a body is read only for a link that takes a photo
    match photos::link_state(&pages.pool, &secret).await {
        Ok(Some(ImportState::Waiting { .. })) => {}
        Ok(Some(_)) => return see_other(&here),
        Ok(None) => return unknown_link(lang),
        Err(error) => {
            warn!(error = %error, "Photo page: could not read the link");
            return busy(lang);
        }
    }
    let Ok(reading) = Arc::clone(&pages.readings).try_acquire_owned() else {
        return form(
            lang,
            StatusCode::SERVICE_UNAVAILABLE,
            &secret,
            Some(text(
                lang,
                "Trop de photos en cours de lecture : réessayez dans un instant.",
                "Too many photos being read: try again in a moment.",
            )),
        );
    };
    let Ok(multipart) = multipart else {
        return form(lang, StatusCode::BAD_REQUEST, &secret, None);
    };
    let bytes = match receive(multipart).await {
        Received::Photo(bytes) => bytes,
        Received::Missing => {
            return form(
                lang,
                StatusCode::BAD_REQUEST,
                &secret,
                Some(text(lang, "Choisissez une photo.", "Choose a photo.")),
            );
        }
        Received::TooLarge => {
            return form(
                lang,
                StatusCode::PAYLOAD_TOO_LARGE,
                &secret,
                Some(text(
                    lang,
                    "Cette photo dépasse 10 Mo : envoyez-en une plus petite.",
                    "This photo is over 10 MB: send a smaller one.",
                )),
            );
        }
        Received::Broken => return form(lang, StatusCode::BAD_REQUEST, &secret, None),
    };
    // Claimed once: a double tap, or another page, finds it being read
    let claimed = match photos::claim(&pages.pool, &secret).await {
        Ok(Some(claimed)) => claimed,
        Ok(None) => return see_other(&here),
        Err(error) => {
            warn!(error = %error, "Photo page: could not claim the link");
            return busy(lang);
        }
    };
    info!(
        import = claimed.id,
        bytes = bytes.len(),
        "Photo import received"
    );
    crate::observability::action(crate::observability::Action::PhotoImportReceived);
    let pool = pages.pool.clone();
    let extractor = Arc::clone(&pages.extractor);
    tokio::spawn(async move {
        let _reading = reading;
        photos::read(&pool, &extractor, claimed, bytes).await;
    });
    see_other(&here)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_43_base64url_characters() {
        assert!(has_secret_shape(&"a".repeat(43)));
        assert!(!has_secret_shape(&"a".repeat(42)));
        assert!(!has_secret_shape(&format!("{}/", "a".repeat(42))));
        assert_eq!(
            link("just-ingredients.fly.dev", "abc"),
            "https://just-ingredients.fly.dev/photo/abc"
        );
    }
}
