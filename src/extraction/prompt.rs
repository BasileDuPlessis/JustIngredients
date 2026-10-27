//! Extraction prompt and response schema, shared with `scripts/llm_benchmark` so the model is
//! evaluated with exactly what the bot sends. Both are embedded at compile time.

/// Instructions sent with every image.
pub const EXTRACTION_PROMPT: &str = include_str!("../../config/extraction/prompt.md");

/// JSON schema the model response must follow; matches [`super::Extraction`].
pub const RESPONSE_SCHEMA_JSON: &str = include_str!("../../config/extraction/schema.json");

/// [`RESPONSE_SCHEMA_JSON`] as raw JSON for the request body.
///
/// Kept raw on purpose: parsing it into a `serde_json::Value` would sort the keys, and Gemini
/// writes the response fields in schema order, so the bot would no longer send the schema the
/// benchmark evaluated.
///
/// # Errors
///
/// Returns an error if the embedded schema is not valid JSON (caught by the tests below).
pub fn response_schema() -> Result<Box<serde_json::value::RawValue>, serde_json::Error> {
    serde_json::value::RawValue::from_string(RESPONSE_SCHEMA_JSON.to_string())
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
        let schema: serde_json::Value = serde_json::from_str(response_schema()?.get())?;
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
    fn serialized_schema_keeps_the_file_order() -> Result<(), serde_json::Error> {
        #[derive(serde::Serialize)]
        struct Body {
            schema: Box<serde_json::value::RawValue>,
        }
        let body = serde_json::to_string(&Body {
            schema: response_schema()?,
        })?;
        let position = |key: &str| body.find(&format!("\"{key}\""));
        assert!(position("is_recipe") < position("transcription"));
        assert!(position("transcription") < position("ingredients"));
        assert!(position("quantity") < position("needs_review"));
        assert!(position("is_recipe").is_some() && position("needs_review").is_some());
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
    fn prompt_lists_exactly_the_canonical_units() {
        let line = EXTRACTION_PROMPT
            .lines()
            .find(|line| line.starts_with("- unit: one of "))
            .unwrap_or_default();
        let listed: std::collections::BTreeSet<&str> = line
            .trim_start_matches("- unit: one of ")
            .split(" when it applies")
            .next()
            .unwrap_or_default()
            .split(", ")
            .collect();
        let canonical: std::collections::BTreeSet<&str> = crate::units::CanonicalUnit::ALL
            .iter()
            .map(|unit| unit.as_str())
            .collect();
        assert_eq!(listed, canonical);
    }

    #[test]
    fn prompt_states_the_agreed_rules() {
        for rule in [
            "une pincée",
            "un quart de beurre",
            "Convert cl and dl to ml",
            "needs_review",
            "is_recipe",
        ] {
            assert!(EXTRACTION_PROMPT.contains(rule), "missing rule: {rule}");
        }
    }
}
