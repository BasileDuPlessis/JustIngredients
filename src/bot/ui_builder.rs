//! UI Builder module for creating keyboards and formatting messages

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Import localization
use crate::localization::{t_args_lang, t_lang};
use std::sync::Arc;

// Import text processing types
use crate::dialogue::IngredientField;
use crate::text_processing::MeasurementMatch;

// Import common UI components
use super::html::Html;
use super::ui_components::{create_add_button, create_localized_button_with_emoji, truncate_text};

/// Quantity and unit as shown to the user ("150 g", "2", "pincée"), empty when the
/// ingredient has neither (e.g. "sel").
/// How a review or editing list names units: as the card's "as in the recipe" view does (the
/// units are kept canonical, "tsp", whatever the page printed): in French ("1 cuil. à café") for
/// a French-speaking user and a recipe that is not English, in English otherwise. The recipe's
/// language is told from the ingredient names (2026-11-06: French recipes showed "tsp").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListUnits {
    french_user: bool,
    english_recipe: bool,
}

impl ListUnits {
    pub(crate) fn of(ingredients: &[MeasurementMatch], language_code: Option<&str>) -> Self {
        Self {
            french_user: crate::engine::Lang::from_code(language_code).is_french(),
            english_recipe: crate::unit_view::recipe_is_english(
                "",
                ingredients
                    .iter()
                    .map(|ingredient| ingredient.ingredient_name.as_str()),
            ),
        }
    }
}

fn measurement_text(ingredient: &MeasurementMatch, units: ListUnits) -> String {
    let quantity_text = ingredient.quantity.trim();
    let quantity = crate::validation::parse_quantity(quantity_text);
    // A quantity that does not read as a number is shown as it is
    if quantity.is_none() && !quantity_text.is_empty() {
        return [
            quantity_text,
            ingredient.measurement.as_deref().unwrap_or("").trim(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    }
    crate::unit_view::show_measure(
        quantity,
        ingredient.measurement.as_deref(),
        &ingredient.ingredient_name,
        crate::unit_view::UnitView::Recipe,
        units.french_user,
        units.english_recipe,
    )
    .text
}

/// One ingredient as shown in the review list, on its buttons and in the edit prompt:
/// "150 g → farine", "½ cuil. à café → sel", or just "sel" without quantity, with ⚠️ when the
/// quantity must be checked.
pub(crate) fn ingredient_line(
    ingredient: &MeasurementMatch,
    name: &str,
    units: ListUnits,
) -> String {
    let warning = if ingredient.requires_quantity_confirmation {
        "⚠️ "
    } else {
        ""
    };
    let measurement = measurement_text(ingredient, units);
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
    let mut result = String::new();
    let units = ListUnits::of(ingredients, language_code);

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
            ingredient_line(ingredient, &ingredient_display, units)
        ));
    }

    fit_lines(&result, MAX_LIST_CHARS)
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
        + Html::text(&format_review_list(
            ingredients,
            language_code,
            localization,
        ))
}

