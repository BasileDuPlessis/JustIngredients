//! The dispatcher's handler tree: groups and channels get a "private chats" answer; private
//! messages and buttons go to their handlers, with the chat's running action
//! (`RecipeDialogue`). Built here rather than in `main.rs`, so that tests build the same tree.

use std::sync::Arc;

use teloxide::dispatching::UpdateHandler;
use teloxide::prelude::*;

use crate::dialogue::RecipeDialogue;
use crate::dialogue_storage::PgDialogueStorage;

/// What the handlers need, built once at startup.
#[derive(Clone)]
pub struct BotServices {
    pub messages: super::MessageServices,
    pub dialogues: Arc<PgDialogueStorage>,
}

/// The tree given to `Dispatcher::builder`.
pub fn handler_tree(services: BotServices) -> UpdateHandler<anyhow::Error> {
    dptree::entry()
        // Recipes belong to one person: groups and channels only get a "private chats" answer
        .branch(
            Update::filter_message()
                .filter(|msg: Message| !msg.chat.is_private())
                .endpoint({
                    let localization = Arc::clone(&services.messages.localization);
                    move |bot: Bot, msg: Message| {
                        let localization = Arc::clone(&localization);
                        async move {
                            super::command_handlers::handle_group_message(&bot, &msg, &localization)
                                .await
                        }
                    }
                }),
        )
        .branch(
            Update::filter_callback_query()
                .filter(|q: CallbackQuery| {
                    q.message.as_ref().is_some_and(|m| !m.chat().is_private())
                })
                .endpoint({
                    let localization = Arc::clone(&services.messages.localization);
                    move |bot: Bot, q: CallbackQuery| {
                        let localization = Arc::clone(&localization);
                        async move {
                            let text = crate::localization::t_lang(
                                &localization,
                                "private-chat-only",
                                q.from.language_code.as_deref(),
                            );
                            bot.answer_callback_query(q.id).text(text).await?;
                            anyhow::Ok(())
                        }
                    }
                }),
        )
        .branch(Update::filter_message().endpoint({
            let storage = Arc::clone(&services.dialogues);
            let messages = services.messages.clone();
            move |bot: Bot, msg: Message| {
                let dialogue = RecipeDialogue::new(storage.clone(), msg.chat.id);
                let messages = messages.clone();
                async move { super::message_handler_with_cache(bot, msg, dialogue, messages).await }
            }
        }))
        .branch(Update::filter_callback_query().endpoint({
            let pool = Arc::clone(&services.messages.pool);
            let storage = Arc::clone(&services.dialogues);
            let localization = Arc::clone(&services.messages.localization);
            move |bot: Bot, q: CallbackQuery| {
                let pool = Arc::clone(&pool);
                let storage = storage.clone();
                let localization = Arc::clone(&localization);
                // Use the chat ID from the original message that contained the inline keyboard
                let chat_id = match &q.message {
                    Some(msg) => match msg {
                        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
                        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
                            ChatId::from(q.from.id)
                        }
                    },
                    None => ChatId::from(q.from.id),
                };
                let dialogue = RecipeDialogue::new(storage, chat_id);
                async move {
                    super::callback_handler_with_cache(bot, q, pool, dialogue, localization).await
                }
            }
        }))
}
