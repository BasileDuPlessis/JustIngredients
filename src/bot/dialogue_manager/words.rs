//! The confirm and cancel words understood when typed.

/// Check if input is a cancellation command
/// Words that cancel the running prompt, in English and French (input already lowercased).
pub(crate) fn is_cancellation_command(input: &str) -> bool {
    matches!(
        input,
        "cancel" | "stop" | "back" | "annuler" | "annule" | "retour" | "arrêter" | "arreter"
    )
}

/// Words that confirm a review typed instead of the ✅ button (input already lowercased).
pub(super) fn is_confirmation_word(input: &str) -> bool {
    matches!(
        input,
        "confirm"
            | "ok"
            | "yes"
            | "save"
            | "valider"
            | "valide"
            | "confirmer"
            | "oui"
            | "enregistrer"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_and_confirm_words_are_understood_in_french_and_english() {
        for word in [
            "cancel", "stop", "back", "annuler", "annule", "retour", "arrêter", "arreter",
        ] {
            assert!(is_cancellation_command(word), "{word}");
        }
        for word in [
            "confirm",
            "ok",
            "yes",
            "save",
            "valider",
            "valide",
            "confirmer",
            "oui",
            "enregistrer",
        ] {
            assert!(is_confirmation_word(word), "{word}");
        }
        for word in ["farine", "annulé ce matin", "200 g"] {
            assert!(
                !is_cancellation_command(word) && !is_confirmation_word(word),
                "{word}"
            );
        }
    }
}
