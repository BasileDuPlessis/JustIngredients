//! UI Builder module for creating keyboards and formatting messages

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Import localization
use crate::localization::t_lang;
use std::sync::Arc;

// Import text processing types
use crate::dialogue::IngredientField;
use crate::text_processing::MeasurementMatch;

// Import common UI components
use super::html::Html;
use super::ui_components::{
    create_add_button, create_localized_button_with_emoji, truncate_text, with_ui_metrics_sync,
};

/// Quantity and unit as shown to the user ("150 g", "2", "pincée"), empty when the
/// ingredient has neither (e.g. "sel").
fn measurement_text(ingredient: &MeasurementMatch) -> String {
    [
        ingredient.quantity.trim(),
        ingredient.measurement.as_deref().unwrap_or("").trim(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

/// One ingredient as shown in the review list, on its buttons and in the edit prompt:
/// "150 g → farine", or just "sel" without quantity, with ⚠️ when the quantity must be checked.
pub(crate) fn ingredient_line(ingredient: &MeasurementMatch, name: &str) -> String {
    let warning = if ingredient.requires_quantity_confirmation {
        "⚠️ "
    } else {
        ""
    };
    let measurement = measurement_text(ingredient);
    if measurement.is_empty() {
        format!("{warning}{name}")
    } else {
        format!("{warning}{measurement} → {name}")
    }
}

/// Format ingredients as a simple numbered list for review
pub fn format_ingredients_list(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    with_ui_metrics_sync("format_ingredients_list", ingredients.len(), || {
        let mut result = String::new();

        for (i, ingredient) in ingredients.iter().enumerate() {
            let ingredient_display = if ingredient.ingredient_name.is_empty() {
                format!(
                    "❓ {}",
                    t_lang(localization, "unknown-ingredient", language_code)
                )
            } else {
                ingredient.ingredient_name.clone()
            };

            result.push_str(&format!(
                "{}. {}\n",
                i + 1,
                ingredient_line(ingredient, &ingredient_display)
            ));
        }

        fit_lines(&result, MAX_LIST_CHARS)
    })
}

/// Longest list or transcription in one message, in characters: Telegram refuses messages over
/// 4096 characters, and titles and instructions need the rest.
pub const MAX_LIST_CHARS: usize = 3500;

/// Keeps the first lines of `text` that fit in `max_chars` characters, and adds "…" when lines
/// were dropped; a single line longer than the budget is cut. Telegram rejects longer messages
/// (the user would be left with "processing…").
pub fn fit_lines(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let budget = max_chars.saturating_sub(2); // room for "…" and its newline
    let mut kept = String::new();
    let mut used = 0;
    for line in text.lines() {
        let length = line.chars().count() + 1;
        if used + length > budget {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
        used += length;
    }
    if kept.is_empty() {
        kept = text.chars().take(budget).collect();
        kept.push('\n');
    }
    kept.push('…');
    kept
}

/// The review message (HTML): bold title, instructions and the numbered ingredient list.
pub fn render_review(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    Html::text("📝 ")
        + Html::bold(&t_lang(localization, "review-title", language_code))
        + Html::text("\n\n")
        + Html::text(&t_lang(localization, "review-description", language_code))
        + Html::text("\n\n")
        + Html::text(&format_ingredients_list(
            ingredients,
            language_code,
            localization,
        ))
}

/// The editing view of a saved recipe (HTML): bold title (with the recipe name when given),
/// instructions and the ingredient list.
pub fn render_saved_editing(
    recipe_name: Option<&str>,
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    let mut title =
        Html::text("✏️ ") + Html::bold(&t_lang(localization, "editing-recipe", language_code));
    if let Some(name) = recipe_name {
        title += Html::text(": ") + Html::bold(name);
    }
    title
        + Html::text("\n\n")
        + Html::text(&t_lang(localization, "editing-instructions", language_code))
        + Html::text("\n\n")
        + Html::text(&format_ingredients_list(
            ingredients,
            language_code,
            localization,
        ))
}

/// "📚 Liste des recettes": the exit of a bottom message when no action is running; it puts that
/// message into the history and brings the recipe list back.
pub fn recipes_list_button(
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardButton {
    InlineKeyboardButton::callback(
        t_lang(localization, "recipes-list-button", language_code),
        "show_list",
    )
}

/// The single exit of an action's message: "❌ Annuler" (`cancel_action`), which drops the action
/// and goes back one step (U6.2).
pub fn cancel_action_keyboard(
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        format!("❌ {}", t_lang(localization, "cancel", language_code)),
        "cancel_action",
    )]])
}

/// Create inline keyboard for ingredient review
pub fn create_ingredient_review_keyboard(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync(
        "create_ingredient_review_keyboard",
        ingredients.len(),
        || {
            // One button per ingredient: it opens the ingredient's menu
            let mut buttons: Vec<Vec<InlineKeyboardButton>> = ingredients
                .iter()
                .enumerate()
                .map(|(i, ingredient)| {
                    vec![InlineKeyboardButton::callback(
                        ingredient_button_label(ingredient, language_code, localization),
                        format!("item_{i}"),
                    )]
                })
                .collect();

            // ➕, then ✅ Valider, then the single exit ❌ Annuler on the last row (U6.2); ➕ also
            // on an empty list: it is the only way to fill it again
            buttons.push(vec![create_add_button(
                localization,
                "add-ingredient",
                "add_ingredient".to_string(),
                language_code,
            )]);
            buttons.push(vec![create_localized_button_with_emoji(
                localization,
                "✅",
                "review-confirm",
                "confirm".to_string(),
                language_code,
            )]);
            buttons.push(vec![create_localized_button_with_emoji(
                localization,
                "❌",
                "cancel",
                "cancel_review".to_string(),
                language_code,
            )]);

            InlineKeyboardMarkup::new(buttons)
        },
    )
}

/// Longest ingredient label on a button, in characters (Telegram shortens longer ones itself).
const BUTTON_LABEL_MAX_CHARS: usize = 60;

/// An ingredient as written on its button: the review line, shortened if very long.
fn ingredient_button_label(
    ingredient: &MeasurementMatch,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    let name = if ingredient.ingredient_name.is_empty() {
        format!(
            "❓ {}",
            t_lang(localization, "unknown-ingredient", language_code)
        )
    } else {
        ingredient.ingredient_name.clone()
    };
    truncate_text(&ingredient_line(ingredient, &name), BUTTON_LABEL_MAX_CHARS)
}

/// What a button of the ingredient list or of an ingredient's menu asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngredientAction {
    /// Open the menu of ingredient `i` (`item_<i>`).
    Menu(usize),
    /// Edit a field of ingredient `i`: `edit_<i>` (whole line), `qty_<i>`, `name_<i>`.
    Edit(IngredientField, usize),
    /// Delete ingredient `i` (`delete_<i>`).
    Delete(usize),
    /// Back from a menu to the ingredient list (`back_review`).
    Back,
}

