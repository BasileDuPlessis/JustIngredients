//! Recipes read from one photo with several sections (UX plan U9, `specs/tasks/linked-recipes.md`):
//! each section becomes a recipe of its own, linked to the others. The first part takes the
//! recipe's name, the others "Name (section)": "Malva pudding" and "Malva pudding (sauce)".

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::text_processing::MeasurementMatch;

/// A photo with more parts than this stays one recipe: a card lists its linked recipes on
/// buttons, and Telegram limits keyboards.
pub const MAX_PARTS: usize = 5;

/// Longest recipe name, in bytes (the limit of `validation::validate_recipe_name`).
const MAX_NAME_BYTES: usize = 255;

/// One part of a reviewed photo: the heading it is printed under, cleaned ("sauce"; `None` for
/// lines under no heading), and the indexes of its lines in the review list, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub heading: Option<String>,
    pub lines: Vec<usize>,
}

/// The parts of a reviewed list: lines grouped by the heading they are printed under, in the
/// order headings first appear; lines under no heading come first (they belong to the dish
/// itself). One part, the whole list, under fewer than two headings or more than
/// [`MAX_PARTS`].
pub fn split_parts(ingredients: &[MeasurementMatch]) -> Vec<Part> {
    let mut groups: Vec<(Option<&str>, Vec<usize>)> = Vec::new();
    for (index, ingredient) in ingredients.iter().enumerate() {
        let heading = ingredient.section.as_deref();
        match groups.iter_mut().find(|(group, _)| *group == heading) {
            Some((_, lines)) => lines.push(index),
            None => groups.push((heading, vec![index])),
        }
    }
    if let Some(unheaded) = groups.iter().position(|(heading, _)| heading.is_none()) {
        let group = groups.remove(unheaded);
        groups.insert(0, group);
    }
    if groups.len() < 2 || groups.len() > MAX_PARTS {
        return vec![Part {
            heading: None,
            lines: (0..ingredients.len()).collect(),
        }];
    }
    groups
        .into_iter()
        .map(|(heading, lines)| Part {
            heading: heading.map(section_title),
            lines,
        })
        .collect()
}

/// A heading as a part of a name: "Pour la sauce :" → "sauce", "For the frosting" →
/// "frosting", "Pour la crème Chantilly" → "crème Chantilly" (first letter lowercased, the
/// rest as printed; a heading printed in capitals is lowercased: "POUR LE GLAÇAGE" →
/// "glaçage").
pub fn section_title(heading: &str) -> String {
    let heading = heading.split_whitespace().collect::<Vec<_>>().join(" ");
    let heading = heading.trim_end_matches([':', ' ']);
    const PREFIXES: [&str; 8] = [
        "pour le ",
        "pour la ",
        "pour les ",
        "pour l'",
        "pour l’",
        "pour ",
        "for the ",
        "for ",
    ];
    let lowercase = heading.to_lowercase();
    let title = PREFIXES
        .iter()
        .find(|prefix| lowercase.starts_with(*prefix) && lowercase.len() > prefix.len())
        // The prefixes are ASCII but for "’": the same length in the original and lowercase
        .and_then(|prefix| heading.get(prefix.len()..))
        .unwrap_or(heading)
        .trim();
    let capitals = title.chars().any(char::is_alphabetic)
        && !title.chars().any(|c| c.is_alphabetic() && c.is_lowercase());
    if capitals {
        return title.to_lowercase();
    }
    let mut chars = title.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => heading.to_lowercase(),
    }
}

/// The name of a linked part: "Malva pudding (sauce)", the recipe's name shortened if needed to
/// keep within the name limit.
pub fn part_name(recipe_name: &str, heading: &str) -> String {
    let suffix = format!(" ({heading})");
    if suffix.len() >= MAX_NAME_BYTES {
        return recipe_name.to_string();
    }
    let mut base = recipe_name.to_string();
    while base.len() + suffix.len() > MAX_NAME_BYTES {
        base.pop();
    }
    format!("{}{suffix}", base.trim_end())
}

