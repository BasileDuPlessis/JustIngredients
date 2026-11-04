//! Message Handler module for processing incoming Telegram messages

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;
use tracing::Instrument;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};
use crate::extraction::SharedExtractor;

// Import command handlers
use super::cleanup;
use super::command_handlers::{
    handle_help_command, handle_recipes_command, handle_search_command, handle_start_command,
    handle_unknown_command, handle_unsupported_message,
};
use super::commands::{route_text, Command, TextRoute};

// Import media handlers
use super::media_handlers::{handle_document_message, handle_photo_message};

async fn handle_text_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    if let Some(text) = msg.text() {
        debug!(user_id = %msg.chat.id, message_length = text.len(), "Received text message from user");

        // Extract user's language code from Telegram
        let language_code = msg
            .from
            .as_ref()
            .and_then(|user| user.language_code.as_ref())
            .map(|s| s.as_str());

        let dialogue_state = dialogue.get().await?;
        let in_action = dialogue_state
            .as_ref()
            .is_some_and(RecipeDialogueState::is_action);

        match route_text(text, in_action) {
            TextRoute::Command(command) => {
                if in_action {
                    // The command ends the running action, which leaves no message behind;
                    // the recipe list stays
                    dialogue.exit().await?;
                    cleanup::delete_action_messages(bot, msg.chat.id).await;
                }
                // The typed command goes too: with its answer, or at once for /liste
                cleanup::track(msg.chat.id, msg.id).await;
                return handle_command(bot, msg, pool, localization, language_code, command).await;
            }
            // Replies typed during an action are deleted when it ends
            TextRoute::ActionInput => cleanup::track(msg.chat.id, msg.id).await,
            TextRoute::FreeText => {}
        }

        if super::action_input::answer_in_action(
            bot,
            msg,
            dialogue,
            Arc::clone(&pool),
            localization,
            language_code,
            dialogue_state,
        )
        .await?
        {
            return Ok(());
        }
        answer_free_text(bot, msg, &pool, localization, language_code, text).await?;
    }
    Ok(())
}

/// A text typed outside an action: a search in the user's recipes, or, for a pasted recipe, the
/// reminder that recipes come from photos.
async fn answer_free_text(
    bot: &Bot,
    msg: &Message,
    pool: &PgPool,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
    text: &str,
) -> Result<()> {
    // A pasted recipe is no search: it stays in the history, under a reminder that recipes
    // come from photos
    if !crate::search::is_query(text) {
        let tip = cleanup::send_transient(
            bot,
            msg.chat.id,
            t_lang(localization, "free-text-tip", language_code),
        )
        .await?;
        super::home::answer_at_bottom(bot, msg.chat.id, tip.id, false, localization, language_code)
            .await;
        return Ok(());
    }
    // Other free text outside an action is a search in the user's recipes (U4.2); the typed
    // text goes with the previous answers
    cleanup::track(msg.chat.id, msg.id).await;
    crate::bot::recipe_list::show_search_results(
        bot,
        pool,
        msg.chat.id,
        text,
        language_code,
        localization,
    )
    .await?;
    Ok(())
}

/// Runs a typed command; `None` is an unknown command.
async fn handle_command(
    bot: &Bot,
    msg: &Message,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
    language_code: Option<&str>,
    command: Option<Command>,
) -> Result<()> {
    match command {
        Some(Command::Start) => {
            handle_start_command(bot, msg, localization, language_code).await?;
            Ok(())
        }
        Some(Command::Help) => {
            handle_help_command(bot, msg, localization, language_code).await?;
            Ok(())
        }
        Some(Command::List) => {
            handle_recipes_command(bot, msg, pool, language_code, localization).await
        }
        Some(Command::Search(words)) => {
            handle_search_command(bot, msg, &pool, &words, localization, language_code).await
        }
        Some(Command::Units) => {
            super::unit_choice::show_chooser(bot, &pool, msg.chat.id, language_code, localization)
                .await
        }
        // The running action, if any, was ended by the caller: the list comes back
        Some(Command::Cancel) => {
            crate::bot::home::send_home(bot, &pool, msg.chat.id, localization, language_code).await;
            Ok(())
        }
        // The owner's usage report (plan O5); anyone else gets the unknown command answer
        Some(Command::Stats) if crate::observability::alerts::owner_chat() == Some(msg.chat.id) => {
            let report = crate::observability::report::stats(&pool).await?;
            let sent = bot.send_message(msg.chat.id, report).await?;
            cleanup::track(msg.chat.id, sent.id).await;
            crate::bot::home::answer_at_bottom(
                bot,
                msg.chat.id,
                sent.id,
                false,
                localization,
                language_code,
            )
            .await;
            Ok(())
        }
        Some(Command::Stats) | None => {
            handle_unknown_command(bot, msg, localization, language_code).await?;
            Ok(())
        }
    }
}

