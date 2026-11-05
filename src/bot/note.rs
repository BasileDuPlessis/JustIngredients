//! A recipe's note: a short free text kept with the recipe (oven temperature, cooking time, the
//! link of its source).
//!
//! Asked once a recipe is saved ([`ask`] with `creation`, "Passer" as the exit), and written or
//! cleared later from the card's 🗒️ button. Both use one dialogue state, `EditingRecipeNote`,
//! and one answer handler. The recipe is saved before the question: walking away loses nothing.
//!
//! The typed text goes through `validation::validate_recipe_note` (one clean line, 200
//! characters at most, refused rather than cut) and is shown only through `Html::text`. It is
//! never logged: only counters.

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use tracing::warn;

use super::cleanup::{self, send_transient};
use super::home::{self, ListAt};
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};
use crate::errors::{AppError, error_logging};
use crate::localization::{LocalizationManager, t_args_lang, t_lang};
use crate::observability::{self, Action};

/// The recipe a note question is about.
#[derive(Debug, Clone, Copy)]
pub struct NoteTarget<'a> {
    pub recipe_id: i64,
    pub name: &'a str,
    /// The note it has now (a question from the card shows it).
    pub current: Option<&'a str>,
}

/// Asks for the note of a recipe: the question goes at the bottom with its buttons (the lists
/// and the active card go into the history, as for a rename), and the typed answer is the note.
/// `creation`: the question right after the recipe was saved.
pub async fn ask(
    bot: &Bot,
    chat_id: ChatId,
    dialogue: &RecipeDialogue,
    target: NoteTarget<'_>,
    creation: bool,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    let title_key = if creation {
        "note-prompt-create"
    } else {
        "note-prompt-edit"
    };
    let mut text = format!(
        "🗒️ {}\n\n",
        t_args_lang(
            localization,
            title_key,
            &[("name", target.name)],
            language_code
        )
    );
    if !creation {
        match target.current {
            Some(note) => text.push_str(&t_args_lang(
                localization,
                "note-current",
                &[("note", note)],
                language_code,
            )),
            None => text.push_str(&t_lang(localization, "note-none-yet", language_code)),
        }
        text.push_str("\n\n");
    }
    text.push_str(&t_lang(localization, "note-prompt-hint", language_code));

    cleanup::delete_lists(bot, chat_id).await;
    let sent = bot
        .send_message(chat_id, text)
        .link_preview_options(super::html::no_link_preview())
        .reply_markup(super::ui_builder::note_keyboard(
            target.recipe_id,
            creation,
            target.current.is_some(),
            language_code,
            localization,
        ))
        .await?;
    cleanup::track(chat_id, sent.id).await;
    dialogue
        .update(RecipeDialogueState::EditingRecipeNote {
            recipe_id: target.recipe_id,
            recipe_name: target.name.to_string(),
            creation,
            language_code: language_code.map(str::to_string),
        })
        .await?;
    Ok(())
}

/// Right after the recipes of a photo were saved: asks for the note of the main one (the
/// first). A failure to ask is logged and the list comes back instead, as if the question had
/// been skipped: the recipes are saved either way.
pub async fn ask_after_save(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    dialogue: &RecipeDialogue,
    saved: &[crate::db::SavedRecipe],
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    let Some(main) = saved.first() else {
        home::send_home(bot, pool, chat_id, localization, language_code).await;
        return;
    };
    // The review and the name prompt go: the question is the only message of the action
    cleanup::delete_action_messages(bot, chat_id).await;
    let target = NoteTarget {
        recipe_id: main.recipe_id,
        name: &main.name,
        current: None,
    };
    if let Err(error) = ask(
        bot,
        chat_id,
        dialogue,
        target,
        true,
        localization,
        language_code,
    )
    .await
    {
        warn!(error = %error, "Could not ask for the recipe note");
        home::send_home_at(
            bot,
            pool,
            chat_id,
            ListAt::Recipe(main.recipe_id),
            localization,
            language_code,
        )
        .await;
    }
}

/// What a typed answer or a button of a note question acts with.
#[derive(Clone, Copy)]
pub struct NoteAnswer<'a> {
    pub bot: &'a Bot,
    pub pool: &'a PgPool,
    pub chat_id: ChatId,
    /// The sender: the owner of the recipe.
    pub telegram_id: i64,
    pub recipe_id: i64,
    pub recipe_name: &'a str,
    pub creation: bool,
    pub localization: &'a Arc<LocalizationManager>,
    pub language_code: Option<&'a str>,
}

