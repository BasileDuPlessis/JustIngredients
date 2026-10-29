# Affichage des unités (validé)

Version 3 du 2026-10-29, validée par l'utilisateur (décisions ci-dessous) : c'est la phase U8 du
[plan UX](ux-improvements.md). Demandes de l'utilisateur :
- version 1 : « proposer de mettre toutes les mesures en unités françaises : g / cl / l… » et
  « un outil de conversion des volumes en poids (ex. 1 cup butter ⇒ … g de beurre), avec la
  densité des principaux ingrédients en constantes » ;
- version 2 : « dans le menu, une commande pour changer la vue des unités : tout en poids (g),
  pratique avec une balance ; tout en unités françaises, cl pour les volumes et g pour les
  poids ; tout en unités anglaises (choisis les défauts) ».

## Décisions (2026-10-29)

1. Par défaut, la vue **« Unités françaises (g, cl) »** pour tout le monde.
2. Les défauts du tableau, avec une balance **précise au gramme** : la vue poids ne garde une
   cuillère que sous 1 g.
3. La vue s'applique **seulement aux fiches recettes** ; la relecture et la modification
   montrent les valeurs enregistrées.
4. Après un choix dans `/unites`, la fiche qu'on regardait revient dans la nouvelle vue (ou la
   liste).
5. La table des densités couvre les ingrédients les plus courants (recherche du 2026-10-29,
   125 entrées).
6. Les unités enregistrées avant la normalisation sont remises en ordre une fois.

## Le principe

**Une vue, pas une conversion enregistrée.** Chaque utilisateur choisit comment les mesures lui
sont affichées ; ses recettes ne changent jamais en base. Changer de vue est sans risque et
réversible à tout moment.

Quatre vues :

| Vue | Pour qui | En bref |
|---|---|---|
| **Comme dans la recette** | tout le monde | les mesures telles que lues ou tapées ; seuls les noms d'unités sont traduits (« tbsp » → « c. à s. » pour un utilisateur français) |
| **Tout en poids (g)** | cuisiner avec une balance | tout ce qui peut se peser en grammes, liquides compris |
| **Unités françaises (g, cl)** (par défaut) | recettes américaines lues en France | les poids en g, les liquides en cl / l, les cuillères gardées |
| **Unités américaines (cups, oz)** | recettes françaises lues à l'anglaise | cups, tbsp, tsp, oz, lb, en fractions (½, ¾) |

Elle s'applique aux **fiches recettes** (consultation, c'est là qu'on cuisine). La relecture
d'une photo et la modification des ingrédients montrent toujours les valeurs enregistrées : ce
qu'on corrige est ce qui est enregistré. Le nom des ingrédients n'est jamais traduit.

Quand une fiche est convertie, une ligne en bas le dit : « ⚖️ Vue poids (g), valeurs arrondies ».
Une mesure impossible à convertir (densité inconnue, « 1 tasse de noix de pécan entières » en
vue poids) reste dans son unité d'origine.

## La commande

`/unites` (français) et `/units` (anglais), dans le menu Telegram avec `/liste`, `/cherche`,
`/annuler`, `/aide`. Le choix est enregistré pour l'utilisateur (une colonne `unit_view` dans
`users`, migration 5).

### 1. Je tape /unites (ou je le choisis dans le menu)

Comme `/aide` : la réponse prend la place du message du bas.

```
┌──────────────────────────────────────────┐
│ ⚖️ Affichage des unités                  │ ▼ bas du fil
│ Les recettes ne changent pas : seul      │
│ l'affichage des fiches suit ce choix.    │
│ [ Comme dans la recette ]                │
│ [ Tout en poids (g) ]                    │
│ [ ✓ Unités françaises (g, cl) ]          │
│ [ Unités américaines (cups, oz) ]        │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

### 2. Je choisis « Tout en poids (g) »

Le message devient une ligne d'historique. Si une fiche était en bas avant `/unites`, elle
revient en bas, dans la nouvelle vue, avec ses boutons ; sinon la liste revient.

```
┌──────────────────────────────────────────┐
│ ⚖️ Unités : tout en poids (g)            │   historique
├──────────────────────────────────────────┤
│ 📖 Pancakes                              │ ▼ bas du fil
│ 📅 29 oct. 2026                          │
│ • 180 g farine                           │
│ • 305 g lait                             │
│ • 43 g beurre                            │
│ • 13 g sucre                             │
│ • 8 g levure chimique                    │
│ • 3 g sel                                │
│ • 1 œuf                                  │
│ ⚖️ Vue poids (g), valeurs arrondies      │
│ [ ✏️ Renommer ]  [ 📝 Modifier ]          │
│ [ 🗑️ Supprimer ]                          │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

