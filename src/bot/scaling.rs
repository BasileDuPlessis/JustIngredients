//! Scaling a recipe's quantities from its card (`specs/tasks/recipe-scaling.md`): "⚖️ Ajuster les
//! quantités" opens a menu that stays open while the user adjusts: ÷ 2 and × 2 of the scale shown
//! (× 2 twice is × 4), the card's quantities updated above at each tap; "Autre…", which asks for
//! one typed answer ("x3", "2/3", "4 oeufs", "20x30 à 30x40", "d24 à d28", "de 4 à 6
//! personnes"), read by `engine::scaling`: a factor applies to the quantities shown, as the
//! buttons do (÷ 2, then "x3" typed: × 3/2); a target (an ingredient's quantity, dishes, servings)
//! to the recipe as saved, which it describes; "↩️ Quantités d'origine"; ◀️ back to the card's
//! buttons. The recipe is never changed.

use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgPool;
use teloxide::prelude::*;
use teloxide::types::MaybeInaccessibleMessage;
use tracing::debug;

use super::cards::Buttons;
use super::cleanup::{self, send_transient};
use super::ui_builder::{SCALE_PREFIX, scale_menu_keyboard};
use crate::dialogue::{RecipeDialogue, RecipeDialogueState};
use crate::engine::scaling::{Ratio, Reason, Scale, ScaleError};
use crate::engine::{EngineError, Owner};
use crate::localization::{LocalizationManager, t_args_lang, t_lang};

/// Whether a button's data is a scaling button.
pub fn is_button(data: &str) -> bool {
    data.starts_with(SCALE_PREFIX)
}

/// A scaling button pressed on a card: the menu, a factor, the question, back.
pub async fn handle_button(
    bot: &Bot,
    q: &teloxide::types::CallbackQuery,
    data: &str,
    pool: &PgPool,
    dialogue: &RecipeDialogue,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    let Some(MaybeInaccessibleMessage::Regular(message)) = &q.message else {
        return Ok(());
    };
    let chat_id = message.chat.id;
    let language_code = q.from.language_code.as_deref();
    let Some(rest) = data.strip_prefix(SCALE_PREFIX) else {
        return Ok(());
    };
    let mut parts = rest.split(':');
    let (Some(action), Some(Ok(recipe_id))) = (parts.next(), parts.next().map(str::parse::<i64>))
    else {
        return Ok(());
    };
    // Only the card at the bottom acts: a history card's button is no longer shown
    if !cleanup::is_active_card(chat_id, message.id).await {
        return Ok(());
    }
    match action {
        "menu" => {
            let current = cleanup::card_scale(chat_id, message.id).await;
            if let Err(e) = bot
                .edit_message_reply_markup(chat_id, message.id)
                .reply_markup(scale_menu_keyboard(
                    recipe_id,
                    current,
                    language_code,
                    localization,
                ))
                .await
            {
                debug!(error = %e, "Could not show the scaling menu");
            }
        }
        // "pick": from the menu, which stays open; "set": from the card's buttons
        "pick" | "set" => {
            let Some(ratio) = parts.next().and_then(Ratio::decode) else {
                return Ok(());
            };
            crate::observability::action(crate::observability::Action::RecipeScaled);
            let scale = (!ratio.is_one()).then(|| Scale::factor(ratio));
            let buttons = if action == "pick" {
                Buttons::ScaleMenu
            } else {
                Buttons::Card
            };
            show(
                bot,
                pool,
                chat_id,
                message.id,
                recipe_id,
                scale.as_ref(),
                buttons,
                language_code,
                localization,
            )
            .await?;
        }
        "back" => {
            let scale = cleanup::card_scale(chat_id, message.id).await;
            let keyboard = super::cards::card_keyboard(
                pool,
                chat_id.0,
                recipe_id,
                scale,
                language_code,
                localization,
            )
            .await;
            if let Err(e) = bot
                .edit_message_reply_markup(chat_id, message.id)
                .reply_markup(keyboard)
                .await
            {
                debug!(error = %e, "Could not put the card's buttons back");
            }
        }
        "ask" => {
            let shown = cleanup::card_scale(chat_id, message.id).await;
            ask(
                bot,
                pool,
                chat_id,
                dialogue,
                recipe_id,
                shown,
                localization,
                language_code,
            )
            .await?
        }
        _ => {}
    }
    Ok(())
}

/// The card shown again at `scale`, in place; a gone recipe says so.
#[allow(clippy::too_many_arguments)]
async fn show(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    card: teloxide::types::MessageId,
    recipe_id: i64,
    scale: Option<&Scale>,
    buttons: Buttons,
    language_code: Option<&str>,
    localization: &Arc<LocalizationManager>,
) -> Result<()> {
    if !super::cards::show_scaled_in_place(
        bot,
        pool,
        chat_id,
        card,
        recipe_id,
        scale,
        buttons,
        language_code,
        localization,
    )
    .await?
    {
        send_transient(
            bot,
            chat_id,
            t_lang(localization, "recipe-not-found", language_code),
        )
        .await?;
    }
    Ok(())
}

