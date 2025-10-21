//! UI Builder module for creating keyboards and formatting messages

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Import localization
use crate::localization::t_lang;
use std::sync::Arc;

// Import text processing types
use crate::text_processing::MeasurementMatch;

/// Format ingredients as a simple numbered list for review
pub fn format_ingredients_list(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> String {
    let start_time = std::time::Instant::now();
    let ingredients_count = ingredients.len();

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

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "format_ingredients_list",
        duration,
        ingredients_count,
        result.lines().count(),
    );

    result
}

/// Create inline keyboard for ingredient review
pub fn create_ingredient_review_keyboard(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let start_time = std::time::Instant::now();
    let ingredients_count = ingredients.len();

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
        // Truncate if too long for button
        let button_text = if display_text.len() > 20 {
            format!("{}...", &display_text[..17])
        } else {
            display_text
        };

        buttons.push(vec![
            InlineKeyboardButton::callback(format!("✏️ {}", button_text), format!("edit_{}", i)),
            InlineKeyboardButton::callback(format!("🗑️ {}", button_text), format!("delete_{}", i)),
        ]);
    }

    // Add Confirm and Cancel buttons at the bottom
    buttons.push(vec![
        InlineKeyboardButton::callback(
            format!(
                "✅ {}",
                t_lang(localization, "review-confirm", language_code)
            ),
            "confirm".to_string(),
        ),
        InlineKeyboardButton::callback(
            format!("❌ {}", t_lang(localization, "cancel", language_code)),
            "cancel_review".to_string(),
        ),
    ]);

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "create_ingredient_review_keyboard",
        duration,
        ingredients_count,
        buttons.len(),
    );

    InlineKeyboardMarkup::new(buttons)
}

/// Create inline keyboard for post-confirmation workflow
pub fn create_post_confirmation_keyboard(
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let start_time = std::time::Instant::now();

    let buttons = vec![
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "➕ {}",
                    t_lang(localization, "workflow-add-another", language_code)
                ),
                "workflow_add_another".to_string(),
            ),
            InlineKeyboardButton::callback(
                format!(
                    "📚 {}",
                    t_lang(localization, "workflow-list-recipes", language_code)
                ),
                "workflow_list_recipes".to_string(),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!(
                "🔍 {}",
                t_lang(localization, "workflow-search-recipes", language_code)
            ),
            "workflow_search_recipes".to_string(),
        )],
    ];

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "create_post_confirmation_keyboard",
        duration,
        0, // No input count for this function
        buttons.len(),
    );

    InlineKeyboardMarkup::new(buttons)
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
    let start_time = std::time::Instant::now();
    let recipes_count = recipes.len();

    let mut buttons = Vec::new();

    // Add recipe buttons
    for recipe_name in recipes {
        // Truncate long recipe names for button display
        let button_text = if recipe_name.len() > 30 {
            format!("{}...", &recipe_name[..27])
        } else {
            recipe_name.clone()
        };

        buttons.push(vec![InlineKeyboardButton::callback(
            button_text,
            format!("select_recipe:{}", recipe_name),
        )]);
    }

    // Calculate total pages
    let total_pages = (total_count as usize).div_ceil(limit as usize);

    // Add navigation buttons if there are multiple pages
    if total_pages > 1 {
        let mut nav_buttons = Vec::new();

        // Previous button
        if current_page > 0 {
            nav_buttons.push(InlineKeyboardButton::callback(
                format!("⬅️ {}", t_lang(localization, "previous", language_code)),
                format!("page:{}", current_page - 1),
            ));
        }

        // Page info (disabled button for display)
        let page_info = format!(
            "{} {} {} {}",
            t_lang(localization, "page", language_code),
            current_page + 1,
            t_lang(localization, "of", language_code),
            total_pages
        );
        nav_buttons.push(InlineKeyboardButton::callback(
            page_info,
            "noop".to_string(), // No-op callback
        ));

        // Next button
        if current_page + 1 < total_pages {
            nav_buttons.push(InlineKeyboardButton::callback(
                format!("{} ➡️", t_lang(localization, "next", language_code)),
                format!("page:{}", current_page + 1),
            ));
        }

        buttons.push(nav_buttons);
    }

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "create_recipes_pagination_keyboard",
        duration,
        recipes_count,
        buttons.len(),
    );

    InlineKeyboardMarkup::new(buttons)
}

/// Create inline keyboard for selecting specific recipe instance from duplicates
pub fn create_recipe_instances_keyboard(
    recipe_data: &[(crate::db::Recipe, Vec<crate::db::Ingredient>)],
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let start_time = std::time::Instant::now();
    let recipes_count = recipe_data.len();

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
        // Truncate if too long for button
        let final_button_text = if button_text.len() > 50 {
            format!("{}...", &button_text[..47])
        } else {
            button_text
        };

        buttons.push(vec![InlineKeyboardButton::callback(
            final_button_text,
            format!("recipe_instance:{}", recipe.id),
        )]);
    }

    // Add back button
    buttons.push(vec![InlineKeyboardButton::callback(
        format!("⬅️ {}", t_lang(localization, "back-to-recipes", language_code)),
        "back_to_recipes".to_string(),
    )]);

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "create_recipe_instances_keyboard",
        duration,
        recipes_count,
        buttons.len(),
    );

    InlineKeyboardMarkup::new(buttons)
}

/// Create inline keyboard for recipe details actions
pub fn create_recipe_details_keyboard(
    recipe_id: i64,
    language_code: Option<&str>,
    localization: &Arc<crate::localization::LocalizationManager>,
) -> InlineKeyboardMarkup {
    let start_time = std::time::Instant::now();

    let buttons = vec![
        vec![
            InlineKeyboardButton::callback(
                format!("✏️ {}", t_lang(localization, "edit-recipe-name", language_code)),
                format!("recipe_action:rename:{}", recipe_id),
            ),
            InlineKeyboardButton::callback(
                format!("🗑️ {}", t_lang(localization, "delete-recipe", language_code)),
                format!("recipe_action:delete:{}", recipe_id),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            format!("⬅️ {}", t_lang(localization, "back-to-recipes", language_code)),
            "back_to_recipes".to_string(),
        )],
    ];

    let duration = start_time.elapsed();
    crate::observability::record_ui_metrics(
        "create_recipe_details_keyboard",
        duration,
        0, // No dynamic content
        buttons.len(),
    );

    InlineKeyboardMarkup::new(buttons)
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
        let quantity_text = ingredient.quantity.map_or(String::new(), |q| format!("{} ", q));
        let unit_text = ingredient.unit.as_deref().unwrap_or("");
        let line = format!("• {}{}{}\n", quantity_text, unit_text, ingredient.name);
        result.push_str(&line);
    }

    result.trim_end().to_string()
}
