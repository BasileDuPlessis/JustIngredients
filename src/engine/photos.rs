//! Recipe photos (`specs/tasks/mcp-photos.md`): every door's photo readings go through the bot's
//! Gemini pipeline (`extraction::GuardedExtractor`).
//!
//! - Each user's daily count of paid readings is kept in the database ([`StoredDailyCalls`]):
//!   shared by Telegram and the assistants, kept across deploys, Paris days.
//! - An assistant imports a photo through a link: [`start_import`] makes it (15 minutes, tied to
//!   the key or connection that asked, which must still be allowed to write), the upload page
//!   [`claim`]s it and [`read`]s the photo in the background into a draft, kept 24 hours: the
//!   lines as the bot's review holds them, the text read, the title. [`save`] applies the bot's
//!   review rules (no flagged quantity, a name) and saves the parts as linked recipes, once.

use base64::Engine as _;
use rand::RngCore;
use serde::Serialize;
use sqlx::PgPool;

use super::tokens::Credential;
use super::writes::{self, Actor};
use super::{EngineError, Owner};
use crate::extraction::guard::{DailyCalls, DailyFuture};
use crate::extraction::{ExtractionError, ImageInput, SharedExtractor};
use crate::secret::SecretString;
use crate::text_processing::MeasurementMatch;

/// How long an upload link takes a photo.
pub const LINK_LIFETIME_SECS: i64 = 15 * 60;
/// How long a draft waits to be saved.
pub const DRAFT_LIFETIME_SECS: i64 = 24 * 3600;
/// A reading started this long ago was cut (a restart): the link takes the photo again.
pub const READING_STALE_SECS: i64 = 5 * 60;

/// Days of counts kept (only today's is read).
const KEEP_DAYS: i32 = 7;

/// The daily count of paid readings, in the database.
#[derive(Debug, Clone)]
pub struct StoredDailyCalls {
    pool: PgPool,
}

impl StoredDailyCalls {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DailyCalls for StoredDailyCalls {
    fn take(
        &self,
        user: i64,
        limit: u32,
    ) -> DailyFuture<'_, anyhow::Result<Option<chrono::NaiveDate>>> {
        Box::pin(async move { crate::db::take_photo_call(&self.pool, user, limit).await })
    }

    fn give_back(&self, user: i64, day: chrono::NaiveDate) -> DailyFuture<'_, ()> {
        Box::pin(async move {
            if let Err(error) = crate::db::give_back_photo_call(&self.pool, user, day).await {
                tracing::warn!(error = %error, "Could not give back a photo reading");
            }
        })
    }
}

/// Deletes the counts of past days and the imports expired a day ago.
pub async fn purge(pool: &PgPool) -> Result<u64, EngineError> {
    Ok(crate::db::purge_photo_days(pool, KEEP_DAYS).await?
        + crate::db::purge_photo_imports(pool).await?)
}

// ---------------------------------------------------------------------------------------------
// Imports

fn hash(secret: &str) -> Vec<u8> {
    super::tokens::hash(secret)
}

/// An import just started: its id, and its link's secret (shown once, in the link).
#[derive(Debug, Clone)]
pub struct NewImport {
    pub id: i64,
    pub secret: SecretString,
}

/// Starts an import for `owner`, tied to the key or connection that asked.
pub async fn start_import(
    pool: &PgPool,
    owner: Owner,
    credential: Credential,
) -> Result<NewImport, EngineError> {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random);
    let id = crate::db::insert_photo_import(
        pool,
        &crate::db::NewPhotoImport {
            telegram_id: owner.telegram_id(),
            upload_hash: &hash(&secret),
            token_id: credential.key_id(),
            grant_id: credential.grant_id(),
            link_lifetime_secs: LINK_LIFETIME_SECS,
        },
    )
    .await?;
    Ok(NewImport {
        id,
        secret: SecretString::new(secret),
    })
}

/// Why a reading gave no draft; a code kept with the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingFailure {
    /// The photo shows no recipe.
    NotARecipe,
    /// A recipe, but no ingredient could be read.
    NoIngredients,
    /// Not an image we read (JPEG, PNG, WebP; other formats converted), or over 10 MB.
    Unreadable,
    /// The user's photos of the day are used up.
    DailyLimit,
    /// Gemini was busy or unreachable: try again.
    Unavailable,
    /// The reading was cut (a restart): send it again.
    Interrupted,
}

