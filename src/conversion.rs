//! The conversion table of the unit views (UX plan U8, `specs/tasks/measure-conversion.md`):
//! what each canonical unit is worth in grams or millilitres, and the density of the most
//! common ingredients, to turn a volume into a weight and back.
//!
//! The ingredient rows are the ones of the design document's table, generated from the same
//! list: change both together.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::OnceLock;

use crate::units::CanonicalUnit;

/// A US cup, in millilitres.
pub const CUP_ML: f64 = 236.588;

/// An amount in the base unit of its kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    /// Grams.
    Mass(f64),
    /// Millilitres.
    Volume(f64),
}

impl Amount {
    /// `quantity` times this amount.
    pub fn times(self, quantity: f64) -> Self {
        match self {
            Self::Mass(grams) => Self::Mass(grams * quantity),
            Self::Volume(millilitres) => Self::Volume(millilitres * quantity),
        }
    }
}

/// What one canonical unit is worth. Spoons take the French values (15 and 5 ml), within 2 % of
/// the American ones, for round results (1 c. à s. = 1,5 cl).
pub fn unit_amount(unit: CanonicalUnit) -> Amount {
    match unit {
        CanonicalUnit::Gram => Amount::Mass(1.0),
        CanonicalUnit::Kilogram => Amount::Mass(1000.0),
        CanonicalUnit::Milligram => Amount::Mass(0.001),
        CanonicalUnit::Pound => Amount::Mass(453.592),
        CanonicalUnit::Ounce => Amount::Mass(28.3495),
        CanonicalUnit::Milliliter => Amount::Volume(1.0),
        CanonicalUnit::Liter => Amount::Volume(1000.0),
        CanonicalUnit::Teaspoon => Amount::Volume(5.0),
        CanonicalUnit::Tablespoon => Amount::Volume(15.0),
        CanonicalUnit::Cup => Amount::Volume(CUP_ML),
        CanonicalUnit::Pint => Amount::Volume(473.176),
        CanonicalUnit::Quart => Amount::Volume(946.353),
        CanonicalUnit::Gallon => Amount::Volume(3785.41),
    }
}

/// An ingredient of the density table.
#[derive(Debug, PartialEq, Eq)]
pub struct Ingredient {
    /// Its French names, the first one being the usual one.
    pub french: &'static [&'static str],
    /// Its English names.
    pub english: &'static [&'static str],
    /// Poured rather than weighed: shown in cl in the French view.
    pub liquid: bool,
    pub grams_per_litre: u16,
}

impl Ingredient {
    /// The weight of a volume of this ingredient.
    pub fn grams(&self, millilitres: f64) -> f64 {
        millilitres * f64::from(self.grams_per_litre) / 1000.0
    }

    /// The volume of a weight of this ingredient.
    pub fn millilitres(&self, grams: f64) -> f64 {
        grams * 1000.0 / f64::from(self.grams_per_litre)
    }
}

const LIQUID: bool = true;
const SOLID: bool = false;

const fn ingredient(
    french: &'static [&'static str],
    english: &'static [&'static str],
    liquid: bool,
    grams_per_litre: u16,
) -> Ingredient {
    Ingredient {
        french,
        english,
        liquid,
        grams_per_litre,
    }
}