/// "Autre…": asks for the scale, typed. The question goes at the bottom with "Annuler" (the card
/// goes into the history, as for a rename); the typed answer is read by `engine::scaling`. A card
/// shown at a scale says that a factor applies to it.
#[allow(clippy::too_many_arguments)]
async fn ask(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    dialogue: &RecipeDialogue,
    recipe_id: i64,
    shown: Option<Ratio>,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    let name = crate::db::read_recipe_with_name(pool, chat_id.0, recipe_id)
        .await?
        .and_then(|recipe| recipe.recipe_name)
        .unwrap_or_default();
    // A running action ends here, as a command would
    if dialogue.get().await?.is_some_and(|state| state.is_action()) {
        dialogue.exit().await?;
    }
    cleanup::delete_lists(bot, chat_id).await;
    let mut text = format!(
        "⚖️ {}\n\n{}",
        t_args_lang(
            localization,
            "scale-prompt",
            &[("name", &name)],
            language_code
        ),
        t_lang(localization, "scale-prompt-examples", language_code)
    );
    if let Some(shown) = shown {
        let french = crate::engine::Lang::from_code(language_code).is_french();
        text.push_str("\n\n");
        text.push_str(&t_args_lang(
            localization,
            "scale-prompt-shown",
            &[("scale", &shown.label(french))],
            language_code,
        ));
    }
    let sent = bot
        .send_message(chat_id, text)
        .reply_markup(teloxide::types::InlineKeyboardMarkup::new(vec![vec![
            teloxide::types::InlineKeyboardButton::callback(
                format!("❌ {}", t_lang(localization, "cancel", language_code)),
                "cancel_action",
            ),
        ]]))
        .await?;
    cleanup::track(chat_id, sent.id).await;
    dialogue
        .update(RecipeDialogueState::ScalingRecipe {
            recipe_id,
            language_code: language_code.map(str::to_string),
            shown: shown.map(Ratio::encode),
        })
        .await?;
    Ok(())
}

/// A typed scale as the card will show it: a factor ("x2", "2/3") times the scale shown, as the
/// menu's buttons; a target ("4 oeufs", dishes, servings) as asked, from the recipe as saved.
fn on_shown(scale: Scale, shown: Option<Ratio>) -> Result<Scale, ScaleError> {
    match (&scale.reason, shown) {
        (Reason::Factor, Some(shown)) => shown
            .times(scale.ratio)
            .map(Scale::factor)
            .ok_or(ScaleError::OutOfRange),
        _ => Ok(scale),
    }
}

/// What the user is told when a typed scale is refused.
fn refusal(
    error: &ScaleError,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> String {
    let examples = t_lang(localization, "scale-prompt-examples", language_code);
    let reason = match error {
        ScaleError::NotUnderstood => t_lang(localization, "scale-not-understood", language_code),
        ScaleError::OutOfRange => t_lang(localization, "scale-out-of-range", language_code),
        ScaleError::NoSuchIngredient(name) => t_args_lang(
            localization,
            "scale-no-such-ingredient",
            &[("name", name)],
            language_code,
        ),
        ScaleError::Ambiguous(names) => t_args_lang(
            localization,
            "scale-ambiguous",
            &[("names", &names.join(", "))],
            language_code,
        ),
        ScaleError::Repeated(name) => t_args_lang(
            localization,
            "scale-repeated",
            &[("name", name)],
            language_code,
        ),
        ScaleError::NoQuantity(name) => t_args_lang(
            localization,
            "scale-no-quantity",
            &[("name", name)],
            language_code,
        ),
        ScaleError::OtherUnit(name) => t_args_lang(
            localization,
            "scale-other-unit",
            &[("name", name)],
            language_code,
        ),
    };
    format!("{reason}\n\n{examples}")
}

/// The typed scale: the card comes back at the bottom, scaled, the menu still open. A factor
/// applies to the scale the card showed (`shown`), a target to the recipe as saved. A scale
/// refused keeps the question open with why; a cancel word does what "Annuler" does.
#[allow(clippy::too_many_arguments)]
pub async fn answer_text(
    bot: &Bot,
    pool: &PgPool,
    chat_id: ChatId,
    dialogue: &RecipeDialogue,
    recipe_id: i64,
    shown: Option<Ratio>,
    text: &str,
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> Result<()> {
    if super::dialogue_manager::is_cancellation_command(&text.trim().to_lowercase()) {
        dialogue.exit().await?;
        super::home::back_to_card(
            bot,
            pool,
            chat_id,
            recipe_id,
            None,
            localization,
            language_code,
        )
        .await;
        return Ok(());
    }
    let owner = Owner::telegram(chat_id.0);
    let asked = crate::engine::recipes::scale_of(pool, owner, recipe_id, text)
        .await
        .and_then(|scale| on_shown(scale, shown).map_err(EngineError::Scale));
    match asked {
        Ok(scale) => {
            crate::dialogue::end_after_write(dialogue).await;
            crate::observability::action(crate::observability::Action::RecipeScaled);
            cleanup::delete_action_messages(bot, chat_id).await;
            super::cards::show_scaled_card(
                bot,
                pool,
                chat_id,
                None,
                recipe_id,
                Some(&scale),
                // Still adjusting: the menu stays open under the card
                Buttons::ScaleMenu,
                language_code,
                localization,
            )
            .await
        }
        Err(EngineError::Scale(error)) => {
            send_transient(bot, chat_id, refusal(&error, localization, language_code)).await?;
            Ok(())
        }
        Err(EngineError::NotFound) => {
            dialogue.exit().await?;
            send_transient(
                bot,
                chat_id,
                t_lang(localization, "recipe-not-found", language_code),
            )
            .await?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