/// Reads the callback data of an ingredient button.
pub fn parse_ingredient_action(data: &str) -> Option<IngredientAction> {
    if data == "back_review" {
        return Some(IngredientAction::Back);
    }
    let (prefix, index) = data.split_once('_')?;
    let index: usize = index.parse().ok()?;
    match prefix {
        "item" => Some(IngredientAction::Menu(index)),
        "edit" => Some(IngredientAction::Edit(IngredientField::Line, index)),
        "qty" => Some(IngredientAction::Edit(IngredientField::Quantity, index)),
        "name" => Some(IngredientAction::Edit(IngredientField::Name, index)),
        "delete" => Some(IngredientAction::Delete(index)),
        _ => None,
    }
}

/// Menu of one ingredient: its line (inactive), change quantity, change name, retype the line,
/// delete, back to the list.
pub fn create_ingredient_menu_keyboard(
    ingredient: &MeasurementMatch,
    index: usize,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let button = |key: &str, data: String| {
        InlineKeyboardButton::callback(t_lang(localization, key, language_code), data)
    };
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            format!(
                "👉 {}",
                ingredient_button_label(ingredient, language_code, localization)
            ),
            "noop",
        )],
        vec![
            button("ingredient-menu-quantity", format!("qty_{index}")),
            button("ingredient-menu-name", format!("name_{index}")),
        ],
        vec![
            button("ingredient-menu-line", format!("edit_{index}")),
            button("ingredient-menu-delete", format!("delete_{index}")),
        ],
        vec![button("ingredient-menu-back", "back_review".to_string())],
    ])
}

/// Locale key of the instruction shown when a field of an ingredient is edited.
pub fn edit_instruction_key(field: IngredientField) -> &'static str {
    match field {
        IngredientField::Line => "edit-ingredient-instruction",
        IngredientField::Quantity => "edit-quantity-instruction",
        IngredientField::Name => "edit-name-instruction",
    }
}