/// The densities: King Arthur Baking's ingredient weight chart and usual USDA values for solids
/// and pastes, physical densities for liquids (researched on 2026-10-29).
pub const INGREDIENTS: &[Ingredient] = &[
    // Farines et féculents
    ingredient(
        &["farine", "farine de blé"],
        &["flour", "all-purpose flour", "bread flour"],
        SOLID,
        507,
    ),
    ingredient(&["farine complète"], &["whole wheat flour"], SOLID, 478),
    ingredient(&["farine de seigle"], &["rye flour"], SOLID, 448),
    ingredient(&["farine de riz"], &["rice flour"], SOLID, 600),
    ingredient(&["farine de sarrasin"], &["buckwheat flour"], SOLID, 507),
    ingredient(
        &["maïzena", "fécule de maïs"],
        &["cornstarch", "corn starch"],
        SOLID,
        473,
    ),
    ingredient(
        &["fécule de pomme de terre"],
        &["potato starch"],
        SOLID,
        642,
    ),
    ingredient(
        &["poudre d'amande", "amande en poudre"],
        &["almond flour", "ground almonds"],
        SOLID,
        406,
    ),
    ingredient(&["poudre de noisette"], &["hazelnut flour"], SOLID, 376),
    ingredient(&["chapelure"], &["breadcrumbs"], SOLID, 473),
    ingredient(&["flocons d'avoine"], &["oats", "rolled oats"], SOLID, 376),
    // Sucres et sirops
    ingredient(
        &["sucre", "sucre en poudre", "sucre semoule"],
        &["sugar", "granulated sugar"],
        SOLID,
        837,
    ),
    ingredient(
        &["sucre glace"],
        &["powdered sugar", "icing sugar", "confectioners sugar"],
        SOLID,
        478,
    ),
    ingredient(
        &["cassonade", "sucre roux", "vergeoise", "sucre de canne"],
        &["brown sugar", "cane sugar"],
        SOLID,
        900,
    ),
    ingredient(&["miel"], &["honey"], SOLID, 1420),
    ingredient(
        &["sirop d'érable", "sirop de maïs"],
        &["maple syrup", "corn syrup"],
        LIQUID,
        1319,
    ),
    ingredient(&["sirop d'agave"], &["agave syrup"], LIQUID, 1420),
    ingredient(&["mélasse"], &["molasses"], LIQUID, 1437),
    ingredient(&["confiture"], &["jam", "preserves"], SOLID, 1437),
    ingredient(
        &["lait concentré sucré"],
        &["sweetened condensed milk"],
        SOLID,
        1319,
    ),
    // Matières grasses
    ingredient(
        &["beurre", "margarine"],
        &["butter", "margarine"],
        SOLID,
        955,
    ),
    ingredient(&["saindoux"], &["lard"], SOLID, 955),
    ingredient(
        &["huile", "huile de tournesol", "huile de colza"],
        &["oil", "vegetable oil", "sunflower oil"],
        LIQUID,
        920,
    ),
    ingredient(&["huile d'olive"], &["olive oil"], LIQUID, 915),
    ingredient(&["huile de coco"], &["coconut oil"], SOLID, 955),
    ingredient(&["beurre de cacahuète"], &["peanut butter"], SOLID, 1141),
    // Produits laitiers
    ingredient(&["lait"], &["milk"], LIQUID, 1030),
    ingredient(
        &["babeurre", "lait fermenté"],
        &["buttermilk"],
        LIQUID,
        1030,
    ),
    ingredient(&["lait concentré"], &["evaporated milk"], LIQUID, 1065),
    ingredient(
        &["lait en poudre"],
        &["milk powder", "dry milk"],
        SOLID,
        473,
    ),
    ingredient(
        &["crème", "crème liquide", "crème entière"],
        &["cream", "heavy cream", "whipping cream"],
        LIQUID,
        1000,
    ),
    ingredient(
        &["crème fraîche", "crème épaisse"],
        &["crème fraîche", "sour cream"],
        SOLID,
        1010,
    ),
    ingredient(
        &["yaourt", "fromage blanc", "skyr"],
        &["yogurt", "greek yogurt"],
        SOLID,
        1040,
    ),
    ingredient(&["mascarpone"], &["mascarpone"], SOLID, 1000),
    ingredient(&["ricotta"], &["ricotta"], SOLID, 1040),
    ingredient(
        &["fromage frais", "cream cheese"],
        &["cream cheese"],
        SOLID,
        981,
    ),
    ingredient(
        &["fromage râpé", "gruyère", "emmental", "mozzarella"],
        &["shredded cheese", "cheddar", "mozzarella"],
        SOLID,
        478,
    ),
    ingredient(&["parmesan"], &["parmesan"], SOLID, 423),
    ingredient(&["feta"], &["feta"], SOLID, 482),
    // Liquides
    ingredient(&["eau"], &["water"], LIQUID, 1000),
    ingredient(&["bouillon", "fond"], &["broth", "stock"], LIQUID, 1000),
    ingredient(
        &["vin", "vin blanc", "vin rouge"],
        &["wine", "white wine", "red wine"],
        LIQUID,
        990,
    ),
    ingredient(&["bière"], &["beer"], LIQUID, 1010),
    ingredient(
        &["jus", "jus d'orange"],
        &["juice", "orange juice"],
        LIQUID,
        1040,
    ),
    ingredient(
        &["jus de citron"],
        &["lemon juice", "lime juice"],
        LIQUID,
        1030,
    ),
    ingredient(&["café", "thé"], &["coffee", "tea"], LIQUID, 1000),
    ingredient(&["lait de coco"], &["coconut milk"], LIQUID, 1019),
    ingredient(&["crème de coco"], &["coconut cream"], SOLID, 1200),
    ingredient(&["vinaigre"], &["vinegar"], LIQUID, 1010),
    ingredient(&["sauce soja"], &["soy sauce"], LIQUID, 1150),
    ingredient(
        &["rhum", "cognac", "kirsch", "alcool"],
        &["rum", "brandy", "liqueur"],
        LIQUID,
        950,
    ),
    ingredient(&["extrait de vanille"], &["vanilla extract"], LIQUID, 947),
    // Sauces et condiments
    ingredient(&["concentré de tomate"], &["tomato paste"], SOLID, 1107),
    ingredient(
        &[
            "coulis de tomate",
            "purée de tomate",
            "sauce tomate",
            "passata",
        ],
        &["tomato puree", "passata", "tomato sauce"],
        SOLID,
        1057,
    ),
    ingredient(&["moutarde"], &["mustard"], SOLID, 1057),
    ingredient(&["mayonnaise"], &["mayonnaise"], SOLID, 940),
    ingredient(&["ketchup"], &["ketchup"], SOLID, 1150),
    ingredient(&["pesto"], &["pesto"], SOLID, 947),
    ingredient(&["tahini", "purée de sésame"], &["tahini"], SOLID, 1082),
    ingredient(
        &["pâte à tartiner", "nutella"],
        &["hazelnut spread", "nutella"],
        SOLID,
        1260,
    ),
    // Chocolat et café
    ingredient(
        &["cacao", "cacao en poudre"],
        &["cocoa", "cocoa powder"],
        SOLID,
        355,
    ),
    ingredient(
        &["chocolat", "pépites de chocolat", "chocolat haché"],
        &["chocolate", "chocolate chips", "chopped chocolate"],
        SOLID,
        719,
    ),
    ingredient(
        &["café soluble", "café en poudre"],
        &["instant coffee", "espresso powder"],
        SOLID,
        473,
    ),
    // Fruits secs et graines
    ingredient(
        &["amande", "amandes entières"],
        &["almonds", "whole almonds"],
        SOLID,
        600,
    ),
    ingredient(&["amandes effilées"], &["sliced almonds"], SOLID, 364),
    ingredient(
        &["noix", "cerneaux de noix"],
        &["walnuts", "chopped walnuts"],
        SOLID,
        478,
    ),
    ingredient(&["noix de pécan"], &["pecans"], SOLID, 444),
    ingredient(&["noisettes"], &["hazelnuts"], SOLID, 600),
    ingredient(&["noix de cajou"], &["cashews"], SOLID, 478),
    ingredient(&["pistaches"], &["pistachios"], SOLID, 507),
    ingredient(&["pignons de pin"], &["pine nuts"], SOLID, 600),
    ingredient(&["cacahuètes"], &["peanuts"], SOLID, 600),
    ingredient(
        &["noix de coco râpée", "noix de coco", "coco râpée"],
        &["shredded coconut", "desiccated coconut"],
        SOLID,
        359,
    ),
    ingredient(&["raisins secs"], &["raisins"], SOLID, 630),
    ingredient(&["abricots secs"], &["dried apricots"], SOLID, 541),
    ingredient(&["dattes"], &["dates"], SOLID, 630),
    ingredient(
        &["cranberries séchées", "canneberges"],
        &["dried cranberries"],
        SOLID,
        482,
    ),
    ingredient(
        &["graines de sésame", "sésame"],
        &["sesame seeds"],
        SOLID,
        600,
    ),
    ingredient(&["graines de chia"], &["chia seeds"], SOLID, 626),
    ingredient(&["graines de lin"], &["flaxseed", "flax seeds"], SOLID, 592),
    ingredient(&["graines de tournesol"], &["sunflower seeds"], SOLID, 592),
    ingredient(&["graines de courge"], &["pumpkin seeds"], SOLID, 676),
    ingredient(&["graines de pavot", "pavot"], &["poppy seeds"], SOLID, 609),
    // Levures, sel et épices
    ingredient(&["sel", "sel fin"], &["salt", "table salt"], SOLID, 1217),
    ingredient(
        &["gros sel", "sel de mer"],
        &["coarse salt", "kosher salt", "sea salt"],
        SOLID,
        1082,
    ),
    ingredient(&["levure chimique"], &["baking powder"], SOLID, 812),
    ingredient(
        &["bicarbonate", "bicarbonate de soude"],
        &["baking soda"],
        SOLID,
        1217,
    ),
    ingredient(
        &["levure boulangère", "levure de boulanger", "levure sèche"],
        &["yeast", "dry yeast", "instant yeast"],
        SOLID,
        609,
    ),
    ingredient(&["cannelle"], &["cinnamon"], SOLID, 527),
    ingredient(
        &[
            "épices",
            "cumin",
            "curry",
            "paprika",
            "muscade",
            "noix de muscade",
            "gingembre moulu",
            "poivre",
            "ail en poudre",
            "oignon en poudre",
        ],
        &[
            "spices",
            "cumin",
            "curry",
            "paprika",
            "nutmeg",
            "ground ginger",
            "pepper",
            "garlic powder",
            "onion powder",
        ],
        SOLID,
        467,
    ),
    ingredient(
        &["herbes séchées", "thym", "origan", "herbes de provence"],
        &["dried herbs", "thyme", "oregano"],
        SOLID,
        203,
    ),
    // Céréales et légumineuses
    ingredient(&["riz"], &["rice"], SOLID, 837),
    ingredient(&["riz cuit"], &["cooked rice"], SOLID, 719),
    ingredient(&["pâtes", "macaronis"], &["pasta", "macaroni"], SOLID, 444),
    ingredient(&["semoule"], &["semolina"], SOLID, 689),
    ingredient(&["couscous"], &["couscous"], SOLID, 731),
    ingredient(&["boulgour"], &["bulgur"], SOLID, 642),
    ingredient(&["quinoa"], &["quinoa"], SOLID, 748),
    ingredient(&["polenta"], &["polenta", "cornmeal"], SOLID, 689),
    ingredient(&["lentilles"], &["lentils"], SOLID, 812),
    ingredient(&["pois chiches"], &["chickpeas"], SOLID, 693),
    // Légumes et fruits (coupés)
    ingredient(&["oignon"], &["onion"], SOLID, 600),
    ingredient(&["échalote"], &["shallot"], SOLID, 659),
    ingredient(&["ail"], &["garlic"], SOLID, 947),
    ingredient(&["carotte"], &["carrot"], SOLID, 600),
    ingredient(
        &["carotte râpée"],
        &["grated carrot", "shredded carrot"],
        SOLID,
        418,
    ),
    ingredient(&["céleri"], &["celery"], SOLID, 600),
    ingredient(&["poivron"], &["bell pepper"], SOLID, 600),
    ingredient(&["champignons"], &["mushrooms"], SOLID, 330),
    ingredient(&["poireau"], &["leek"], SOLID, 389),
    ingredient(&["courgette"], &["zucchini"], SOLID, 571),
    ingredient(&["épinards"], &["spinach"], SOLID, 127),
    ingredient(&["tomates"], &["tomatoes"], SOLID, 761),
    ingredient(&["maïs"], &["corn"], SOLID, 642),
    ingredient(&["petits pois"], &["peas"], SOLID, 592),
    ingredient(&["olives"], &["olives"], SOLID, 600),
    ingredient(
        &[
            "herbes fraîches",
            "persil",
            "ciboulette",
            "basilic",
            "coriandre",
        ],
        &["fresh herbs", "parsley", "chives", "basil", "cilantro"],
        SOLID,
        178,
    ),
    ingredient(&["pommes"], &["apples"], SOLID, 478),
    ingredient(&["pommes de terre"], &["potatoes"], SOLID, 634),
    ingredient(
        &["banane écrasée", "banane"],
        &["banana", "mashed banana"],
        SOLID,
        959,
    ),
    ingredient(&["compote"], &["applesauce"], SOLID, 1078),
    ingredient(
        &["purée de potiron", "potiron"],
        &["pumpkin puree", "pumpkin"],
        SOLID,
        1036,
    ),
    ingredient(
        &["myrtilles", "fruits rouges"],
        &["blueberries", "berries"],
        SOLID,
        655,
    ),
    ingredient(&["framboises"], &["raspberries"], SOLID, 507),
    ingredient(&["fraises"], &["strawberries"], SOLID, 706),
];