## Une recette dans les quatre vues

Une recette américaine enregistrée en tasses (le résultat exact du code, testé) :

```
Comme dans la recette       Tout en poids (g)        Unités françaises          Unités américaines
• 1 ½ tasse farine          • 180 g farine           • 180 g farine             • 1 ½ cups farine
• 1 ¼ tasse lait            • 305 g lait             • 30 cl lait               • 1 ¼ cups lait
• 3 c. à s. beurre          • 43 g beurre            • 3 c. à s. beurre         • 3 tbsp beurre
• 1 c. à s. sucre           • 13 g sucre             • 1 c. à s. sucre          • 1 tbsp sucre
• 2 c. à c. levure chimique • 8 g levure chimique    • 2 c. à c. levure chimique • 2 tsp levure chimique
• ½ c. à c. sel             • 3 g sel                • ½ c. à c. sel            • ½ tsp sel
• 1 œuf                     • 1 œuf                  • 1 œuf                    • 1 œuf
```

Une recette française enregistrée en grammes et en ml :

```
Comme dans la recette    Tout en poids (g)     Unités françaises     Unités américaines
• 250 g farine           • 250 g farine        • 250 g farine        • 2 cups farine
• 500 ml lait            • 515 g lait          • 50 cl lait          • 2 cups lait
• 50 g sucre             • 50 g sucre          • 50 g sucre          • ¼ cup sucre
• 30 g beurre            • 30 g beurre         • 30 g beurre         • 2 tbsp beurre
• 3 œufs                 • 3 œufs              • 3 œufs              • 3 œufs
• 1 pincée sel           • 1 pincée sel        • 1 pincée sel        • 1 pincée sel
```

Les cuillères, tasses et mesures américaines s'écrivent en fractions (« ½ c. à c. », « 1 ½ cups »),
les unités métriques avec une virgule (« 1,5 kg »). « Levure » seule n'est pas convertie : chimique
ou boulangère, la densité n'est pas la même.

## Les défauts de chaque vue

