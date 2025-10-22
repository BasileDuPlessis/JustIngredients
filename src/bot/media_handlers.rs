//! Media Handlers module for processing photo and document messages

use anyhow::Result;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::debug;

// Import localization
use crate::localization::t_lang;

// Import dialogue types
use crate::dialogue::RecipeDialogue;

// Import image processing functions
use super::image_processing::{download_and_process_image, ImageProcessingParams};

// Import HandlerContext
// use super::HandlerContext;

// Import observability
// use crate::observability;

/// Handle photo messages
pub async fn handle_photo_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    debug!(user_id = %msg.chat.id, "Received photo message from user");

    // Record user engagement metric for photo upload
    if let Some(user) = msg.from.as_ref() {
        crate::observability::record_user_engagement_metrics(
            user.id.0 as i64,
            crate::observability::UserAction::PhotoUpload,
            None, // No session duration for individual actions
            language_code,
        );
    }

    if let Some(photos) = msg.photo() {
        if let Some(largest_photo) = photos.last() {
            // Extract caption if present - this will be used as recipe name candidate
            // PHOTO CAPTION FEATURE: Captions provide automatic recipe naming for better UX
            let caption = msg.caption().map(|s| s.to_string());

            let _temp_path = download_and_process_image(
                bot,
                ImageProcessingParams {
                    file_id: largest_photo.file.id.clone(),
                    chat_id: msg.chat.id,
                    success_message: &t_lang(localization, "processing-photo", language_code),
                    language_code,
                    dialogue,
                    pool,
                    caption,
                },
                localization,
            )
            .await;
        }
    }
    Ok(())
}

/// Handle document messages
pub async fn handle_document_message(
    bot: &Bot,
    msg: &Message,
    dialogue: RecipeDialogue,
    pool: Arc<PgPool>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Result<()> {
    // Extract user's language code from Telegram
    let language_code = msg
        .from
        .as_ref()
        .and_then(|user| user.language_code.as_ref())
        .map(|s| s.as_str());

    if let Some(doc) = msg.document() {
        if let Some(mime_type) = &doc.mime_type {
            if mime_type.to_string().starts_with("image/") {
                debug!(user_id = %msg.chat.id, mime_type = %mime_type, "Received image document from user");

                // Record user engagement metric for document upload
                if let Some(user) = msg.from.as_ref() {
                    crate::observability::record_user_engagement_metrics(
                        user.id.0 as i64,
                        crate::observability::UserAction::DocumentUpload,
                        None, // No session duration for individual actions
                        language_code,
                    );
                }

                let _temp_path = download_and_process_image(
                    bot,
                    ImageProcessingParams {
                        file_id: doc.file.id.clone(),
                        chat_id: msg.chat.id,
                        success_message: &t_lang(
                            localization,
                            "processing-document",
                            language_code,
                        ),
                        language_code,
                        dialogue,
                        pool,
                        caption: None, // Documents don't have captions like photos do
                    },
                    localization,
                )
                .await;
            } else {
                debug!(user_id = %msg.chat.id, mime_type = %mime_type, "Received non-image document from user");
                bot.send_message(
                    msg.chat.id,
                    t_lang(localization, "error-unsupported-format", language_code),
                )
                .await?;
            }
        } else {
            debug!(user_id = %msg.chat.id, "Received document without mime type from user");
            bot.send_message(
                msg.chat.id,
                t_lang(localization, "error-no-mime-type", language_code),
            )
            .await?;
        }
    }
    Ok(())
}
