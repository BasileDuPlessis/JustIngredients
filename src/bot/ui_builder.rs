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
    create_add_button, create_back_button, create_localized_button_with_emoji,
    create_pagination_buttons, truncate_text, with_ui_metrics_sync,
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

        result
    })
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

            // Add Confirm and Cancel buttons at the bottom
            buttons.push(vec![
                create_localized_button_with_emoji(
                    localization,
                    "✅",
                    "review-confirm",
                    "confirm".to_string(),
                    language_code,
                ),
                create_localized_button_with_emoji(
                    localization,
                    "❌",
                    "cancel",
                    "cancel_review".to_string(),
                    language_code,
                ),
            ]);

            // Add "Add Ingredient" button if we're in editing mode (has more than just confirm/cancel)
            if !ingredients.is_empty() {
                buttons.push(vec![create_add_button(
                    localization,
                    "add-ingredient",
                    "add_ingredient".to_string(),
                    language_code,
                )]);
            }

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

/// Create inline keyboard for OCR processing with cancel option
pub fn create_processing_keyboard(
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync("create_processing_keyboard", 0, || {
        let buttons = vec![vec![create_localized_button_with_emoji(
            localization,
            "❌",
            "cancel",
            "cancel_processing".to_string(),
            language_code,
        )]];

        InlineKeyboardMarkup::new(buttons)
    })
}

/// Create inline keyboard for paginated recipe list
pub fn create_recipes_pagination_keyboard(
    recipes: &[String],
    current_page: usize,
    total_count: i64,
    limit: i64,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync("create_recipes_pagination_keyboard", recipes.len(), || {
        let mut buttons = Vec::new();

        // Add recipe buttons
        for recipe_name in recipes {
            let button_text = truncate_text(recipe_name, 30);
            buttons.push(vec![InlineKeyboardButton::callback(
                button_text,
                crate::bot::recipe_selection::recipe_selection_callback(recipe_name),
            )]);
        }

        // Calculate total pages
        let total_pages = (total_count as usize).div_ceil(limit as usize);

        // Add navigation buttons if there are multiple pages
        if total_pages > 1 {
            let nav_buttons =
                create_pagination_buttons(localization, current_page, total_pages, language_code);
            buttons.push(nav_buttons);
        }

        InlineKeyboardMarkup::new(buttons)
    })
}

/// Create inline keyboard for selecting specific recipe instance from duplicates
pub fn create_recipe_instances_keyboard(
    recipe_data: &[(crate::db::Recipe, Vec<crate::db::Ingredient>)],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync(
        "create_recipe_instances_keyboard",
        recipe_data.len(),
        || {
            let mut buttons = Vec::new();

            // Add buttons for each recipe instance
            for (recipe, ingredients) in recipe_data {
                let created_at = recipe.created_at.format("%b %d, %Y %H:%M");

                // Create ingredient preview (first 3 ingredients)
                let ingredient_preview = if ingredients.is_empty() {
                    t_lang(localization, "no-ingredients-found", language_code)
                } else {
                    let preview_names: Vec<String> = ingredients
                        .iter()
                        .take(3)
                        .map(|ing| ing.name.clone())
                        .collect();
                    preview_names.join(", ")
                };

                let button_text = format!("📅 {} • {}", created_at, ingredient_preview);
                let final_button_text = truncate_text(&button_text, 50);

                buttons.push(vec![InlineKeyboardButton::callback(
                    final_button_text,
                    format!("recipe_instance:{}", recipe.id),
                )]);
            }

            // Add back button
            buttons.push(vec![create_back_button(
                localization,
                "back_to_recipes".to_string(),
                language_code,
            )]);

            InlineKeyboardMarkup::new(buttons)
        },
    )
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
            vec![
                create_localized_button_with_emoji(
                    localization,
                    "🗑️",
                    "delete-recipe",
                    format!("recipe_action:delete:{}", recipe_id),
                    language_code,
                ),
                create_localized_button_with_emoji(
                    localization,
                    "📊",
                    "recipe-statistics",
                    format!("recipe_action:statistics:{}", recipe_id),
                    language_code,
                ),
            ],
            vec![create_back_button(
                localization,
                "back_to_recipes".to_string(),
                language_code,
            )],
        ];

        InlineKeyboardMarkup::new(buttons)
    })
}

/// The recipe card (HTML): bold name, creation date and the saved ingredients.
pub fn render_recipe_card(
    recipe_name: &str,
    created_at: chrono::DateTime<chrono::Utc>,
    ingredients: &[crate::db::Ingredient],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    Html::text("📖 ")
        + Html::bold(recipe_name)
        + Html::text(&format!(
            "\n\n📅 {}\n\n",
            created_at.format("%B %d, %Y at %H:%M")
        ))
        + Html::text(&format_database_ingredients_list(
            ingredients,
            language_code,
            localization,
        ))
}

/// The card of a recipe that was just saved (HTML): ✅, bold name and the ingredients. It stays
/// in the chat, with the recipe card buttons, when the review messages are deleted.
pub fn render_saved_recipe(
    recipe_name: &str,
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> Html {
    Html::text("✅ ")
        + Html::bold(recipe_name)
        + Html::text("\n\n")
        + Html::text(&format_ingredients_list(
            ingredients,
            language_code,
            localization,
        ))
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

/// Format a list of database ingredients for display
pub fn format_database_ingredients_list(
    ingredients: &[crate::db::Ingredient],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    if ingredients.is_empty() {
        return t_lang(localization, "no-ingredients-found", language_code);
    }

    let mut result = String::new();
    for ingredient in ingredients {
        let quantity_text = ingredient
            .quantity
            .map_or(String::new(), |q| format!("{} ", q));
        let unit_text = ingredient.unit.as_deref().unwrap_or("");
        let unit_space = if unit_text.is_empty() { "" } else { " " };
        let line = format!(
            "• {}{}{}{}\n",
            quantity_text, unit_text, unit_space, ingredient.name
        );
        result.push_str(&line);
    }

    result.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            created_at,
            &[ingredient],
            Some("fr"),
            &localization,
        );
        assert_eq!(
            card.as_str(),
            "📖 <b>Tarte &lt;b&gt;</b>\n\n📅 October 27, 2026 at 16:30\n\n• 2 dl lait &lt;entier&gt; &amp; sucre"
        );
        let empty = render_recipe_card("Vide", created_at, &[], Some("en"), &localization);
        assert!(
            empty.as_str().ends_with("No Ingredients Detected"),
            "{}",
            empty.as_str()
        );
    }

    #[test]
    fn the_saved_recipe_card_has_a_check_mark_a_bold_name_and_the_ingredients() {
        let localization =
            crate::localization::create_localization_manager().expect("localization");
        let card =
            render_saved_recipe("Crêpes & co", &[named("farine")], Some("fr"), &localization);
        assert_eq!(
            card.as_str(),
            "✅ <b>Crêpes &amp; co</b>\n\n1. 2 g → farine\n"
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
        assert_eq!(rows.len(), 15 + 2, "15 ingredients, confirm/cancel, add");
        for (i, row) in rows.iter().take(15).enumerate() {
            assert_eq!(row, &vec![format!("item_{i}")]);
        }
        assert_eq!(
            rows[15],
            vec!["confirm".to_string(), "cancel_review".to_string()]
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