/// Every name of the table as search words, with its ingredient and language.
struct Name {
    words: Vec<String>,
    french: bool,
    ingredient: &'static Ingredient,
}

fn names() -> &'static [Name] {
    static NAMES: OnceLock<Vec<Name>> = OnceLock::new();
    NAMES.get_or_init(|| {
        INGREDIENTS
            .iter()
            .flat_map(|ingredient| {
                let french = ingredient.french.iter().map(move |name| (name, true));
                let english = ingredient.english.iter().map(move |name| (name, false));
                french.chain(english).map(move |(name, french)| Name {
                    words: crate::search::query_words(name),
                    french,
                    ingredient,
                })
            })
            .filter(|name| !name.words.is_empty())
            .collect()
    })
}

/// The ingredient of the table that a recipe line names, by whole words like the search
/// (accents, case, plurals and small words ignored). The name with the most words wins
/// ("farine de riz" over "farine"); on a tie, the first word counts in French ("lait d'amande"
/// is milk) and the last one in English ("almond milk" is milk).
pub fn find_ingredient(name: &str) -> Option<&'static Ingredient> {
    let words = crate::search::query_words(name);
    names()
        .iter()
        .filter_map(|candidate| {
            let start = find_run(&words, &candidate.words)?;
            let position = if candidate.french {
                -(start as i64)
            } else {
                (start + candidate.words.len()) as i64
            };
            Some(((candidate.words.len(), position), candidate.ingredient))
        })
        // The first of equal candidates, in table order
        .fold(
            None,
            |best: Option<((usize, i64), &Ingredient)>, candidate| match best {
                Some(best) if best.0 >= candidate.0 => Some(best),
                _ => Some(candidate),
            },
        )
        .map(|(_, ingredient)| ingredient)
}