impl ReadingFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotARecipe => "not_a_recipe",
            Self::NoIngredients => "no_ingredients",
            Self::Unreadable => "unreadable",
            Self::DailyLimit => "daily_limit",
            Self::Unavailable => "unavailable",
            Self::Interrupted => "interrupted",
        }
    }

    fn parse(code: &str) -> Option<Self> {
        [
            Self::NotARecipe,
            Self::NoIngredients,
            Self::Unreadable,
            Self::DailyLimit,
            Self::Unavailable,
            Self::Interrupted,
        ]
        .into_iter()
        .find(|failure| failure.as_str() == code)
    }

    /// Whether another photo may be sent through the same link.
    const fn retry(self) -> bool {
        !matches!(self, Self::DailyLimit)
    }

    fn of(error: &ExtractionError) -> Self {
        match error {
            ExtractionError::DailyLimitReached { .. } => Self::DailyLimit,
            ExtractionError::UnsupportedImage(_) | ExtractionError::ImageTooLarge { .. } => {
                Self::Unreadable
            }
            ExtractionError::Blocked(_)
            | ExtractionError::Recitation
            | ExtractionError::Truncated
            | ExtractionError::InvalidResponse(_) => Self::NoIngredients,
            _ => Self::Unavailable,
        }
    }
}

/// A line of a draft, as the assistant is shown it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftLine {
    /// Its number in the draft, from 1 (the number a correction gives).
    pub line: usize,
    pub name: String,
    /// The quantity as read ("2", "1/2"); none when none was printed.
    pub quantity: Option<String>,
    pub unit: Option<String>,
    /// The part of the photo it is printed under ("Pour la sauce").
    pub part: Option<String>,
    /// Flagged: its quantity must be confirmed before the draft is saved.
    pub to_confirm: bool,
}

/// Where an import stands.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ImportState {
    /// The link waits for the photo; the last reading's failure, if a photo was sent already.
    Waiting {
        last_failure: Option<ReadingFailure>,
    },
    /// Gemini is reading the photo.
    Reading,
    /// Read: the draft to show the user and confirm.
    Ready {
        /// The title read on the page, the name it would be saved under; none: a name must
        /// be given.
        title: Option<String>,
        /// The recipes it becomes (one per part, the first is the dish), once named.
        parts: Vec<String>,
        lines: Vec<DraftLine>,
    },
    /// No draft and no more tries through this link.
    Failed {
        failure: ReadingFailure,
    },
    /// Saved as these recipes.
    Saved {
        recipe_ids: Vec<i64>,
    },
    Discarded,
    /// The link or the draft expired.
    Expired,
}

fn lines_of(row: &crate::db::PhotoImportRow) -> Vec<MeasurementMatch> {
    row.ingredients
        .clone()
        .and_then(|lines| serde_json::from_value(lines).ok())
        .unwrap_or_default()
}

fn state_of(row: &crate::db::PhotoImportRow) -> ImportState {
    let failure = row.failure.as_deref().and_then(ReadingFailure::parse);
    match row.status.as_str() {
        "saved" => ImportState::Saved {
            recipe_ids: row.saved_recipe_ids.clone().unwrap_or_default(),
        },
        "discarded" => ImportState::Discarded,
        _ if !row.alive => ImportState::Expired,
        // A link whose key or connection was revoked (or lost writing) takes no photo
        "waiting" | "reading" if !row.may_write => ImportState::Expired,
        "reading" if row.reading_stale => ImportState::Waiting {
            last_failure: Some(ReadingFailure::Interrupted),
        },
        "reading" => ImportState::Reading,
        "waiting" if !row.link_open => ImportState::Expired,
        "waiting" => ImportState::Waiting {
            last_failure: failure,
        },
        "ready" => {
            let lines = lines_of(row);
            let parts = row.title.as_deref().map_or_else(Vec::new, |title| {
                crate::recipe_parts::part_names(title, &crate::recipe_parts::split_parts(&lines))
            });
            ImportState::Ready {
                title: row.title.clone(),
                parts,
                lines: lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| DraftLine {
                        line: index + 1,
                        name: line.ingredient_name.clone(),
                        quantity: Some(line.quantity.clone()).filter(|q| !q.trim().is_empty()),
                        unit: line.measurement.clone(),
                        part: line.section.clone(),
                        to_confirm: line.requires_quantity_confirmation,
                    })
                    .collect(),
            }
        }
        _ => ImportState::Failed {
            failure: failure.unwrap_or(ReadingFailure::Unavailable),
        },
    }
}

/// Where the import of the link `secret` stands; `None` for an unknown link.
pub async fn link_state(pool: &PgPool, secret: &str) -> Result<Option<ImportState>, EngineError> {
    Ok(
        crate::db::photo_import_by_link(pool, &hash(secret), READING_STALE_SECS)
            .await?
            .as_ref()
            .map(state_of),
    )
}