/// The names of the recipes a reviewed photo becomes, in the order of [`split_parts`]: the
/// recipe's name, then "Name (section)" for every other part.
pub fn part_names(recipe_name: &str, parts: &[Part]) -> Vec<String> {
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| match (index, part.heading.as_deref()) {
            (0, _) | (_, None) => recipe_name.to_string(),
            (_, Some(heading)) => part_name(recipe_name, heading),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, section: Option<&str>) -> MeasurementMatch {
        MeasurementMatch {
            quantity: "1".to_string(),
            measurement: None,
            ingredient_name: name.to_string(),
            line_number: 0,
            start_pos: 0,
            end_pos: 0,
            requires_quantity_confirmation: false,
            section: section.map(str::to_string),
        }
    }

    #[test]
    fn a_photo_with_two_sections_gives_two_parts() {
        let malva = [
            line("sucre", Some("Pour le gâteau")),
            line("œufs", Some("Pour le gâteau")),
            line("crème", Some("Pour la sauce :")),
            line("beurre", Some("Pour la sauce :")),
        ];
        let parts = split_parts(&malva);
        assert_eq!(
            parts,
            [
                Part {
                    heading: Some("gâteau".to_string()),
                    lines: vec![0, 1]
                },
                Part {
                    heading: Some("sauce".to_string()),
                    lines: vec![2, 3]
                },
            ]
        );
        assert_eq!(
            part_names("Malva pudding", &parts),
            ["Malva pudding", "Malva pudding (sauce)"]
        );
    }

    #[test]
    fn lines_under_no_heading_are_the_dish_itself() {
        let tart = [
            line("farine", None),
            line("beurre", None),
            line("crème", Some("Garniture")),
            line("sel", None),
        ];
        let parts = split_parts(&tart);
        assert_eq!(parts[0].heading, None);
        assert_eq!(parts[0].lines, [0, 1, 3]);
        assert_eq!(parts[1].lines, [2]);
        assert_eq!(part_names("Tarte", &parts), ["Tarte", "Tarte (garniture)"]);
    }

    #[test]
    fn one_part_without_two_sections_or_with_too_many() {
        let plain = [line("farine", None), line("sel", None)];
        assert_eq!(split_parts(&plain).len(), 1);
        let one_heading = [line("farine", Some("Pâte")), line("sel", Some("Pâte"))];
        assert_eq!(split_parts(&one_heading).len(), 1);
        assert_eq!(split_parts(&[]).len(), 1);
        let many: Vec<MeasurementMatch> = (0..=MAX_PARTS)
            .map(|n| line("x", Some(&format!("Partie {n}"))))
            .collect();
        let parts = split_parts(&many);
        assert_eq!(parts.len(), 1, "more than {MAX_PARTS} parts: one recipe");
        assert_eq!(parts[0].lines.len(), MAX_PARTS + 1);
    }

    #[test]
    fn headings_are_cleaned_into_name_parts() {
        assert_eq!(section_title("Pour la sauce :"), "sauce");
        assert_eq!(
            section_title("POUR LE GLAÇAGE"),
            "glaçage",
            "printed in capitals"
        );
        assert_eq!(section_title("For the frosting"), "frosting");
        assert_eq!(section_title("Pour l'appareil"), "appareil");
        assert_eq!(section_title("Pour la crème Chantilly"), "crème Chantilly");
        assert_eq!(section_title("Sauce"), "sauce");
        assert_eq!(section_title("Pour"), "pour", "nothing after the prefix");
        assert_eq!(section_title("  Garniture  :  "), "garniture");
    }

    #[test]
    fn long_names_are_shortened_to_fit() {
        let name = part_name(&"é".repeat(200), "sauce");
        assert!(name.len() <= MAX_NAME_BYTES, "{}", name.len());
        assert!(name.ends_with(" (sauce)"));
    }
}