/// Records the request in the deduplicator and returns `true` when it was already seen.
///
/// Each incoming message must go through this check exactly once: the deduplicator records
/// the request on the first call, so a second check would drop the message as a duplicate.
fn is_duplicate_message(
    deduplicator: Option<&crate::deduplication::SharedDeduplicator>,
    chat_id: teloxide::types::ChatId,
    message_id: teloxide::types::MessageId,
) -> Result<bool> {
    let Some(dedup) = deduplicator else {
        return Ok(false);
    };
    let request_id = crate::deduplication::RequestId::new(chat_id, message_id);
    if dedup.is_duplicate(&request_id)? {
        debug!(
            "Ignoring duplicate request: chat_id={}, message_id={}",
            chat_id, message_id
        );
        return Ok(true);
    }
    Ok(false)
}

/// Handles a message that already passed the duplicate check.
async fn dispatch_message(
    bot: Bot,
    msg: Message,
    pool: Arc<PgPool>,
    dialogue: RecipeDialogue,
    localization: Arc<crate::localization::LocalizationManager>,
    extractor: SharedExtractor,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    if let Some(user) = &msg.from {
        crate::observability::usage::seen(
            &pool,
            crate::bot::telegram_id(user.id),
            user.language_code.as_deref(),
        );
    }
    let result = if msg.text().is_some() {
        handle_text_message(&bot, &msg, dialogue, pool, &localization).await
    } else if msg.photo().is_some() {
        handle_photo_message(&bot, &msg, dialogue, pool, &localization, &extractor).await
    } else if msg.document().is_some() {
        handle_document_message(&bot, &msg, dialogue, pool, &localization, &extractor).await
    } else {
        match dialogue.get().await {
            Ok(state) => {
                let in_action = state.is_some_and(|state| state.is_action());
                handle_unsupported_message(&bot, &msg, in_action, &localization).await
            }
            Err(error) => Err(error.into()),
        }
    };

    let duration = start_time.elapsed();
    crate::observability::update_handled("message", duration);
    if result.is_err() {
        crate::observability::error("handler", "message");
    }

    result
}

/// Services shared by every message, built once at startup and cloned per message (all
/// fields are reference-counted).
#[derive(Clone)]
pub struct MessageServices {
    pub pool: Arc<PgPool>,
    pub localization: Arc<crate::localization::LocalizationManager>,
    pub deduplicator: Option<crate::deduplication::SharedDeduplicator>,
    pub extractor: SharedExtractor,
}

/// The message branch of the dispatcher: drops duplicate deliveries, then handles the message
/// by its type (text, photo, document, other).
pub async fn message_handler_with_cache(
    bot: Bot,
    msg: Message,
    dialogue: RecipeDialogue,
    services: MessageServices,
) -> Result<()> {
    let MessageServices {
        pool,
        localization,
        deduplicator,
        extractor,
    } = services;
    let span = crate::observability::telegram_span(
        "message_handler_with_cache",
        msg.from.as_ref().map(|u| crate::bot::telegram_id(u.id)),
    );
    // Instrumented, not entered: an entered span would tag other tasks across .await
    async {
        if is_duplicate_message(deduplicator.as_ref(), msg.chat.id, msg.id)? {
            return Ok(());
        }

        // The duplicate check above already ran and must not run again; the dispatcher
        // measures the message.
        dispatch_message(bot, msg, pool, dialogue, localization, extractor).await
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deduplication::{RequestDeduplicator, SharedDeduplicator};
    use teloxide::types::{ChatId, MessageId};

    #[test]
    fn is_duplicate_message_without_deduplicator_processes_every_message() -> Result<()> {
        assert!(!is_duplicate_message(None, ChatId(1), MessageId(1))?);
        assert!(!is_duplicate_message(None, ChatId(1), MessageId(1))?);
        Ok(())
    }

    #[test]
    fn is_duplicate_message_accepts_first_delivery_and_rejects_redelivery() -> Result<()> {
        let dedup: SharedDeduplicator = Arc::new(RequestDeduplicator::new(60, 100));
        assert!(!is_duplicate_message(
            Some(&dedup),
            ChatId(1),
            MessageId(7)
        )?);
        assert!(is_duplicate_message(Some(&dedup), ChatId(1), MessageId(7))?);
        assert!(!is_duplicate_message(
            Some(&dedup),
            ChatId(1),
            MessageId(8)
        )?);
        Ok(())
    }
}