/// Where `phrase` appears in `words` as consecutive words.
fn find_run(words: &[String], phrase: &[String]) -> Option<usize> {
    if phrase.is_empty() || phrase.len() > words.len() {
        return None;
    }
    (0..=words.len() - phrase.len()).find(|&start| words[start..start + phrase.len()] == *phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usual_name(name: &str) -> Option<&'static str> {
        find_ingredient(name).map(|ingredient| ingredient.french[0])
    }

    #[test]
    fn a_cup_of_flour_is_120_g_and_a_spoon_of_butter_14_g() {
        let grams = |name: &str, unit: CanonicalUnit| match unit_amount(unit) {
            Amount::Volume(millilitres) => {
                find_ingredient(name).map(|ingredient| ingredient.grams(millilitres).round())
            }
            Amount::Mass(_) => None,
        };
        assert_eq!(grams("farine", CanonicalUnit::Cup), Some(120.0));
        assert_eq!(grams("all-purpose flour", CanonicalUnit::Cup), Some(120.0));
        assert_eq!(grams("beurre doux", CanonicalUnit::Tablespoon), Some(14.0));
        assert_eq!(grams("sucre", CanonicalUnit::Cup), Some(198.0));
        assert_eq!(grams("sel", CanonicalUnit::Teaspoon), Some(6.0));
        assert_eq!(
            unit_amount(CanonicalUnit::Pound).times(2.0),
            Amount::Mass(907.184)
        );
    }

    #[test]
    fn the_most_precise_name_wins() {
        assert_eq!(usual_name("lait de coco"), Some("lait de coco"));
        assert_eq!(usual_name("farine de riz"), Some("farine de riz"));
        assert_eq!(usual_name("Farine T55"), Some("farine"));
        assert_eq!(usual_name("sucre glace"), Some("sucre glace"));
        assert_eq!(
            usual_name("beurre de cacahuète"),
            Some("beurre de cacahuète")
        );
        assert_eq!(usual_name("pommes de terre"), Some("pommes de terre"));
        assert_eq!(usual_name("noix de muscade"), Some("épices"));
        assert_eq!(usual_name("noix de coco râpée"), Some("noix de coco râpée"));
        assert_eq!(usual_name("crème fraîche épaisse"), Some("crème fraîche"));
        assert_eq!(usual_name("gros sel"), Some("gros sel"));
        assert_eq!(usual_name("extra virgin olive oil"), Some("huile d'olive"));
        assert_eq!(usual_name("garlic powder"), Some("épices"));
        assert_eq!(usual_name("corn syrup"), Some("sirop d'érable"));
        assert_eq!(usual_name("Cream cheese"), Some("fromage frais"));
    }

    #[test]
    fn ties_go_to_the_head_word_of_each_language() {
        assert_eq!(usual_name("lait d'amande"), Some("lait"));
        assert_eq!(usual_name("almond milk"), Some("lait"));
        assert_eq!(usual_name("beurre demi-sel"), Some("beurre"));
        assert_eq!(usual_name("unsalted butter"), Some("beurre"));
    }

    #[test]
    fn unknown_names_find_nothing() {
        for name in ["œufs", "eggs", "zeste de citron", "levure", "", "xyz"] {
            assert_eq!(usual_name(name), None, "{name}");
        }
    }

    #[test]
    fn every_name_of_the_table_finds_its_own_ingredient() {
        for ingredient in INGREDIENTS {
            assert!(
                (100..=2000).contains(&ingredient.grams_per_litre),
                "{:?}",
                ingredient.french
            );
            for name in ingredient.french.iter().chain(ingredient.english) {
                assert_eq!(
                    find_ingredient(name),
                    Some(ingredient),
                    "{name} finds another ingredient"
                );
            }
        }
    }
}
