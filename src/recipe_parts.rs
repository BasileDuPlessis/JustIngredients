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

/// Lines of a photo grouped by part, with the heading of each part as first printed.
struct Group<'a> {
    key: Option<String>,
    printed: Option<&'a str>,
    lines: Vec<usize>,
}

/// What a heading is grouped by: its cleaned title, case and accents folded ("Pour la sauce",
/// "Pour la sauce :" and "POUR LA SAUCE" are one part); `None` for a blank one.
fn heading_key(heading: &str) -> Option<String> {
    let key = crate::search::fold(&section_title(heading))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!key.is_empty()).then_some(key)
}

/// Lines under no heading before the first one are the dish itself; one after a heading joins
/// the part above it.
fn groups(ingredients: &[MeasurementMatch]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut previous: Option<usize> = None;
    for (index, ingredient) in ingredients.iter().enumerate() {
        let printed = ingredient.section.as_deref();
        let key = printed.and_then(heading_key);
        let group = match (&key, previous) {
            (None, Some(previous)) => previous,
            _ => match groups.iter().position(|group| group.key == key) {
                Some(group) => group,
                None => {
                    groups.push(Group {
                        key,
                        printed: printed.filter(|_| ingredient.section.is_some()),
                        lines: Vec::new(),
                    });
                    groups.len() - 1
                }
            },
        };
        groups[group].lines.push(index);
        previous = Some(group);
    }
    groups
}

/// The parts of a reviewed list ([`groups`]): in the order headings first appear, the dish
/// itself first. One part, the whole list, for fewer than two parts or more than
/// [`MAX_PARTS`].
pub fn split_parts(ingredients: &[MeasurementMatch]) -> Vec<Part> {
    let groups = groups(ingredients);
    if groups.len() < 2 || groups.len() > MAX_PARTS {
        return vec![Part {
            heading: None,
            lines: (0..ingredients.len()).collect(),
        }];
    }
    groups
        .into_iter()
        .map(|group| Part {
            heading: group.key.as_ref().and(group.printed).map(section_title),
            lines: group.lines,
        })
        .collect()
}

/// A photo's lines in the order of its parts, each under its part's heading as first printed;
/// without parts, under none. The review then numbers lines in order, and a line added at the
/// end joins the last part.
pub fn arrange(ingredients: Vec<MeasurementMatch>) -> Vec<MeasurementMatch> {
    let parts: Vec<(Option<String>, Vec<usize>)> = {
        let groups = groups(&ingredients);
        if groups.len() < 2 || groups.len() > MAX_PARTS {
            Vec::new()
        } else {
            groups
                .into_iter()
                .map(|group| {
                    (
                        group.key.as_ref().and(group.printed).map(str::to_string),
                        group.lines,
                    )
                })
                .collect()
        }
    };
    if parts.is_empty() {
        return ingredients
            .into_iter()
            .map(|ingredient| MeasurementMatch {
                section: None,
                ..ingredient
            })
            .collect();
    }
    let mut slots: Vec<Option<MeasurementMatch>> = ingredients.into_iter().map(Some).collect();
    parts
        .into_iter()
        .flat_map(|(heading, lines)| {
            lines
                .into_iter()
                .map(move |line| (heading.clone(), line))
                .collect::<Vec<_>>()
        })
        .filter_map(|(heading, line)| {
            slots
                .get_mut(line)
                .and_then(Option::take)
                .map(|ingredient| MeasurementMatch {
                    section: heading,
                    ..ingredient
                })
        })
        .collect()
}

/// A heading as a part of a name: "Pour la sauce :" → "sauce", "For the frosting" →
/// "frosting", "Pour la crème Chantilly" → "crème Chantilly" (first letter lowercased, the
/// rest as printed; a heading printed in capitals is lowercased: "POUR LE GLAÇAGE" →
/// "glaçage"; an acronym is kept: "BBQ sauce").
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
    const ARTICLES: [&str; 6] = ["le", "la", "les", "l'", "l’", "the"];
    let lowercase = heading.to_lowercase();
    let title = PREFIXES
        .iter()
        .find(|prefix| lowercase.starts_with(*prefix) && lowercase.len() > prefix.len())
        // The prefixes are ASCII but for "’": the same length in the original and lowercase
        .and_then(|prefix| heading.get(prefix.len()..))
        .map(str::trim)
        // "Pour la": nothing after the article, the heading stays whole
        .filter(|title| !ARTICLES.contains(&title.to_lowercase().as_str()))
        .unwrap_or(heading)
        .trim();
    let capitals = title.chars().any(char::is_alphabetic)
        && !title.chars().any(|c| c.is_alphabetic() && c.is_lowercase());
    if capitals {
        return title.to_lowercase();
    }
    let mut chars = title.chars();
    match chars.next() {
        // "BBQ sauce": an acronym first, kept
        Some(_) if chars.clone().next().is_some_and(char::is_uppercase) => title.to_string(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => heading.to_lowercase(),
    }
}