/// The note typed as the answer. A text refused (empty, too long) keeps the question open with
/// the reason; a database failure too, with "try again".
pub async fn answer_text(
    answer: NoteAnswer<'_>,
    dialogue: &RecipeDialogue,
    text: &str,
) -> Result<()> {
    let NoteAnswer {
        bot,
        chat_id,
        localization,
        language_code,
        ..
    } = answer;
    let note = match crate::validation::validate_recipe_note(text) {
        Ok(note) => note,
        Err(AppError::Validation(reason)) => {
            send_transient(
                bot,
                chat_id,
                t_lang(localization, reason.l10n_key(), language_code),
            )
            .await?;
            return Ok(());
        }
        Err(error) => {
            // The validator only knows validation reasons; nothing of the text is quoted
            warn!(error = %error, "Unexpected error validating a recipe note");
            return Ok(());
        }
    };
    write(answer, dialogue, Some(&note)).await
}

/// "🗑 Effacer la note".
pub async fn clear(answer: NoteAnswer<'_>, dialogue: &RecipeDialogue) -> Result<()> {
    write(answer, dialogue, None).await
}

/// Writes the note (`None` clears it), then ends the action: the cards of the recipe show it,
/// the history gets one line, the list comes back on the recipe's page.
async fn write(
    answer: NoteAnswer<'_>,
    dialogue: &RecipeDialogue,
    note: Option<&str>,
) -> Result<()> {
    let NoteAnswer {
        bot,
        pool,
        chat_id,
        telegram_id,
        recipe_id,
        recipe_name,
        creation,
        localization,
        language_code,
    } = answer;
    let written = crate::db::set_recipe_note(pool, telegram_id, recipe_id, note).await;
    let found = match written {
        Ok(found) => found,
        Err(error) => {
            error_logging::log_database_error(
                &error,
                "set_recipe_note",
                Some(telegram_id),
                Some(&[("recipe_id", &recipe_id.to_string())]),
            );
            // Nothing was written: the question stays open to answer again
            send_transient(
                bot,
                chat_id,
                t_lang(localization, "save-failed-retry", language_code),
            )
            .await?;
            return Ok(());
        }
    };
    crate::dialogue::end_after_write(dialogue).await;

    let mut keep = Vec::new();
    if found {
        observability::action(match (note.is_some(), creation) {
            (false, _) => Action::NoteCleared,
            (true, true) => Action::NoteAdded,
            (true, false) => Action::NoteEdited,
        });
        // The cards already shown get the note (or lose it)
        super::cards::refresh_cards(
            bot,
            pool,
            chat_id,
            telegram_id,
            recipe_id,
            language_code,
            localization,
        )
        .await;
        let key = if note.is_some() {
            "note-saved-line"
        } else {
            "note-cleared-line"
        };
        let line = t_args_lang(localization, key, &[("name", recipe_name)], language_code);
        if let Err(error) = bot
            .send_message(chat_id, line)
            .disable_notification(true)
            .await
        {
            warn!(error = %error, "Failed to send the note line");
        }
    } else {
        // Deleted meanwhile: say so, and keep the message until the next action
        let sent = bot
            .send_message(
                chat_id,
                t_lang(localization, "recipe-not-found", language_code),
            )
            .await?;
        keep.push(sent.id);
    }
    home::send_home_keeping_at(
        bot,
        pool,
        chat_id,
        &keep,
        ListAt::Recipe(recipe_id),
        localization,
        language_code,
    )
    .await;
    Ok(())
}

/// The question left without a note, by "Passer" / "❌ Annuler" or a cancel word, once the
/// action has ended: after a creation the list comes back (the recipe is saved), from a card the
/// card is the bottom message again.
pub async fn leave(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    recipe_id: i64,
    creation: bool,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) {
    if creation {
        observability::action(Action::NoteSkipped);
        home::send_home_at(
            bot,
            pool,
            chat_id,
            ListAt::Recipe(recipe_id),
            localization,
            language_code,
        )
        .await;
    } else {
        home::back_to_card(
            bot,
            pool,
            chat_id,
            recipe_id,
            None,
            localization,
            language_code,
        )
        .await;
    }
}