| Mesure enregistrée | Tout en poids (g) | Unités françaises (g, cl) | Unités américaines (cups, oz) |
|---|---|---|---|
| g, kg, mg | g (1 200 g, pas de kg : c'est ce que montre la balance) | g, kg à partir d'1 kg | solide du tableau (farine, sucre…) en cups dès ¼ cup, sinon tbsp / tsp ; le reste en oz sous 1 lb, lb au-delà |
| lb, oz | g | g | inchangé |
| ml, l (liquide) | g par densité (eau, lait ≈ 1) | cl, l à partir d'1 l | cups (¼ cup et plus), sinon tbsp / tsp |
| cup, pint, quart (liquide) | g par densité | cl, l à partir d'1 l | inchangé |
| cup (solide du tableau) | g par densité | g par densité | inchangé |
| cup (solide inconnu) | inchangé | inchangé | inchangé |
| c. à s., c. à c. | g par densité, sauf sous 1 g (la balance est précise au gramme) | inchangé | tbsp, tsp |
| unités de comptage (pincée, gousses), sans unité (2 œufs), sans quantité (sel) | inchangé | inchangé | inchangé |

Arrondis « de cuisine » :
- grammes : au gramme sous 100 g, aux 5 g au-dessus (226 g → 225 g) ;
- cl : au demi-cl sous 10 cl, au cl au-dessus (23,7 cl → 24 cl) ; l au dixième (1,25 l → 1,3 l) ;
- cups au quart ou au tiers le plus proche (2,08 → 2 cups), tbsp au demi, tsp au quart ;
  oz au demi, lb au quart.

Nombres : fractions pour les cuillères, les tasses et les mesures américaines (« ½ c. à c. »,
« 1 ½ cups »), virgule décimale pour les unités métriques en français (« 1,5 kg »). Les valeurs des unités et des ingrédients sont dans la table de
conversion ci-dessous.

« Anglaises » s'entend ici au sens américain : les recettes en anglais lues par le bot viennent
surtout de sites américains, et les recettes britanniques actuelles sont déjà en g et ml.

## La table de conversion

Deux tables de constantes, dans un module `src/conversion.rs` (testé), reprises ici à
l'identique : les **unités** ramenées en grammes ou en millilitres, et les **densités** en g/l
des principaux ingrédients. Toute conversion passe par elles : volume → ml → (densité) → g, et
l'inverse. Pour en ajouter ou corriger une valeur, c'est une ligne dans chaque table.

### 1. Unités (françaises et anglaises)

| Unité (fr) | Unit (en) | Stockée | Vaut |
|---|---|---|---|
| gramme | gram | `g` | 1 g |
| kilogramme | kilogram | `kg` | 1 000 g |
| milligramme | milligram | `mg` | 0,001 g |
| livre | pound | `lb` | 453,6 g |
| once | ounce | `oz` | 28,35 g |
| millilitre | milliliter | `ml` | 1 ml |
| centilitre | centiliter | `ml` (converti à l'entrée) | 10 ml |
| décilitre | deciliter | `ml` (converti à l'entrée) | 100 ml |
| litre | liter | `l` | 1 000 ml |
| cuillère à café (c. à c.) | teaspoon | `tsp` | 5 ml |
| cuillère à soupe (c. à s.) | tablespoon | `tbsp` | 15 ml |
| tasse | cup (US) | `cup` | 236,6 ml (23,66 cl) |
| pinte | pint (US) | `pint` | 473,2 ml |
| quart | quart (US) | `quart` | 946,4 ml |
| gallon | gallon (US) | `gallon` | 3 785 ml |

Les cuillères américaines (14,8 ml et 4,9 ml) et françaises (15 ml et 5 ml) diffèrent de moins
de 2 % : une seule valeur, la française, pour des résultats ronds (1 c. à s. = 1,5 cl). Les
unités de comptage (pincée, gousse, zeste, sachet, brin…) n'ont pas de valeur : elles ne se
convertissent pas.

### 2. Densités des ingrédients (g/l)

« Liquide » : affiché en cl / l dans la vue française ; « solide » : en g. Les colonnes « 1
tasse », « 1 c. à s. » et « 1 c. à c. » sont calculées depuis la densité (tasse US de
236,6 ml, cuillères de 15 et 5 ml), pour se repérer.

Sources (recherche du 2026-10-29) : pour les solides et les pâtes, le tableau de référence de
King Arthur Baking ([ingredient weight chart](https://www.kingarthurbaking.com/learn/ingredient-weight-chart)),
complété par les valeurs USDA courantes (concentré de tomate, moutarde, ketchup, ricotta…) ;
pour les liquides, les densités physiques (King Arthur compte 1 tasse d'eau pour 227 g, la règle
américaine « 1 cup = 8 oz », alors qu'elle en pèse 237). La liste couvre les ingrédients les plus
courants des recettes françaises et des recettes américaines en tasses.

| Ingrédient (fr) | Ingredient (en) | Type | g/l | 1 tasse | 1 c. à s. | 1 c. à c. |
|---|---|---|---|---|---|---|
| **Farines et féculents** | | | | | | |
| farine, farine de blé | flour, all-purpose flour, bread flour | solide | 507 | 120 g | 7,6 g | 2,5 g |
| farine complète | whole wheat flour | solide | 478 | 113 g | 7,2 g | 2,4 g |
| farine de seigle | rye flour | solide | 448 | 106 g | 6,7 g | 2,2 g |
| farine de riz | rice flour | solide | 600 | 142 g | 9 g | 3 g |
| farine de sarrasin | buckwheat flour | solide | 507 | 120 g | 7,6 g | 2,5 g |
| maïzena, fécule de maïs | cornstarch, corn starch | solide | 473 | 112 g | 7,1 g | 2,4 g |
| fécule de pomme de terre | potato starch | solide | 642 | 152 g | 9,6 g | 3,2 g |
| poudre d'amande, amande en poudre | almond flour, ground almonds | solide | 406 | 96 g | 6,1 g | 2 g |
| poudre de noisette | hazelnut flour | solide | 376 | 89 g | 5,6 g | 1,9 g |
| chapelure | breadcrumbs | solide | 473 | 112 g | 7,1 g | 2,4 g |
| flocons d'avoine | oats, rolled oats | solide | 376 | 89 g | 5,6 g | 1,9 g |
| **Sucres et sirops** | | | | | | |
| sucre, sucre en poudre, sucre semoule | sugar, granulated sugar | solide | 837 | 198 g | 13 g | 4,2 g |
| sucre glace | powdered sugar, icing sugar, confectioners sugar | solide | 478 | 113 g | 7,2 g | 2,4 g |
| cassonade, sucre roux, vergeoise, sucre de canne | brown sugar, cane sugar | solide | 900 | 213 g | 14 g | 4,5 g |
| miel | honey | solide | 1 420 | 336 g | 21 g | 7,1 g |
| sirop d'érable, sirop de maïs | maple syrup, corn syrup | liquide | 1 319 | 312 g | 20 g | 6,6 g |
| sirop d'agave | agave syrup | liquide | 1 420 | 336 g | 21 g | 7,1 g |
| mélasse | molasses | liquide | 1 437 | 340 g | 22 g | 7,2 g |
| confiture | jam, preserves | solide | 1 437 | 340 g | 22 g | 7,2 g |
| lait concentré sucré | sweetened condensed milk | solide | 1 319 | 312 g | 20 g | 6,6 g |
| **Matières grasses** | | | | | | |
| beurre, margarine | butter, margarine | solide | 955 | 226 g | 14 g | 4,8 g |
| saindoux | lard | solide | 955 | 226 g | 14 g | 4,8 g |
| huile, huile de tournesol, huile de colza | oil, vegetable oil, sunflower oil | liquide | 920 | 218 g | 14 g | 4,6 g |
| huile d'olive | olive oil | liquide | 915 | 216 g | 14 g | 4,6 g |
| huile de coco | coconut oil | solide | 955 | 226 g | 14 g | 4,8 g |
| beurre de cacahuète | peanut butter | solide | 1 141 | 270 g | 17 g | 5,7 g |
| **Produits laitiers** | | | | | | |
| lait | milk | liquide | 1 030 | 244 g | 15 g | 5,2 g |
| babeurre, lait fermenté | buttermilk | liquide | 1 030 | 244 g | 15 g | 5,2 g |
| lait concentré | evaporated milk | liquide | 1 065 | 252 g | 16 g | 5,3 g |
| lait en poudre | milk powder, dry milk | solide | 473 | 112 g | 7,1 g | 2,4 g |
| crème, crème liquide, crème entière | cream, heavy cream, whipping cream | liquide | 1 000 | 237 g | 15 g | 5 g |
| crème fraîche, crème épaisse | crème fraîche, sour cream | solide | 1 010 | 239 g | 15 g | 5 g |
| yaourt, fromage blanc, skyr | yogurt, greek yogurt | solide | 1 040 | 246 g | 16 g | 5,2 g |
| mascarpone | mascarpone | solide | 1 000 | 237 g | 15 g | 5 g |
| ricotta | ricotta | solide | 1 040 | 246 g | 16 g | 5,2 g |
| fromage frais, cream cheese | cream cheese | solide | 981 | 232 g | 15 g | 4,9 g |
| fromage râpé, gruyère, emmental, mozzarella | shredded cheese, cheddar, mozzarella | solide | 478 | 113 g | 7,2 g | 2,4 g |
| parmesan | parmesan | solide | 423 | 100 g | 6,3 g | 2,1 g |
| feta | feta | solide | 482 | 114 g | 7,2 g | 2,4 g |
| **Liquides** | | | | | | |
| eau | water | liquide | 1 000 | 237 g | 15 g | 5 g |
| bouillon, fond | broth, stock | liquide | 1 000 | 237 g | 15 g | 5 g |
| vin, vin blanc, vin rouge | wine, white wine, red wine | liquide | 990 | 234 g | 15 g | 5 g |
| bière | beer | liquide | 1 010 | 239 g | 15 g | 5 g |
| jus, jus d'orange | juice, orange juice | liquide | 1 040 | 246 g | 16 g | 5,2 g |
| jus de citron | lemon juice, lime juice | liquide | 1 030 | 244 g | 15 g | 5,2 g |
| café, thé | coffee, tea | liquide | 1 000 | 237 g | 15 g | 5 g |
| lait de coco | coconut milk | liquide | 1 019 | 241 g | 15 g | 5,1 g |
| crème de coco | coconut cream | solide | 1 200 | 284 g | 18 g | 6 g |
| vinaigre | vinegar | liquide | 1 010 | 239 g | 15 g | 5 g |
| sauce soja | soy sauce | liquide | 1 150 | 272 g | 17 g | 5,8 g |
| rhum, cognac, kirsch, alcool | rum, brandy, liqueur | liquide | 950 | 225 g | 14 g | 4,8 g |
| extrait de vanille | vanilla extract | liquide | 947 | 224 g | 14 g | 4,7 g |
| **Sauces et condiments** | | | | | | |
| concentré de tomate | tomato paste | solide | 1 107 | 262 g | 17 g | 5,5 g |
| coulis de tomate, purée de tomate, sauce tomate, passata | tomato puree, passata, tomato sauce | solide | 1 057 | 250 g | 16 g | 5,3 g |
| moutarde | mustard | solide | 1 057 | 250 g | 16 g | 5,3 g |
| mayonnaise | mayonnaise | solide | 940 | 222 g | 14 g | 4,7 g |
| ketchup | ketchup | solide | 1 150 | 272 g | 17 g | 5,8 g |
| pesto | pesto | solide | 947 | 224 g | 14 g | 4,7 g |
| tahini, purée de sésame | tahini | solide | 1 082 | 256 g | 16 g | 5,4 g |
| pâte à tartiner, nutella | hazelnut spread, nutella | solide | 1 260 | 298 g | 19 g | 6,3 g |
| **Chocolat et café** | | | | | | |
| cacao, cacao en poudre | cocoa, cocoa powder | solide | 355 | 84 g | 5,3 g | 1,8 g |
| chocolat, pépites de chocolat, chocolat haché | chocolate, chocolate chips, chopped chocolate | solide | 719 | 170 g | 11 g | 3,6 g |
| café soluble, café en poudre | instant coffee, espresso powder | solide | 473 | 112 g | 7,1 g | 2,4 g |
| **Fruits secs et graines** | | | | | | |
| amande, amandes entières | almonds, whole almonds | solide | 600 | 142 g | 9 g | 3 g |
| amandes effilées | sliced almonds | solide | 364 | 86 g | 5,5 g | 1,8 g |
| noix, cerneaux de noix | walnuts, chopped walnuts | solide | 478 | 113 g | 7,2 g | 2,4 g |
| noix de pécan | pecans | solide | 444 | 105 g | 6,7 g | 2,2 g |
| noisettes | hazelnuts | solide | 600 | 142 g | 9 g | 3 g |
| noix de cajou | cashews | solide | 478 | 113 g | 7,2 g | 2,4 g |
| pistaches | pistachios | solide | 507 | 120 g | 7,6 g | 2,5 g |
| pignons de pin | pine nuts | solide | 600 | 142 g | 9 g | 3 g |
| cacahuètes | peanuts | solide | 600 | 142 g | 9 g | 3 g |
| noix de coco râpée, noix de coco, coco râpée | shredded coconut, desiccated coconut | solide | 359 | 85 g | 5,4 g | 1,8 g |
| raisins secs | raisins | solide | 630 | 149 g | 9,4 g | 3,1 g |
| abricots secs | dried apricots | solide | 541 | 128 g | 8,1 g | 2,7 g |
| dattes | dates | solide | 630 | 149 g | 9,4 g | 3,1 g |
| cranberries séchées, canneberges | dried cranberries | solide | 482 | 114 g | 7,2 g | 2,4 g |
| graines de sésame, sésame | sesame seeds | solide | 600 | 142 g | 9 g | 3 g |
| graines de chia | chia seeds | solide | 626 | 148 g | 9,4 g | 3,1 g |
| graines de lin | flaxseed, flax seeds | solide | 592 | 140 g | 8,9 g | 3 g |
| graines de tournesol | sunflower seeds | solide | 592 | 140 g | 8,9 g | 3 g |
| graines de courge | pumpkin seeds | solide | 676 | 160 g | 10 g | 3,4 g |
| graines de pavot, pavot | poppy seeds | solide | 609 | 144 g | 9,1 g | 3 g |
| **Levures, sel et épices** | | | | | | |
| sel, sel fin | salt, table salt | solide | 1 217 | 288 g | 18 g | 6,1 g |
| gros sel, sel de mer | coarse salt, kosher salt, sea salt | solide | 1 082 | 256 g | 16 g | 5,4 g |
| levure chimique | baking powder | solide | 812 | 192 g | 12 g | 4,1 g |
| bicarbonate, bicarbonate de soude | baking soda | solide | 1 217 | 288 g | 18 g | 6,1 g |
| levure boulangère, levure de boulanger, levure sèche | yeast, dry yeast, instant yeast | solide | 609 | 144 g | 9,1 g | 3 g |
| cannelle | cinnamon | solide | 527 | 125 g | 7,9 g | 2,6 g |
| épices, cumin, curry, paprika, muscade, noix de muscade, gingembre moulu, poivre, ail en poudre, oignon en poudre | spices, cumin, curry, paprika, nutmeg, ground ginger, pepper, garlic powder, onion powder | solide | 467 | 110 g | 7 g | 2,3 g |
| herbes séchées, thym, origan, herbes de provence | dried herbs, thyme, oregano | solide | 203 | 48 g | 3 g | 1 g |
| **Céréales et légumineuses** | | | | | | |
| riz | rice | solide | 837 | 198 g | 13 g | 4,2 g |
| riz cuit | cooked rice | solide | 719 | 170 g | 11 g | 3,6 g |
| pâtes, macaronis | pasta, macaroni | solide | 444 | 105 g | 6,7 g | 2,2 g |
| semoule | semolina | solide | 689 | 163 g | 10 g | 3,4 g |
| couscous | couscous | solide | 731 | 173 g | 11 g | 3,7 g |
| boulgour | bulgur | solide | 642 | 152 g | 9,6 g | 3,2 g |
| quinoa | quinoa | solide | 748 | 177 g | 11 g | 3,7 g |
| polenta | polenta, cornmeal | solide | 689 | 163 g | 10 g | 3,4 g |
| lentilles | lentils | solide | 812 | 192 g | 12 g | 4,1 g |
| pois chiches | chickpeas | solide | 693 | 164 g | 10 g | 3,5 g |
| **Légumes et fruits (coupés)** | | | | | | |
| oignon | onion | solide | 600 | 142 g | 9 g | 3 g |
| échalote | shallot | solide | 659 | 156 g | 9,9 g | 3,3 g |
| ail | garlic | solide | 947 | 224 g | 14 g | 4,7 g |
| carotte | carrot | solide | 600 | 142 g | 9 g | 3 g |
| carotte râpée | grated carrot, shredded carrot | solide | 418 | 99 g | 6,3 g | 2,1 g |
| céleri | celery | solide | 600 | 142 g | 9 g | 3 g |
| poivron | bell pepper | solide | 600 | 142 g | 9 g | 3 g |
| champignons | mushrooms | solide | 330 | 78 g | 5 g | 1,6 g |
| poireau | leek | solide | 389 | 92 g | 5,8 g | 1,9 g |
| courgette | zucchini | solide | 571 | 135 g | 8,6 g | 2,9 g |
| épinards | spinach | solide | 127 | 30 g | 1,9 g | 0,6 g |
| tomates | tomatoes | solide | 761 | 180 g | 11 g | 3,8 g |
| maïs | corn | solide | 642 | 152 g | 9,6 g | 3,2 g |
| petits pois | peas | solide | 592 | 140 g | 8,9 g | 3 g |
| olives | olives | solide | 600 | 142 g | 9 g | 3 g |
| herbes fraîches, persil, ciboulette, basilic, coriandre | fresh herbs, parsley, chives, basil, cilantro | solide | 178 | 42 g | 2,7 g | 0,9 g |
| pommes | apples | solide | 478 | 113 g | 7,2 g | 2,4 g |
| pommes de terre | potatoes | solide | 634 | 150 g | 9,5 g | 3,2 g |
| banane écrasée, banane | banana, mashed banana | solide | 959 | 227 g | 14 g | 4,8 g |
| compote | applesauce | solide | 1 078 | 255 g | 16 g | 5,4 g |
| purée de potiron, potiron | pumpkin puree, pumpkin | solide | 1 036 | 245 g | 16 g | 5,2 g |
| myrtilles, fruits rouges | blueberries, berries | solide | 655 | 155 g | 9,8 g | 3,3 g |
| framboises | raspberries | solide | 507 | 120 g | 7,6 g | 2,5 g |
| fraises | strawberries | solide | 706 | 167 g | 11 g | 3,5 g |

Reconnaissance du nom, comme la recherche : sans accents, majuscules ni pluriels, par mots
entiers, en français et en anglais. Le nom le plus précis gagne (« farine de riz » avant
« farine » et « riz », « lait de coco » avant « lait », « sucre glace » avant « sucre ») ; à
égalité, le premier mot compte en français (« lait d'amande » → lait) et le dernier en anglais
(« almond milk » → milk). Un ingrédient absent de la table garde son unité dans les vues qui
demanderaient une densité.

## Plus tard, pas dans cette phase

- Un bouton ⚖️ sur la fiche pour passer d'une vue à l'autre sans le menu.
- Convertir les œufs en grammes (1 œuf ≈ 50 g) dans la vue poids.
- Les températures (°F → °C) : elles sont dans les instructions, que le bot ne lit pas.

## Ce que contient la production (2026-10-29)

Requête lancée par l'utilisateur : `SELECT unit, count(*) FROM ingredients GROUP BY unit`. Sur
194 lignes :

| Unités | Lignes |
|---|---|
| g (101), G (5), gram (5), gr (3), kg (1) | 115 |
| sans unité (œufs, sel…) | 32 |
| cl (8), litre (4), dl (3), ml (3), l (1), litres (1) | 20 |
| tbsp (6), tsp (3), cuillère(s) à café (2), cuillères à soupe (1) | 12 |
| cup | 3 |
| zeste(s) (4), pincée / PINCÉES (3), gousse (2), brins (1), SACHET (1) | 11 |

Conclusions :
- les recettes sont déjà surtout en unités françaises : la vue française par défaut les montre
  presque telles quelles, et la vue la plus utile sera « Tout en poids (g) » (20 lignes en
  volume, 12 en cuillères) ;
- 26 lignes datent d'avant la normalisation des unités (G, gram, gr, litre, litres, dl, cl,
  cuillères en toutes lettres, majuscules). La conversion les lit dans tous les cas, mais la vue
  « comme dans la recette » afficherait « 5 G ». Proposé : **une remise en ordre unique** des
  unités enregistrées avec `units::normalize_measurement` (le code déjà utilisé pour les photos
  et les lignes tapées), en passe au démarrage comme la numérotation des doublons : G, gram, gr
  → g ; litre(s) → l ; cl, dl → ml avec la quantité convertie (8 cl → 80 ml, toujours affichés
  en cl à un utilisateur français) ; cuillères → tbsp / tsp ; PINCÉES, SACHET en minuscules.
  C'est la tâche 5 de [measurement-standardization-tasks.md](measurement-standardization-tasks.md).