/// Create inline keyboard for recipe details actions
pub fn create_recipe_details_keyboard(
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync("create_recipe_details_keyboard", 0, || {
        let buttons = vec![
            vec![
                create_localized_button_with_emoji(
                    localization,
                    "✏️",
                    "edit-recipe-name",
                    format!("recipe_action:rename:{}", recipe_id),
                    language_code,
                ),
                create_localized_button_with_emoji(
                    localization,
                    "📝",
                    "edit-ingredients",
                    format!("recipe_action:edit_ingredients:{}", recipe_id),
                    language_code,
                ),
            ],
            vec![create_localized_button_with_emoji(
                localization,
                "🗑️",
                "delete-recipe",
                format!("recipe_action:delete:{}", recipe_id),
                language_code,
            )],
            // The single exit of a card shown at the bottom (U6.1)
            vec![recipes_list_button(language_code, localization)],
        ];

        InlineKeyboardMarkup::new(buttons)
    })
}

/// Paris time of `at`: UTC+2 from the last Sunday of March to the last Sunday of October (both
/// at 01:00 UTC, the European rule), UTC+1 otherwise. The bot's users are French; no time zone
/// database is needed for one zone.
pub fn paris_time(at: chrono::DateTime<chrono::Utc>) -> chrono::NaiveDateTime {
    use chrono::{Datelike, NaiveDate, TimeZone};
    let last_sunday_at_one_utc = |month: u32| {
        NaiveDate::from_ymd_opt(at.year(), month, 31)
            .map(|last| last - chrono::Days::new(u64::from(last.weekday().num_days_from_sunday())))
            .and_then(|sunday| sunday.and_hms_opt(1, 0, 0))
            .map(|start| chrono::Utc.from_utc_datetime(&start))
    };
    let summer = match (last_sunday_at_one_utc(3), last_sunday_at_one_utc(10)) {
        (Some(start), Some(end)) => at >= start && at < end,
        _ => false,
    };
    (at + chrono::Duration::hours(if summer { 2 } else { 1 })).naive_utc()
}

/// A recipe date in the user's language and Paris time: "27 octobre 2026 à 21:36" or
/// "October 27, 2026 at 21:36"; `short` gives the day only, for list buttons: "27 oct. 2026" or
/// "Oct 27, 2026".
pub fn format_recipe_date(
    at: chrono::DateTime<chrono::Utc>,
    language_code: Option<&str>,
    short: bool,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    use chrono::{Datelike, Timelike};
    const FR_MONTHS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    const FR_SHORT: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    let local = paris_time(at);
    let month = local.month0() as usize;
    let time = format!("{:02}:{:02}", local.hour(), local.minute());
    match (
        crate::localization::detect_language(localization, language_code).as_str(),
        short,
    ) {
        ("fr", false) => format!(
            "{} {} {} à {time}",
            local.day(),
            FR_MONTHS[month],
            local.year()
        ),
        ("fr", true) => format!("{} {} {}", local.day(), FR_SHORT[month], local.year()),
        (_, false) => format!("{} at {time}", local.format("%B %-d, %Y")),
        (_, true) => local.format("%b %-d, %Y").to_string(),
    }
}

/// The recipe card (HTML): bold name, creation date and the saved ingredients with their
/// measures in the user's unit view, then "⚖️ Vue …, valeurs arrondies" when the view converted
/// one (UX plan U8).
pub fn render_recipe_card(
    recipe_name: &str,
    ingredients: &[crate::db::Ingredient],
    view: crate::unit_view::UnitView,
    english_recipe: bool,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    let (list, converted) = format_database_ingredients_list(
        ingredients,
        view,
        english_recipe,
        language_code,
        localization,
    );
    // No creation date: the user asked for the card to show the recipe only (2026-10-30)
    let mut card =
        Html::text("📖 ") + Html::bold(recipe_name) + Html::text("\n\n") + Html::text(&list);
    let footer = match view {
        crate::unit_view::UnitView::Recipe => None,
        crate::unit_view::UnitView::Weight => Some("unit-view-footer-weight"),
        crate::unit_view::UnitView::French => Some("unit-view-footer-french"),
        crate::unit_view::UnitView::Us => Some("unit-view-footer-us"),
    };
    if let Some(footer) = footer.filter(|_| converted) {
        card += Html::text(&format!(
            "\n\n{}",
            t_lang(localization, footer, language_code)
        ));
    }
    card
}