/// A name shortened to `max_chars` for a button, keeping its "(section)" and "(2)" whole:
/// "Tarte au citron meringuée maison (meringue)" → "Tarte au c... (meringue)", so that linked
/// recipes, which differ only at the end, keep different buttons.
pub fn short_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let suffix_start = name
        .match_indices(" (")
        .map(|(at, _)| at)
        .find(|&at| is_parenthesized(&name[at..]));
    match suffix_start {
        Some(at) if name[at..].chars().count() + 6 <= max_chars => {
            let suffix = &name[at..];
            let head: String = name[..at]
                .chars()
                .take(max_chars - suffix.chars().count() - 3)
                .collect();
            format!("{}...{suffix}", head.trim_end())
        }
        _ => {
            let head: String = name.chars().take(max_chars.saturating_sub(3)).collect();
            format!("{head}...")
        }
    }
}

/// Whether `text` is only " (…)" groups: " (sauce)", " (sauce) (2)".
fn is_parenthesized(text: &str) -> bool {
    let mut rest = text;
    while !rest.is_empty() {
        let Some(inner) = rest.strip_prefix(" (") else {
            return false;
        };
        let Some(close) = inner.find(')') else {
            return false;
        };
        if inner[..close].contains('(') {
            return false;
        }
        rest = &inner[close + 1..];
    }
    true
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
        assert_eq!(parts[0].lines, [0, 1], "the lines before the first heading");
        assert_eq!(parts[1].lines, [2, 3], "sel, after the heading, joins it");
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
        assert_eq!(
            section_title("Pour la"),
            "pour la",
            "nothing after the article"
        );
        assert_eq!(section_title("Pour la :"), "pour la");
        assert_eq!(
            section_title("BBQ sauce"),
            "BBQ sauce",
            "an acronym is kept"
        );
        assert_eq!(section_title("  Garniture  :  "), "garniture");
    }

    #[test]
    fn spellings_of_one_heading_are_one_part() {
        let malva = [
            line("sucre", Some("Pour le gâteau")),
            line("crème", Some("Pour la sauce")),
            line("beurre", Some("Pour la sauce :")),
            line("eau", Some("POUR LA SAUCE")),
        ];
        let parts = split_parts(&malva);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].heading.as_deref(), Some("sauce"));
        assert_eq!(parts[1].lines, [1, 2, 3]);
    }

    #[test]
    fn a_line_under_no_heading_after_one_joins_the_part_above() {
        let malva = [
            line("sucre", Some("Pour le gâteau")),
            line("farine", Some("Pour le gâteau")),
            line("crème", Some("Pour la sauce")),
            line("sel", None),
        ];
        let parts = split_parts(&malva);
        assert_eq!(parts[0].lines, [0, 1]);
        assert_eq!(parts[1].lines, [2, 3], "sel stays with the sauce");
        let interleaved = [
            line("sucre", Some("Gâteau")),
            line("crème", Some("Sauce")),
            line("farine", Some("Gâteau")),
            line("sel", None),
        ];
        let arranged = arrange(interleaved.to_vec());
        let order: Vec<(&str, Option<&str>)> = arranged
            .iter()
            .map(|line| (line.ingredient_name.as_str(), line.section.as_deref()))
            .collect();
        assert_eq!(
            order,
            [
                ("sucre", Some("Gâteau")),
                ("farine", Some("Gâteau")),
                ("sel", Some("Gâteau")),
                ("crème", Some("Sauce")),
            ],
            "in part order, the last line ends the last part"
        );
        assert_eq!(split_parts(&arranged).len(), 2);
        let plain = arrange(vec![line("farine", Some("Pâte")), line("sel", None)]);
        assert!(
            plain.iter().all(|line| line.section.is_none()),
            "one part: no heading kept"
        );
    }

    #[test]
    fn buttons_keep_the_part_of_a_long_name() {
        assert_eq!(
            short_name("Tarte au citron meringuée maison (meringue)", 30),
            "Tarte au citron... (meringue)"
        );
        let names = [
            "Tarte au citron meringuée maison",
            "Tarte au citron meringuée maison (pâte)",
            "Tarte au citron meringuée maison (crème)",
            "Tarte au citron meringuée maison (meringue) (2)",
        ];
        let labels: Vec<String> = names.iter().map(|name| short_name(name, 30)).collect();
        for (label, name) in labels.iter().zip(names) {
            assert!(label.chars().count() <= 30, "{label}");
            if let Some(at) = name.find(" (") {
                assert!(label.ends_with(&name[at..]), "{label}");
            }
        }
        let unique: std::collections::BTreeSet<&String> = labels.iter().collect();
        assert_eq!(unique.len(), names.len(), "{labels:?}");
        assert_eq!(short_name("Crêpes", 30), "Crêpes");
        assert_eq!(
            short_name(&"a".repeat(40), 30),
            format!("{}...", "a".repeat(27))
        );
    }

    #[test]
    fn long_names_are_shortened_to_fit() {
        let name = part_name(&"é".repeat(200), "sauce");
        assert!(name.len() <= MAX_NAME_BYTES, "{}", name.len());
        assert!(name.ends_with(" (sauce)"));
    }
}