/// The review list: numbered lines, grouped by part when the photo has several (UX plan U9),
/// each part under its title and the whole under a line saying they become linked recipes.
pub fn format_review_list(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    let parts = crate::recipe_parts::split_parts(ingredients);
    if parts.len() < 2 {
        return format_ingredients_list(ingredients, language_code, localization);
    }
    let units = ListUnits::of(ingredients, language_code);
    let mut result = t_args_lang(
        localization,
        "review-parts-intro",
        &[("count", &parts.len().to_string())],
        language_code,
    );
    for (index, part) in parts.iter().enumerate() {
        let title = match (index, part.heading.as_deref()) {
            (0, None) => t_lang(localization, "review-part-main", language_code),
            (0, Some(heading)) => t_args_lang(
                localization,
                "review-part-main-heading",
                &[("heading", heading)],
                language_code,
            ),
            (_, heading) => t_args_lang(
                localization,
                "review-part-linked",
                &[("heading", heading.unwrap_or_default())],
                language_code,
            ),
        };
        result.push_str(&format!("\n\n{title}"));
        for &line in &part.lines {
            let ingredient = &ingredients[line];
            let name = if ingredient.ingredient_name.is_empty() {
                format!(
                    "❓ {}",
                    t_lang(localization, "unknown-ingredient", language_code)
                )
            } else {
                ingredient.ingredient_name.clone()
            };
            result.push_str(&format!(
                "\n{}. {}",
                line + 1,
                ingredient_line(ingredient, &name, units)
            ));
        }
    }
    fit_lines(&result, MAX_LIST_CHARS)
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

/// The buttons under a note question. Right after a creation: "Passer" alone (the recipe is
/// saved already; it uses the cancel button's data, which ends the action). From a card: "🗑
/// Effacer la note" when there is one, above "❌ Annuler" (back to the card).
pub fn note_keyboard(
    recipe_id: i64,
    creation: bool,
    has_note: bool,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    if creation {
        return InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
            t_lang(localization, "note-skip-button", language_code),
            "cancel_action",
        )]]);
    }
    let mut rows = Vec::new();
    if has_note {
        rows.push(vec![InlineKeyboardButton::callback(
            format!(
                "🗑️ {}",
                t_lang(localization, "note-clear-button", language_code)
            ),
            format!("{CLEAR_NOTE_PREFIX}{recipe_id}"),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback(
        format!("❌ {}", t_lang(localization, "cancel", language_code)),
        "cancel_action",
    )]);
    InlineKeyboardMarkup::new(rows)
}

/// Callback data of "🗑️ Effacer la note" starts with this, then the recipe's id: a button of an
/// older question cannot clear the note of the recipe being asked about now.
pub const CLEAR_NOTE_PREFIX: &str = "clear_recipe_note:";

/// Create inline keyboard for ingredient review
pub fn create_ingredient_review_keyboard(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    // One button per ingredient: it opens the ingredient's menu; in the order of the
    // photo's parts, like the list above
    let parts = crate::recipe_parts::split_parts(ingredients);
    let units = ListUnits::of(ingredients, language_code);
    let mut buttons: Vec<Vec<InlineKeyboardButton>> = parts
        .iter()
        .flat_map(|part| part.lines.iter())
        .filter_map(|&i| ingredients.get(i).map(|ingredient| (i, ingredient)))
        .map(|(i, ingredient)| {
            vec![InlineKeyboardButton::callback(
                ingredient_button_label(ingredient, units, language_code, localization),
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
}

/// Longest ingredient label on a button, in characters (Telegram shortens longer ones itself).
const BUTTON_LABEL_MAX_CHARS: usize = 60;

/// An ingredient as written on its button: the review line, shortened if very long.
fn ingredient_button_label(
    ingredient: &MeasurementMatch,
    units: ListUnits,
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
    truncate_text(
        &ingredient_line(ingredient, &name, units),
        BUTTON_LABEL_MAX_CHARS,
    )
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
                ingredient_button_label(
                    ingredient,
                    ListUnits::of(std::slice::from_ref(ingredient), language_code),
                    language_code,
                    localization
                )
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

/// Linked recipes shown on a card, at most (Telegram limits keyboards).
pub const MAX_LINKED_SHOWN: usize = 10;

/// Callback of a linked recipe's button on a card: `recipe_link:<id>` (UX plan U9).
pub const RECIPE_LINK_PREFIX: &str = "recipe_link:";

/// The card's keyboard: rename and edit, delete, one button per linked recipe (U9), then the
/// list.
pub fn create_recipe_details_keyboard(
    recipe_id: i64,
    linked: &[crate::db::RecipeSummary],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let mut buttons = vec![
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
        vec![
            create_localized_button_with_emoji(
                localization,
                "📌",
                "edit-recipe-note",
                format!("recipe_action:note:{}", recipe_id),
                language_code,
            ),
            create_localized_button_with_emoji(
                localization,
                "🗑️",
                "delete-recipe",
                format!("recipe_action:delete:{}", recipe_id),
                language_code,
            ),
        ],
    ];
    buttons.extend(linked.iter().take(MAX_LINKED_SHOWN).map(|recipe| {
        vec![InlineKeyboardButton::callback(
            format!("🔗 {}", crate::recipe_parts::short_name(&recipe.name, 40)),
            format!("{RECIPE_LINK_PREFIX}{}", recipe.id),
        )]
    }));
    // The single exit of a card shown at the bottom (U6.1)
    buttons.push(vec![recipes_list_button(language_code, localization)]);

    InlineKeyboardMarkup::new(buttons)
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

/// Telegram refuses messages over 4096 characters; a card keeps below this.
const CARD_CHARS: usize = 4000;

/// Room kept for the unit-view footer, whose text is known only after the list is built.
const FOOTER_RESERVE: usize = 200;

/// The recipe card (HTML) of the engine's view of a recipe: bold name, the memo on a line of its
/// own, the ingredients with their measures as the view shows them, then "⚖️ Vue …, valeurs
/// arrondies" when the view converted one (UX plan U8), and the linked recipes. The ingredient
/// list takes what the rest leaves of Telegram's 4096 characters, so a long memo never makes the
/// card fail to show. The measures are already in the user's language (`engine::Lang`).
pub fn render_recipe_card(
    recipe: &crate::engine::recipes::RecipeView,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    let name = recipe.name.as_deref().unwrap_or("Unnamed Recipe");
    let linked_line = (!recipe.linked.is_empty()).then(|| {
        let names: Vec<&str> = recipe
            .linked
            .iter()
            .take(MAX_LINKED_SHOWN)
            .map(|linked| linked.name.as_str())
            .collect();
        // Within Telegram's 4096 characters, after a list of up to 3500
        let names = truncate_text(&names.join(", "), 400);
        t_args_lang(
            localization,
            "card-linked-recipes",
            &[("names", &names)],
            language_code,
        )
    });
    let note_chars = recipe
        .note
        .as_ref()
        .map_or(0, |note| 4 + note.chars().count());
    let linked_chars = linked_line
        .as_ref()
        .map_or(0, |line| 2 + line.chars().count());
    let rest = 3 + name.chars().count() + 2 + note_chars + linked_chars + FOOTER_RESERVE;
    let list_budget = CARD_CHARS.saturating_sub(rest).clamp(500, MAX_LIST_CHARS);

    let (list, converted) = format_shown_ingredients(
        &recipe.ingredients,
        language_code,
        localization,
        list_budget,
    );
    // No creation date: the user asked for the card to show the recipe only (2026-10-30)
    let mut card = Html::text("📖 ") + Html::bold(name);
    if let Some(note) = &recipe.note {
        card += Html::text("\n📌 ") + Html::text(note);
    }
    card += Html::text("\n\n") + Html::text(&list);
    let footer = match recipe.unit_view {
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
    if let Some(line) = linked_line {
        card += Html::text(&format!("\n\n{line}"));
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

/// The ingredients of a card, one "• 180 g farine" line each, their measures as the engine
/// shows them; and whether the view converted one. Lines that do not fit in `max_chars`
/// characters are cut ("…").
pub fn format_shown_ingredients(
    ingredients: &[crate::engine::recipes::ShownIngredient],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
    max_chars: usize,
) -> (String, bool) {
    if ingredients.is_empty() {
        return (
            t_lang(localization, "no-ingredients-found", language_code),
            false,
        );
    }
    let converted = ingredients
        .iter()
        .any(|ingredient| ingredient.shown.converted);
    let lines: Vec<String> = ingredients
        .iter()
        .map(|ingredient| {
            if ingredient.shown.text.is_empty() {
                format!("• {}", ingredient.name)
            } else {
                format!("• {} {}", ingredient.shown.text, ingredient.name)
            }
        })
        .collect();
    (fit_lines(&lines.join("\n"), max_chars), converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipe_card_offers_rename_edit_note_delete_and_the_list() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let keyboard = create_recipe_details_keyboard(7, &[], Some("fr"), &localization);
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
                vec!["recipe_action:note:7", "recipe_action:delete:7"],
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
            section: None,
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
        assert!(
            without_name
                .as_str()
                .starts_with("✏️ <b>Editing recipe</b>\n\n")
        );
    }

    /// A card built as the engine builds it, for a recipe written in French.
    fn card_of(
        name: &str,
        note: Option<&str>,
        ingredients: &[crate::db::Ingredient],
        linked: &[(i64, &str)],
        view: crate::unit_view::UnitView,
        language_code: Option<&str>,
        localization: &Arc<crate::localization::LocalizationManager>,
    ) -> Html {
        let linked: Vec<crate::engine::recipes::RecipeEntry> = linked
            .iter()
            .map(|(id, name)| crate::engine::recipes::RecipeEntry {
                id: *id,
                name: (*name).to_string(),
                note: None,
            })
            .collect();
        let recipe = crate::engine::recipes::build_view(
            crate::engine::recipes::ReadRecipe {
                id: 1,
                name: Some(name),
                note,
                ingredients,
                linked: &linked,
                english_recipe: false,
            },
            view,
            crate::engine::Lang::from_code(language_code),
        );
        render_recipe_card(&recipe, language_code, localization)
    }

    #[test]
    fn a_card_lists_its_linked_recipes_and_opens_them() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let created_at = chrono::Utc::now();
        let sauce = crate::db::RecipeSummary {
            id: 12,
            name: "Malva pudding (sauce)".to_string(),
            created_at,
            note: None,
        };
        let card = card_of(
            "Malva pudding",
            None,
            &[],
            &[(12, "Malva pudding (sauce)")],
            crate::unit_view::UnitView::French,
            Some("fr"),
            &localization,
        );
        assert!(
            card.as_str()
                .ends_with("🔗 Recettes liées : Malva pudding (sauce)"),
            "{}",
            card.as_str()
        );
        let keyboard = create_recipe_details_keyboard(7, &[sauce], Some("fr"), &localization);
        let rows: Vec<(String, String)> = keyboard
            .inline_keyboard
            .iter()
            .map(|row| {
                let data = match &row[0].kind {
                    teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => data.clone(),
                    other => format!("{other:?}"),
                };
                (row[0].text.clone(), data)
            })
            .collect();
        assert_eq!(
            rows[2],
            (
                "🔗 Malva pudding (sauce)".to_string(),
                "recipe_link:12".to_string()
            ),
            "after delete, before the list"
        );
        assert_eq!(rows[3].1, "show_list");
    }

    #[test]
    fn a_photo_in_two_parts_is_reviewed_part_by_part() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let in_part = |name: &str, section: Option<&str>| MeasurementMatch {
            section: section.map(str::to_string),
            ..named(name)
        };
        let ingredients = [
            in_part("sucre", Some("Pour le gâteau")),
            in_part("crème", Some("Pour la sauce")),
            in_part("farine", Some("Pour le gâteau")),
        ];
        let list = format_review_list(&ingredients, Some("fr"), &localization);
        assert!(
            list.starts_with("🧩 Cette photo contient 2 parties"),
            "{list}"
        );
        let gateau = list.find("🧩 La recette : gâteau").expect("first part");
        let sauce = list.find("🧩 Recette liée : sauce").expect("second part");
        assert!(gateau < list.find("3. ").expect("line 3") && list.find("3. ") < Some(sauce));
        let keyboard = create_ingredient_review_keyboard(&ingredients, Some("fr"), &localization);
        let data: Vec<String> = keyboard
            .inline_keyboard
            .iter()
            .map(|row| match &row[0].kind {
                teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => data.clone(),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            data,
            [
                "item_0",
                "item_2",
                "item_1",
                "add_ingredient",
                "confirm",
                "cancel_review"
            ],
            "buttons in the order of the parts"
        );
        let one_part = [named("sucre"), named("crème")];
        let keyboard = create_ingredient_review_keyboard(&one_part, Some("fr"), &localization);
        assert_eq!(keyboard.inline_keyboard.len(), 5);
        assert!(!format_review_list(&one_part, Some("fr"), &localization).contains('🧩'));
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
        let card = card_of(
            "Tarte <b>",
            None,
            std::slice::from_ref(&ingredient),
            &[],
            crate::unit_view::UnitView::French,
            Some("fr"),
            &localization,
        );
        assert_eq!(
            card.as_str(),
            "📖 <b>Tarte &lt;b&gt;</b>\n\n• 20 cl lait &lt;entier&gt; &amp; sucre\n\n⚖️ Vue unités françaises (g, cl), valeurs arrondies"
        );
        let as_saved = card_of(
            "Tarte",
            None,
            &[ingredient],
            &[],
            crate::unit_view::UnitView::Recipe,
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
        let empty = card_of(
            "Vide",
            None,
            &[],
            &[],
            crate::unit_view::UnitView::French,
            Some("en"),
            &localization,
        );
        assert!(
            empty.as_str().ends_with("No Ingredients Detected"),
            "{}",
            empty.as_str()
        );
    }

    fn saved_ingredient(name: &str) -> crate::db::Ingredient {
        let at = chrono::Utc::now();
        crate::db::Ingredient {
            id: 1,
            user_id: 1,
            recipe_id: Some(1),
            name: name.to_string(),
            quantity: Some(2.0),
            unit: Some("dl".to_string()),
            created_at: at,
            updated_at: at,
        }
    }

    #[test]
    fn the_note_shows_under_the_name_and_is_escaped() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let card = card_of(
            "Tarte",
            Some("Four 180 °C <b>25</b> & https://exemple.fr/a?b=1&c=2"),
            &[saved_ingredient("lait")],
            &[],
            crate::unit_view::UnitView::Recipe,
            Some("fr"),
            &localization,
        );
        assert_eq!(
            card.as_str(),
            "📖 <b>Tarte</b>\n📌 Four 180 °C &lt;b&gt;25&lt;/b&gt; &amp; https://exemple.fr/a?b=1&amp;c=2\n\n• 200 ml lait"
        );
    }

    #[test]
    fn a_card_with_the_longest_note_and_a_long_list_fits_in_one_message() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let ingredients: Vec<crate::db::Ingredient> = (0..300)
            .map(|i| {
                saved_ingredient(&format!(
                    "ingrédient numéro {i} avec un nom assez long & précis"
                ))
            })
            .collect();
        let names: Vec<(i64, String)> = (0..12_i64)
            .map(|i| (i, format!("{i} {}", "é".repeat(100))))
            .collect();
        let linked: Vec<(i64, &str)> = names
            .iter()
            .map(|(id, name)| (*id, name.as_str()))
            .collect();
        let note = "é".repeat(crate::validation::MAX_NOTE_CHARS);
        let name = "N".repeat(crate::validation::MAX_RECIPE_NAME_CHARS);
        let card = card_of(
            &name,
            Some(&note),
            &ingredients,
            &linked,
            crate::unit_view::UnitView::French,
            Some("fr"),
            &localization,
        );
        let length = card.as_str().chars().count();
        assert!(length <= 4096, "{length} characters");
        assert!(card.as_str().contains('…'), "the list was cut");
        assert!(card.as_str().contains(&note), "the note is whole");
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
        assert!(
            keyboard
                .inline_keyboard
                .iter()
                .flatten()
                .all(|button| !button.text.contains("Missing translation"))
        );
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

    const ENGLISH: ListUnits = ListUnits {
        french_user: false,
        english_recipe: false,
    };
    const FRENCH: ListUnits = ListUnits {
        french_user: true,
        english_recipe: false,
    };

    #[test]
    fn ingredients_without_quantity_show_only_their_name() {
        assert_eq!(
            ingredient_line(&ingredient("", None, false), "sel", FRENCH),
            "sel"
        );
        assert_eq!(
            ingredient_line(&ingredient("", None, true), "sel", FRENCH),
            "⚠️ sel"
        );
    }

    #[test]
    fn flagged_quantities_start_with_a_warning() {
        assert_eq!(
            ingredient_line(&ingredient("0", Some("cups"), true), "sugar", ENGLISH),
            "⚠️ 0 cups → sugar"
        );
    }

    #[test]
    fn quantities_and_units_are_shown_before_the_name() {
        assert_eq!(
            ingredient_line(&ingredient("150", Some("g"), false), "farine", FRENCH),
            "150 g → farine"
        );
        assert_eq!(
            ingredient_line(&ingredient("2", None, true), "gousse d'ail", FRENCH),
            "⚠️ 2 → gousse d'ail"
        );
        assert_eq!(
            ingredient_line(&ingredient("", Some("pincée"), false), "sel", FRENCH),
            "pincée → sel"
        );
    }

    /// 2026-11-06: the review of a French recipe showed "1 tsp → levure chimique".
    #[test]
    fn spoons_are_named_in_the_recipes_language() {
        let spoon = |quantity: &str, unit: &str| MeasurementMatch {
            ingredient_name: "levure chimique".to_string(),
            ..ingredient(quantity, Some(unit), false)
        };
        assert_eq!(
            ingredient_line(&spoon("1", "tsp"), "levure chimique", FRENCH),
            "1 cuil. à café → levure chimique"
        );
        assert_eq!(
            ingredient_line(&spoon("1", "tbsp"), "lait", FRENCH),
            "1 cuil. à soupe → lait"
        );
        assert_eq!(
            ingredient_line(&spoon("0.5", "tsp"), "sel", FRENCH),
            "½ cuil. à café → sel"
        );
        assert_eq!(
            ingredient_line(&spoon("1", "tsp"), "salt", ENGLISH),
            "1 tsp → salt",
            "an English-speaking user"
        );
        let english_recipe = ListUnits {
            french_user: true,
            english_recipe: true,
        };
        assert_eq!(
            ingredient_line(&spoon("2", "tbsp"), "olive oil", english_recipe),
            "2 tbsp → olive oil",
            "an English recipe keeps its units"
        );
        assert_eq!(
            ingredient_line(&spoon("1-2", "tsp"), "sel", FRENCH),
            "1-2 tsp → sel",
            "a quantity that is not a number is shown as it is"
        );
        // The list decides from the names and the user's language
        let french_list = [spoon("1", "tsp")];
        assert_eq!(ListUnits::of(&french_list, Some("fr")), FRENCH);
        assert!(!ListUnits::of(&french_list, Some("en")).french_user);
    }
}