/// Where the import `id` of `owner` stands; `None` when it is not one of the owner's.
pub async fn import_state(
    pool: &PgPool,
    owner: Owner,
    id: i64,
) -> Result<Option<ImportState>, EngineError> {
    Ok(
        crate::db::photo_import_of(pool, owner.telegram_id(), id, READING_STALE_SECS)
            .await?
            .as_ref()
            .map(state_of),
    )
}

/// An import claimed for a reading: its id and owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claimed {
    pub id: i64,
    pub owner: Owner,
}

/// Claims the link `secret` for a reading, once (a double tap claims nothing): waiting, open,
/// and its key or connection still active and allowed to write.
pub async fn claim(pool: &PgPool, secret: &str) -> Result<Option<Claimed>, EngineError> {
    Ok(
        crate::db::claim_photo_import(pool, &hash(secret), READING_STALE_SECS)
            .await?
            .map(|(id, telegram_id)| Claimed {
                id,
                owner: Owner::telegram(telegram_id),
            }),
    )
}

/// Reads the photo of a claimed import with the bot's pipeline, and keeps the draft, or why
/// there is none. Never fails: the import says what happened.
pub async fn read(pool: &PgPool, extractor: &SharedExtractor, claimed: Claimed, bytes: Vec<u8>) {
    let reading = match ImageInput::prepare(bytes).await {
        Ok(image) => {
            extractor
                .extract(&image.requested_by(claimed.owner.telegram_id()))
                .await
        }
        Err(error) => Err(error),
    };
    let outcome = match reading {
        Ok(extraction) if !extraction.is_recipe => Err(ReadingFailure::NotARecipe),
        Ok(extraction) => {
            let lines = crate::recipe_parts::arrange(
                crate::extraction::mapping::to_measurement_matches(&extraction),
            );
            if lines.is_empty() {
                Err(ReadingFailure::NoIngredients)
            } else {
                Ok((lines, extraction.transcription, extraction.title))
            }
        }
        Err(error) => {
            tracing::info!(
                kind = error.kind(),
                import = claimed.id,
                "Photo import not read"
            );
            Err(ReadingFailure::of(&error))
        }
    };
    let stored = match outcome {
        Ok((lines, transcription, title)) => {
            // The title is the name only if the bot would take it as one
            let title = title
                .as_deref()
                .map(str::trim)
                .and_then(|title| crate::validation::validate_recipe_name(title).ok())
                .map(str::to_string);
            tracing::info!(
                import = claimed.id,
                lines = lines.len(),
                to_confirm = lines
                    .iter()
                    .filter(|line| line.requires_quantity_confirmation)
                    .count(),
                "Photo import read"
            );
            match serde_json::to_value(&lines) {
                Ok(json) => {
                    crate::db::photo_reading_done(
                        pool,
                        claimed.id,
                        &json,
                        &transcription,
                        title.as_deref(),
                        DRAFT_LIFETIME_SECS,
                    )
                    .await
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(failure) => {
            crate::db::photo_reading_failed(pool, claimed.id, failure.as_str(), failure.retry())
                .await
        }
    };
    if let Err(error) = stored {
        tracing::warn!(error = %error, import = claimed.id, "Could not keep a photo's reading");
    }
}

// ---------------------------------------------------------------------------------------------
// Saving a draft

/// Why a draft was not saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("not found")]
    NotFound,
    /// Not ready (still waiting, being read, failed, discarded) or expired.
    #[error("not ready to save")]
    NotReady(ImportState),
    /// The photo gave no name: one must be given.
    #[error("a name is needed")]
    NeedsName,
    /// These lines (number, ingredient) are flagged: their quantity must be confirmed.
    #[error("quantities to confirm")]
    ToConfirm(Vec<(usize, String)>),
    /// A correction for a line that is not flagged, or does not exist.
    #[error("line {0} cannot be changed: only flagged lines can")]
    NotFlagged(usize),
    /// A correction or the name refused, as typed in the bot.
    #[error("line {line} refused: {reason}")]
    InvalidLine {
        line: usize,
        reason: crate::errors::ValidationError,
    },
    #[error("invalid name: {0}")]
    InvalidName(crate::errors::ValidationError),
    #[error(transparent)]
    Internal(#[from] EngineError),
}

impl From<anyhow::Error> for SaveError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(EngineError::Internal(error))
    }
}

/// A saved draft: its recipes (id and name, the dish first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedRecipes {
    pub recipes: Vec<(i64, String)>,
}

/// The recipes a reviewed photo's lines become under `name`, linked: one per part, as the bot
/// saves a photo (`recipe_parts`).
pub fn photo_parts<'a>(
    ingredients: &'a [MeasurementMatch],
    name: &str,
) -> Vec<crate::db::RecipePart<'a>> {
    let parts = crate::recipe_parts::split_parts(ingredients);
    let names = crate::recipe_parts::part_names(name, &parts);
    parts
        .iter()
        .zip(names)
        .map(|(part, name)| crate::db::RecipePart {
            name,
            ingredients: part
                .lines
                .iter()
                .filter_map(|&line| ingredients.get(line))
                .map(crate::db::NewIngredient::from)
                .collect(),
        })
        .collect()
}