/// The recipe card shown again after its ingredients were edited (HTML): bold name and the
/// ingredient list.
pub fn render_recipe_summary(
    recipe_name: &str,
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    Html::text("📝 ")
        + Html::bold(recipe_name)
        + Html::text("\n\n")
        + Html::text(&format_ingredients_list(
            ingredients,
            language_code,
            localization,
        ))
}

/// The saved ingredients of a card, one "• 180 g farine" line each, measures in `view` (unit
/// names of an English recipe in English in the "as in the recipe" view); and whether the view
/// converted one.
pub fn format_database_ingredients_list(
    ingredients: &[crate::db::Ingredient],
    view: crate::unit_view::UnitView,
    english_recipe: bool,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> (String, bool) {
    if ingredients.is_empty() {
        return (
            t_lang(localization, "no-ingredients-found", language_code),
            false,
        );
    }
    let french = crate::localization::detect_language(localization, language_code) == "fr";
    let mut converted = false;
    let lines: Vec<String> = ingredients
        .iter()
        .map(|ingredient| {
            let measure = crate::unit_view::show_measure(
                ingredient.quantity,
                ingredient.unit.as_deref(),
                &ingredient.name,
                view,
                french,
                english_recipe,
            );
            converted |= measure.converted;
            if measure.text.is_empty() {
                format!("• {}", ingredient.name)
            } else {
                format!("• {} {}", measure.text, ingredient.name)
            }
        })
        .collect();
    (fit_lines(&lines.join("\n"), MAX_LIST_CHARS), converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipe_card_offers_rename_edit_delete_and_the_list() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let keyboard = create_recipe_details_keyboard(7, Some("fr"), &localization);
        let data: Vec<Vec<String>> = keyboard
            .inline_keyboard
            .iter()
            .map(|row| {
                row.iter()
                    .map(|button| match &button.kind {
                        teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => {
                            data.clone()
                        }
                        _ => String::new(),
                    })
                    .collect()
            })
            .collect();
        assert_eq!(
            data,
            vec![
                vec!["recipe_action:rename:7", "recipe_action:edit_ingredients:7"],
                vec!["recipe_action:delete:7"],
                vec!["show_list"],
            ]
        );
    }

    fn ingredient(quantity: &str, unit: Option<&str>, flagged: bool) -> MeasurementMatch {
        MeasurementMatch {
            quantity: quantity.to_string(),
            measurement: unit.map(str::to_string),
            ingredient_name: "x".to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 0,
            requires_quantity_confirmation: flagged,
        }
    }

    fn named(name: &str) -> MeasurementMatch {
        MeasurementMatch {
            ingredient_name: name.to_string(),
            ..ingredient("2", Some("g"), false)
        }
    }

    #[test]
    fn review_message_has_a_bold_title_and_escaped_ingredients() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let html = render_review(&[named("<sel> & poivre")], Some("fr"), &localization);
        let html = html.as_str();
        assert!(
            html.starts_with("📝 <b>Révisez vos ingrédients</b>\n\n"),
            "{html}"
        );
        assert!(html.contains("2 g → &lt;sel&gt; &amp; poivre"), "{html}");
        assert!(!html.contains("<sel>"), "{html}");
    }

    #[test]
    fn saved_editing_view_escapes_the_recipe_name() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let html = render_saved_editing(
            Some("Fish & <Chips>"),
            &[named("frites")],
            Some("en"),
            &localization,
        );
        assert!(
            html.as_str()
                .starts_with("✏️ <b>Editing recipe</b>: <b>Fish &amp; &lt;Chips&gt;</b>\n\n"),
            "{}",
            html.as_str()
        );
        let without_name =
            render_saved_editing(None, &[named("frites")], Some("en"), &localization);
        assert!(without_name
            .as_str()
            .starts_with("✏️ <b>Editing recipe</b>\n\n"));
    }

    #[test]
    fn recipe_card_has_a_bold_escaped_name_and_escaped_ingredients() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-10-27T16:30:00Z")
            .expect("date")
            .with_timezone(&chrono::Utc);
        let ingredient = crate::db::Ingredient {
            id: 1,
            user_id: 1,
            recipe_id: Some(1),
            name: "lait <entier> & sucre".to_string(),
            quantity: Some(2.0),
            unit: Some("dl".to_string()),
            created_at,
            updated_at: created_at,
        };
        let card = render_recipe_card(
            "Tarte <b>",
            std::slice::from_ref(&ingredient),
            crate::unit_view::UnitView::French,
            false,
            Some("fr"),
            &localization,
        );
        assert_eq!(
            card.as_str(),
            "📖 <b>Tarte &lt;b&gt;</b>\n\n• 20 cl lait &lt;entier&gt; &amp; sucre\n\n⚖️ Vue unités françaises (g, cl), valeurs arrondies"
        );
        let as_saved = render_recipe_card(
            "Tarte",
            &[ingredient],
            crate::unit_view::UnitView::Recipe,
            false,
            Some("fr"),
            &localization,
        );
        assert!(
            as_saved
                .as_str()
                .ends_with("• 200 ml lait &lt;entier&gt; &amp; sucre"),
            "a unit saved as dl reads as ml, no footer: {}",
            as_saved.as_str()
        );
        let empty = render_recipe_card(
            "Vide",
            &[],
            crate::unit_view::UnitView::French,
            false,
            Some("en"),
            &localization,
        );
        assert!(
            empty.as_str().ends_with("No Ingredients Detected"),
            "{}",
            empty.as_str()
        );
    }

    #[test]
    fn long_texts_are_cut_at_a_line_and_marked() {
        assert_eq!(fit_lines("a\nb\n", 10), "a\nb\n");
        assert_eq!(fit_lines("aaaa\nbbbb\ncccc\n", 12), "aaaa\nbbbb\n…");
        assert_eq!(
            fit_lines(&"x".repeat(20), 10),
            format!("{}\n…", "x".repeat(8))
        );
    }

    #[test]
    fn a_200_ingredient_review_fits_in_one_telegram_message() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let ingredients: Vec<MeasurementMatch> = (0..200)
            .map(|i| {
                named(&format!(
                    "ingrédient numéro {i} avec un nom assez long & précis"
                ))
            })
            .collect();
        for html in [
            render_review(&ingredients, Some("fr"), &localization),
            render_saved_editing(
                Some("Une très longue recette"),
                &ingredients,
                Some("fr"),
                &localization,
            ),
        ] {
            // The markup is at least as long as the text Telegram counts
            let length = html.as_str().chars().count();
            assert!(length <= 4096, "{length} characters");
            assert!(html.as_str().contains('…'));
        }
    }

    fn utc(text: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(text)
            .expect("date")
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn paris_time_follows_summer_time() {
        let local = |text: &str| paris_time(utc(text)).format("%Y-%m-%d %H:%M").to_string();
        // 2026: summer time from 29 March 01:00 UTC to 25 October 01:00 UTC
        assert_eq!(local("2026-03-29T00:59:00Z"), "2026-03-29 01:59");
        assert_eq!(local("2026-03-29T01:00:00Z"), "2026-03-29 03:00");
        assert_eq!(local("2026-07-14T12:00:00Z"), "2026-07-14 14:00");
        assert_eq!(local("2026-10-25T00:59:00Z"), "2026-10-25 02:59");
        assert_eq!(local("2026-10-25T01:00:00Z"), "2026-10-25 02:00");
        assert_eq!(local("2026-12-31T23:30:00Z"), "2027-01-01 00:30");
    }

    #[test]
    fn recipe_dates_follow_the_user_language() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let at = utc("2026-10-27T20:36:00Z");
        assert_eq!(
            format_recipe_date(at, Some("fr"), false, &localization),
            "27 octobre 2026 à 21:36"
        );
        assert_eq!(
            format_recipe_date(at, Some("fr-FR"), true, &localization),
            "27 oct. 2026"
        );
        assert_eq!(
            format_recipe_date(at, Some("en"), false, &localization),
            "October 27, 2026 at 21:36"
        );
        assert_eq!(
            format_recipe_date(at, None, true, &localization),
            "Oct 27, 2026"
        );
    }

    #[test]
    fn recipe_summary_has_a_bold_escaped_name() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let summary = render_recipe_summary("A & B", &[named("sel")], Some("fr"), &localization);
        assert_eq!(summary.as_str(), "📝 <b>A &amp; B</b>\n\n1. 2 g → sel\n");
    }

    fn callbacks(keyboard: &InlineKeyboardMarkup) -> Vec<Vec<String>> {
        keyboard
            .inline_keyboard
            .iter()
            .map(|row| {
                row.iter()
                    .map(|button| match &button.kind {
                        teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => {
                            data.clone()
                        }
                        other => panic!("unexpected button {other:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_review_has_one_button_per_ingredient_then_the_actions() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let ingredients: Vec<MeasurementMatch> =
            (0..15).map(|i| named(&format!("ingrédient {i}"))).collect();
        let keyboard = create_ingredient_review_keyboard(&ingredients, Some("fr"), &localization);
        let rows = callbacks(&keyboard);
        assert_eq!(rows.len(), 15 + 3, "15 ingredients, add, confirm, cancel");
        for (i, row) in rows.iter().take(15).enumerate() {
            assert_eq!(row, &vec![format!("item_{i}")]);
        }
        assert_eq!(
            rows[15..].to_vec(),
            vec![
                vec!["add_ingredient".to_string()],
                vec!["confirm".to_string()],
                vec!["cancel_review".to_string()],
            ],
            "the single exit ❌ is the last row"
        );
        assert!(rows.iter().flatten().all(|data| data.len() <= 64));
        let label = &keyboard.inline_keyboard[0][0].text;
        assert_eq!(
            label, "2 g → ingrédient 0",
            "full line, no 20-character cut"
        );
    }

    #[test]
    fn long_ingredient_labels_are_shortened() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let keyboard = create_ingredient_review_keyboard(
            &[named(&"farine ".repeat(30))],
            Some("fr"),
            &localization,
        );
        assert!(keyboard.inline_keyboard[0][0].text.chars().count() <= BUTTON_LABEL_MAX_CHARS);
    }

    #[test]
    fn the_ingredient_menu_offers_every_action() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let keyboard =
            create_ingredient_menu_keyboard(&named("farine"), 7, Some("fr"), &localization);
        assert_eq!(
            callbacks(&keyboard),
            vec![
                vec!["noop".to_string()],
                vec!["qty_7".to_string(), "name_7".to_string()],
                vec!["edit_7".to_string(), "delete_7".to_string()],
                vec!["back_review".to_string()],
            ]
        );
        assert!(keyboard.inline_keyboard[0][0].text.contains("farine"));
        assert!(keyboard
            .inline_keyboard
            .iter()
            .flatten()
            .all(|button| !button.text.contains("Missing translation")));
    }

    #[test]
    fn ingredient_buttons_parse_back() {
        use IngredientAction::*;
        assert_eq!(parse_ingredient_action("item_3"), Some(Menu(3)));
        assert_eq!(
            parse_ingredient_action("edit_3"),
            Some(Edit(IngredientField::Line, 3))
        );
        assert_eq!(
            parse_ingredient_action("qty_12"),
            Some(Edit(IngredientField::Quantity, 12))
        );
        assert_eq!(
            parse_ingredient_action("name_0"),
            Some(Edit(IngredientField::Name, 0))
        );
        assert_eq!(parse_ingredient_action("delete_4"), Some(Delete(4)));
        assert_eq!(parse_ingredient_action("back_review"), Some(Back));
        for other in [
            "confirm",
            "cancel_review",
            "add_ingredient",
            "item_x",
            "qty_",
            "noop",
        ] {
            assert_eq!(parse_ingredient_action(other), None, "{other}");
        }
    }

    #[test]
    fn ingredients_without_quantity_show_only_their_name() {
        assert_eq!(ingredient_line(&ingredient("", None, false), "sel"), "sel");
        assert_eq!(
            ingredient_line(&ingredient("", None, true), "sel"),
            "⚠️ sel"
        );
    }

    #[test]
    fn flagged_quantities_start_with_a_warning() {
        assert_eq!(
            ingredient_line(&ingredient("0", Some("cups"), true), "sugar"),
            "⚠️ 0 cups → sugar"
        );
    }

    #[test]
    fn quantities_and_units_are_shown_before_the_name() {
        assert_eq!(
            ingredient_line(&ingredient("150", Some("g"), false), "farine"),
            "150 g → farine"
        );
        assert_eq!(
            ingredient_line(&ingredient("2", None, true), "gousse d'ail"),
            "⚠️ 2 → gousse d'ail"
        );
        assert_eq!(
            ingredient_line(&ingredient("", Some("pincée"), false), "sel"),
            "pincée → sel"
        );
    }
}
