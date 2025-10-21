//! UI Builder module for creating keyboards and formatting messages

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Import localization
use crate::localization::t_lang;
use std::sync::Arc;

// Import text processing types
use crate::text_processing::MeasurementMatch;

// Import common UI components
use super::ui_components::{
    create_add_button, create_back_button, create_localized_button_with_emoji, create_pagination_buttons, truncate_text, with_ui_metrics_sync,
};

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

            let measurement_display = if let Some(ref unit) = ingredient.measurement {
                format!("{} {}", ingredient.quantity, unit)
            } else {
                ingredient.quantity.clone()
            };

            result.push_str(&format!(
                "{}. **{}** → {}\n",
                i + 1,
                measurement_display,
                ingredient_display
            ));
        }

        result
    })
}

/// Create inline keyboard for ingredient review
pub fn create_ingredient_review_keyboard(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync("create_ingredient_review_keyboard", ingredients.len(), || {
        let mut buttons = Vec::new();

        // Create Edit and Delete buttons for each ingredient
        for (i, ingredient) in ingredients.iter().enumerate() {
            let ingredient_display = if ingredient.ingredient_name.is_empty() {
                format!(
                    "❓ {}",
                    t_lang(localization, "unknown-ingredient", language_code)
                )
            } else {
                ingredient.ingredient_name.clone()
            };

            let measurement_display = if let Some(ref unit) = ingredient.measurement {
                format!("{} {}", ingredient.quantity, unit)
            } else {
                ingredient.quantity.clone()
            };

            let display_text = format!("{} → {}", measurement_display, ingredient_display);
            let button_text = truncate_text(&display_text, 20);

            buttons.push(vec![
                InlineKeyboardButton::callback(format!("✏️ {}", button_text), format!("edit_{}", i)),
                InlineKeyboardButton::callback(format!("🗑️ {}", button_text), format!("delete_{}", i)),
            ]);
        }

        // Add Confirm and Cancel buttons at the bottom
        buttons.push(vec![
            create_localized_button_with_emoji(localization, "✅", "review-confirm", "confirm".to_string(), language_code),
            create_localized_button_with_emoji(localization, "❌", "cancel", "cancel_review".to_string(), language_code),
        ]);

        // Add "Add Ingredient" button if we're in editing mode (has more than just confirm/cancel)
        if !ingredients.is_empty() {
            buttons.push(vec![
                create_add_button(localization, "add-ingredient", "add_ingredient".to_string(), language_code)
            ]);
        }

        InlineKeyboardMarkup::new(buttons)
    })
}

/// Create inline keyboard for post-confirmation workflow
pub fn create_post_confirmation_keyboard(
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    with_ui_metrics_sync("create_post_confirmation_keyboard", 0, || {
        let buttons = vec![
            vec![
                create_localized_button_with_emoji(
                    localization,
                    "➕",
                    "workflow-add-another",
                    "workflow_add_another".to_string(),
                    language_code,
                ),
                create_localized_button_with_emoji(
                    localization,
                    "📚",
                    "workflow-list-recipes",
                    "workflow_list_recipes".to_string(),
                    language_code,
                ),
            ],
            vec![create_localized_button_with_emoji(
                localization,
                "🔍",
                "workflow-search-recipes",
                "workflow_search_recipes".to_string(),
                language_code,
            )],
        ];

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
                format!("select_recipe:{}", recipe_name),
            )]);
        }

        // Calculate total pages
        let total_pages = (total_count as usize).div_ceil(limit as usize);

        // Add navigation buttons if there are multiple pages
        if total_pages > 1 {
            let nav_buttons = create_pagination_buttons(localization, current_page, total_pages, language_code);
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
    with_ui_metrics_sync("create_recipe_instances_keyboard", recipe_data.len(), || {
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
    })
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
