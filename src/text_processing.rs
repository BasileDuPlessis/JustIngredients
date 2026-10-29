//! An ingredient line as the bot handles it: read from a photo (`extraction::mapping`), typed
//! by the user (`validation::parse_ingredient_from_text`) or loaded from the database
//! (`ingredient_editing`), then shown in the review, kept in the dialogue and saved.

/// An ingredient line: quantity, unit and name.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeasurementMatch {
    /// The extracted quantity (e.g., "2", "1/2", "500")
    pub quantity: String,
    /// The measurement unit (e.g., "cups", "g", "tablespoons")
    pub measurement: Option<String>,
    /// The extracted ingredient name (e.g., "flour", "de tomates", "all-purpose flour")
    pub ingredient_name: String,
    /// The line number where the measurement was found
    pub line_number: usize,
    /// The starting character position in the line
    pub start_pos: usize,
    /// The ending character position in the line
    pub end_pos: usize,
    /// Whether this measurement requires user confirmation (e.g., missing or absurd quantity)
    pub requires_quantity_confirmation: bool,
}