/// Saves the ready draft `id` of `owner` as linked recipes, once (a retried save answers the
/// same recipes): under `name`, else the title read; with `corrections` (line number from 1,
/// the line typed again) of flagged lines only; refused while a line stays flagged.
pub async fn save(
    pool: &PgPool,
    actor: Actor,
    id: i64,
    name: Option<&str>,
    corrections: &[(usize, String)],
) -> Result<SavedRecipes, SaveError> {
    let owner = actor.owner;
    let row = crate::db::photo_import_of(pool, owner.telegram_id(), id, READING_STALE_SECS)
        .await?
        .ok_or(SaveError::NotFound)?;
    match state_of(&row) {
        ImportState::Ready { .. } => {}
        ImportState::Saved { recipe_ids } => return saved_again(pool, owner, &recipe_ids).await,
        other => return Err(SaveError::NotReady(other)),
    }
    let mut lines = lines_of(&row);
    for (number, typed) in corrections {
        let index = number
            .checked_sub(1)
            .ok_or(SaveError::NotFlagged(*number))?;
        let section = match lines.get(index) {
            Some(line) if line.requires_quantity_confirmation => line.section.clone(),
            _ => return Err(SaveError::NotFlagged(*number)),
        };
        let mut corrected = crate::validation::parse_ingredient_from_text(typed.trim()).map_err(
            |error| match error {
                crate::errors::AppError::Validation(reason) => SaveError::InvalidLine {
                    line: *number,
                    reason,
                },
                other => SaveError::from(anyhow::anyhow!("{other}")),
            },
        )?;
        corrected.section = section;
        if let Some(line) = lines.get_mut(index) {
            *line = corrected;
        }
    }
    let to_confirm: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.requires_quantity_confirmation)
        .map(|(index, line)| (index + 1, line.ingredient_name.clone()))
        .collect();
    if !to_confirm.is_empty() {
        return Err(SaveError::ToConfirm(to_confirm));
    }
    let name = match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => crate::validation::validate_recipe_name(name)
            .map_err(|error| match error {
                crate::errors::AppError::Validation(reason) => SaveError::InvalidName(reason),
                other => SaveError::from(anyhow::anyhow!("{other}")),
            })?
            .to_string(),
        None => row.title.clone().ok_or(SaveError::NeedsName)?,
    };
    let transcription = row.transcription.clone().unwrap_or_default();
    let parts = photo_parts(&lines, &name);

    // The draft is claimed in the transaction that creates its recipes: saved once, or not at all
    let mut tx = writes::begin(pool).await?;
    if !crate::db::claim_photo_draft(&mut tx, owner.telegram_id(), id).await? {
        drop(tx);
        return match import_state(pool, owner, id).await? {
            Some(ImportState::Saved { recipe_ids }) => saved_again(pool, owner, &recipe_ids).await,
            Some(other) => Err(SaveError::NotReady(other)),
            None => Err(SaveError::NotFound),
        };
    }
    let saved = writes::create_in(&mut tx, actor, None, &transcription, &parts).await?;
    let ids: Vec<i64> = saved.iter().map(|recipe| recipe.recipe_id).collect();
    crate::db::photo_draft_saved(&mut tx, id, &ids).await?;
    writes::commit(tx).await?;
    Ok(SavedRecipes {
        recipes: saved
            .into_iter()
            .map(|recipe| (recipe.recipe_id, recipe.name))
            .collect(),
    })
}

/// The recipes a draft was saved as, for a save asked again.
async fn saved_again(
    pool: &PgPool,
    owner: Owner,
    recipe_ids: &[i64],
) -> Result<SavedRecipes, SaveError> {
    let mut recipes = Vec::new();
    for &id in recipe_ids {
        if let Some(recipe) =
            crate::db::read_recipe_with_name(pool, owner.telegram_id(), id).await?
        {
            recipes.push((id, recipe.recipe_name.unwrap_or_default()));
        }
    }
    Ok(SavedRecipes { recipes })
}

/// Drops the import `id` of `owner` (unless saved); [`EngineError::NotFound`] when there is
/// none to drop.
pub async fn discard(pool: &PgPool, owner: Owner, id: i64) -> Result<(), EngineError> {
    if crate::db::discard_photo_import(pool, owner.telegram_id(), id).await? {
        Ok(())
    } else {
        Err(EngineError::NotFound)
    }
}
