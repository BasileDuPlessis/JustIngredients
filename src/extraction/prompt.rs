//! Extraction prompt and response schema, shared with `scripts/llm_benchmark` so the model is
//! evaluated with exactly what the bot sends. Both are embedded at compile time.

/// Instructions sent with every image.
pub const EXTRACTION_PROMPT: &str = include_str!("../../config/extraction/prompt.md");

/// JSON schema the model response must follow; matches [`super::Extraction`].
pub const RESPONSE_SCHEMA_JSON: &str = include_str!("../../config/extraction/schema.json");

/// Parses [`RESPONSE_SCHEMA_JSON`] for the request body.
///
/// # Errors
///
/// Returns an error if the embedded schema is not valid JSON (caught by the tests below).
pub fn response_schema() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(RESPONSE_SCHEMA_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::Extraction;

    fn required(value: &serde_json::Value) -> Vec<&str> {
        value["required"]
            .as_array()
            .map(|names| names.iter().filter_map(|name| name.as_str()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn schema_is_valid_json_and_lists_the_extraction_fields() -> Result<(), serde_json::Error> {
        let schema = response_schema()?;
        assert_eq!(
            required(&schema),
            vec!["is_recipe", "language", "transcription", "ingredients"]
        );
        assert_eq!(
            required(&schema["properties"]["ingredients"]["items"]),
            vec!["name", "quantity", "unit", "needs_review", "source_text"]
        );
        Ok(())
    }

    #[test]
    fn a_response_following_the_schema_deserializes() -> Result<(), serde_json::Error> {
        let extraction: Extraction = serde_json::from_str(
            r#"{"is_recipe": true, "language": "fr", "transcription": "1 pincée de sel",
                "ingredients": [{"name": "sel", "quantity": null, "unit": null,
                                 "needs_review": false, "source_text": "1 pincée de sel"}]}"#,
        )?;
        assert_eq!(extraction.ingredients[0].name, "sel");
        Ok(())
    }

    #[test]
    fn prompt_states_the_agreed_rules() {
        for rule in [
            "une pincée",
            "Convert cl and dl to ml",
            "needs_review",
            "is_recipe",
        ] {
            assert!(EXTRACTION_PROMPT.contains(rule), "missing rule: {rule}");
        }
    }
}
