//! UI Builder module for creating keyboards and formatting messages

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Import localization
use crate::localization::t_lang;

// Import text processing types
use crate::text_processing::MeasurementMatch;

/// Format ingredients as a simple numbered list for review
pub fn format_ingredients_list(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
) -> String {
    let mut result = String::new();

    for (i, ingredient) in ingredients.iter().enumerate() {
        let ingredient_display = if ingredient.ingredient_name.is_empty() {
            format!("❓ {}", t_lang("unknown-ingredient", language_code))
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
}

/// Create inline keyboard for ingredient review
pub fn create_ingredient_review_keyboard(
    ingredients: &[MeasurementMatch],
    language_code: Option<&str>,
) -> InlineKeyboardMarkup {
    let mut buttons = Vec::new();

    // Create Edit and Delete buttons for each ingredient
    for (i, ingredient) in ingredients.iter().enumerate() {
        let ingredient_display = if ingredient.ingredient_name.is_empty() {
            format!("❓ {}", t_lang("unknown-ingredient", language_code))
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
            format!("✅ {}", t_lang("review-confirm", language_code)),
            "confirm".to_string(),
        ),
        InlineKeyboardButton::callback(
            format!("❌ {}", t_lang("cancel", language_code)),
            "cancel_review".to_string(),
        ),
    ]);

    InlineKeyboardMarkup::new(buttons)
}

/// Create inline keyboard for paginated recipe list
pub fn create_recipes_pagination_keyboard(
    recipes: &[String],
    current_page: usize,
    total_count: i64,
    limit: i64,
    language_code: Option<&str>,
) -> InlineKeyboardMarkup {
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
                format!("⬅️ {}", t_lang("previous", language_code)),
                format!("page:{}", current_page - 1),
            ));
        }

        // Page info (disabled button for display)
        let page_info = format!(
            "{} {} {} {}",
            t_lang("page", language_code),
            current_page + 1,
            t_lang("of", language_code),
            total_pages
        );
        nav_buttons.push(InlineKeyboardButton::callback(
            page_info,
            "noop".to_string(), // No-op callback
        ));

        // Next button
        if current_page + 1 < total_pages {
            nav_buttons.push(InlineKeyboardButton::callback(
                format!("{} ➡️", t_lang("next", language_code)),
                format!("page:{}", current_page + 1),
            ));
        }

        buttons.push(nav_buttons);
    }

    InlineKeyboardMarkup::new(buttons)
}
