//! Callback data of the "open this recipe" buttons.
//!
//! Telegram rejects a button whose callback data exceeds 64 bytes (`BUTTON_DATA_INVALID`), and
//! the whole message with it, so a single long recipe name (e.g. a pasted URL) made its page of
//! the recipe list impossible to display. Names that fit keep the readable
//! `select_recipe:<name>` form (also what older messages carry); longer names are sent as a
//! fingerprint, resolved among the user's own recipe names when the button is pressed.

use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;

/// Telegram's limit on callback data.
pub const MAX_CALLBACK_DATA_BYTES: usize = 64;

const BY_NAME: &str = "select_recipe:";
const BY_FINGERPRINT: &str = "select_recipe_h:";
/// 48 bits of SHA-256: collisions are negligible among one user's recipe names.
const FINGERPRINT_BYTES: usize = 6;

/// Callback data that opens `recipe_name`, always within [`MAX_CALLBACK_DATA_BYTES`].
pub fn recipe_selection_callback(recipe_name: &str) -> String {
    let by_name = format!("{BY_NAME}{recipe_name}");
    if by_name.len() <= MAX_CALLBACK_DATA_BYTES {
        by_name
    } else {
        format!("{BY_FINGERPRINT}{}", recipe_name_fingerprint(recipe_name))
    }
}

/// Short, stable fingerprint of a recipe name (12 hex characters).
pub fn recipe_name_fingerprint(recipe_name: &str) -> String {
    Sha256::digest(recipe_name.as_bytes())
        .iter()
        .take(FINGERPRINT_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `data` comes from a recipe selection button (either form).
pub fn is_recipe_selection(data: &str) -> bool {
    data.starts_with(BY_NAME) || data.starts_with(BY_FINGERPRINT)
}

/// The recipe name a selection button refers to. A fingerprint is looked up among the recipes of
/// `telegram_id` only; `None` when nothing matches (the recipe was renamed or deleted).
///
/// # Errors
///
/// Database errors while resolving a fingerprint.
pub async fn resolve_recipe_selection(
    pool: &PgPool,
    telegram_id: i64,
    data: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(name) = data.strip_prefix(BY_NAME) {
        return Ok(Some(name.to_string()));
    }
    let Some(fingerprint) = data.strip_prefix(BY_FINGERPRINT) else {
        return Ok(None);
    };
    let names = crate::db::get_user_recipe_names(pool, telegram_id).await?;
    Ok(names
        .into_iter()
        .find(|name| recipe_name_fingerprint(name) == fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_keep_the_readable_form() {
        assert_eq!(recipe_selection_callback("Crêpes"), "select_recipe:Crêpes");
    }

    #[test]
    fn long_names_fit_telegram_limit() {
        let url_name =
            "https://www.marmiton.org/recettes/recette_gateau-au-chocolat-fondant-rapide_166352.aspx";
        let data = recipe_selection_callback(url_name);
        assert!(
            data.len() <= MAX_CALLBACK_DATA_BYTES,
            "{} bytes",
            data.len()
        );
        assert_eq!(
            data,
            format!("select_recipe_h:{}", recipe_name_fingerprint(url_name))
        );
        assert!(is_recipe_selection(&data));
    }

    #[test]
    fn the_limit_counts_bytes_not_characters() {
        // 26 "é" = 52 bytes: with the 14-byte prefix the readable form would be 66 bytes.
        let accented = "é".repeat(26);
        assert!(recipe_selection_callback(&accented).starts_with("select_recipe_h:"));
        let fits = "a".repeat(MAX_CALLBACK_DATA_BYTES - BY_NAME.len());
        assert_eq!(recipe_selection_callback(&fits), format!("{BY_NAME}{fits}"));
    }

    #[test]
    fn fingerprints_are_stable_and_distinct() {
        assert_eq!(recipe_name_fingerprint("a"), recipe_name_fingerprint("a"));
        assert_ne!(recipe_name_fingerprint("a"), recipe_name_fingerprint("b"));
        assert_eq!(recipe_name_fingerprint("a").len(), FINGERPRINT_BYTES * 2);
    }

    #[test]
    fn other_callbacks_are_not_selections() {
        assert!(!is_recipe_selection("page:3"));
        assert!(!is_recipe_selection("select_recipes"));
    }
}
