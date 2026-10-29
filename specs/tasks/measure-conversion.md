# Affichage des unités : proposition (à valider)

Version 2 du 2026-10-29. Demandes de l'utilisateur :
- version 1 : « proposer de mettre toutes les mesures en unités françaises : g / cl / l… » et
  « un outil de conversion des volumes en poids (ex. 1 cup butter ⇒ … g de beurre), avec la
  densité des principaux ingrédients en constantes » ;
- version 2 : « dans le menu, une commande pour changer la vue des unités : tout en poids (g),
  pratique avec une balance ; tout en unités françaises, cl pour les volumes et g pour les
  poids ; tout en unités anglaises (choisis les défauts) ».

Rien n'est codé : ce document décrit ce que je propose, avec des wireframes, et finit par les
questions à trancher.

## Le principe

**Une vue, pas une conversion enregistrée.** Chaque utilisateur choisit comment les mesures lui
sont affichées ; ses recettes ne changent jamais en base. Changer de vue est sans risque et
réversible à tout moment.

Quatre vues :

| Vue | Pour qui | En bref |
|---|---|---|
| **Comme dans la recette** (par défaut) | tout le monde | les mesures telles que lues ou tapées ; seuls les noms d'unités sont traduits (« tbsp » → « c. à s. » pour un utilisateur français) |
| **Tout en poids (g)** | cuisiner avec une balance | tout ce qui peut se peser en grammes, liquides compris |
| **Unités françaises (g, cl)** | recettes américaines lues en France | les poids en g, les liquides en cl / l, les cuillères gardées |
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
│ [ ✓ Comme dans la recette ]              │
│ [ Tout en poids (g) ]                    │
│ [ Unités françaises (g, cl) ]            │
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
│ • 45 g beurre                            │
│ • 13 g sucre                             │
│ • 8 g levure chimique                    │
│ • ½ c. à c. sel                          │
│ • 1 œuf                                  │
│ ⚖️ Vue poids (g), valeurs arrondies      │
│ [ ✏️ Renommer ]  [ 📝 Modifier ]          │
│ [ 🗑️ Supprimer ]                          │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

## Une recette dans les quatre vues

Une recette américaine enregistrée en tasses :

```
Comme dans la recette    Tout en poids (g)     Unités françaises     Unités américaines
• 1,5 tasse farine       • 180 g farine        • 180 g farine        • 1 ½ cups farine
• 1,25 tasse lait        • 305 g lait          • 30 cl lait          • 1 ¼ cups lait
• 3 c. à s. beurre       • 45 g beurre         • 3 c. à s. beurre    • 3 tbsp beurre
• 1 c. à s. sucre        • 13 g sucre          • 1 c. à s. sucre     • 1 tbsp sucre
• 2 c. à c. levure       • 8 g levure          • 2 c. à c. levure    • 2 tsp levure
• 0,5 c. à c. sel        • ½ c. à c. sel       • ½ c. à c. sel       • ½ tsp sel
• 1 œuf                  • 1 œuf               • 1 œuf               • 1 œuf
```

Une recette française enregistrée en grammes et en ml :

```
Comme dans la recette    Tout en poids (g)     Unités françaises     Unités américaines
• 250 g farine           • 250 g farine        • 250 g farine        • 2 cups farine
• 50 cl lait             • 515 g lait          • 50 cl lait          • 2 cups lait
• 50 g sucre             • 50 g sucre          • 50 g sucre          • ¼ cup sucre
• 30 g beurre            • 30 g beurre         • 30 g beurre         • 2 tbsp beurre
• 3 œufs                 • 3 œufs              • 3 œufs              • 3 œufs
• 1 pincée sel           • 1 pincée sel        • 1 pincée sel        • 1 pincée sel
```

## Les défauts de chaque vue

| Mesure enregistrée | Tout en poids (g) | Unités françaises (g, cl) | Unités américaines (cups, oz) |
|---|---|---|---|
| g, kg, mg | g (1 200 g, pas de kg : c'est ce que montre la balance) | g, kg à partir d'1 kg | solide du tableau (farine, sucre…) en cups dès ¼ cup, sinon tbsp / tsp ; le reste en oz sous 1 lb, lb au-delà |
| lb, oz | g | g | inchangé |
| ml, l (liquide) | g par densité (eau, lait ≈ 1) | cl, l à partir d'1 l | cups (¼ cup et plus), sinon tbsp / tsp |
| cup, pint, quart (liquide) | g par densité | cl, l à partir d'1 l | inchangé |
| cup (solide du tableau) | g par densité | g par densité | inchangé |
| cup (solide inconnu) | inchangé | inchangé | inchangé |
| c. à s., c. à c. | g par densité, sauf sous 5 g (une balance de cuisine n'est pas précise en dessous) | inchangé | tbsp, tsp |
| unités de comptage (pincée, gousses), sans unité (2 œufs), sans quantité (sel) | inchangé | inchangé | inchangé |

Arrondis « de cuisine » :
- grammes : au gramme sous 20 g, aux 5 g au-dessus (227 g → 225 g) ;
- cl : au demi-cl sous 10 cl, au cl au-dessus (23,7 cl → 24 cl) ; l au dixième (1,25 l → 1,3 l) ;
- cups au quart ou au tiers le plus proche (2,08 → 2 cups), tbsp au demi, tsp au quart ;
  oz au demi, lb au quart.

Nombres : virgule décimale dans les vues françaises (« 1,5 tasse »), fractions dans la vue
américaine (« 1 ½ cups »). Les valeurs des unités et des ingrédients sont dans la table de
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

« Liquide » : affiché en cl / l dans la vue française, en cups dans la vue américaine ;
« solide » : en g dans la vue française, en cups dans la vue américaine. Les colonnes « 1
tasse » et « 1 c. à s. » sont calculées depuis la densité (tasse US de 236,6 ml, c. à s. de
15 ml), pour se repérer. Valeurs des tableaux de pâtisserie courants (King Arthur Baking) et
des densités physiques pour les liquides, à ajuster si tu as tes propres repères.

| Ingrédient (fr) | Ingredient (en) | Type | g/l | 1 tasse | 1 c. à s. |
|---|---|---|---|---|---|
| farine | flour | solide | 505 | 120 g | 8 g |
| sucre, sucre en poudre | sugar, granulated sugar | solide | 845 | 200 g | 13 g |
| sucre glace | powdered sugar, icing sugar | solide | 485 | 115 g | 7 g |
| cassonade, sucre roux, vergeoise | brown sugar | solide | 910 | 215 g | 14 g |
| beurre | butter | solide | 960 | 227 g | 14 g |
| beurre de cacahuète | peanut butter | solide | 1 140 | 270 g | 17 g |
| miel | honey | solide | 1 415 | 335 g | 21 g |
| sirop d'érable | maple syrup | liquide | 1 310 | 310 g | 20 g |
| yaourt, fromage blanc | yogurt | solide | 1 040 | 246 g | 16 g |
| cacao | cocoa | solide | 360 | 85 g | 5 g |
| maïzena, fécule | cornstarch | solide | 480 | 114 g | 7 g |
| poudre d'amande, amandes en poudre | almond flour, ground almonds | solide | 400 | 95 g | 6 g |
| flocons d'avoine | oats, rolled oats | solide | 380 | 90 g | 6 g |
| riz | rice | solide | 845 | 200 g | 13 g |
| lentilles | lentils | solide | 845 | 200 g | 13 g |
| semoule, couscous, quinoa | semolina, couscous, quinoa | solide | 720 | 170 g | 11 g |
| pépites de chocolat | chocolate chips | solide | 720 | 170 g | 11 g |
| noix, noix de pécan, noisettes (hachées) | walnuts, pecans, hazelnuts (chopped) | solide | 485 | 115 g | 7 g |
| raisins secs | raisins | solide | 635 | 150 g | 10 g |
| noix de coco râpée | shredded coconut | solide | 360 | 85 g | 5 g |
| fromage râpé | shredded cheese | solide | 485 | 115 g | 7 g |
| parmesan râpé | grated parmesan | solide | 425 | 100 g | 6 g |
| sel | salt | solide | 1 215 | 287 g | 18 g (1 c. à c. = 6 g) |
| levure chimique | baking powder | solide | 810 | 192 g | 12 g (1 c. à c. = 4 g) |
| bicarbonate | baking soda | solide | 1 015 | 240 g | 15 g (1 c. à c. = 5 g) |
| levure boulangère sèche | dry yeast | solide | 610 | 144 g | 9 g (1 c. à c. = 3 g) |
| cannelle, épices moulues | cinnamon, ground spices | solide | 505 | 120 g | 8 g (1 c. à c. = 2,5 g) |
| extrait de vanille | vanilla extract | liquide | 880 | 208 g | 13 g (1 c. à c. = 4,4 g) |
| eau | water | liquide | 1 000 | 237 g | 15 g |
| lait | milk | liquide | 1 030 | 244 g | 15 g |
| crème, crème liquide, crème fraîche | cream, heavy cream | liquide | 1 000 | 237 g | 15 g |
| huile | oil | liquide | 920 | 218 g | 14 g |
| vin | wine | liquide | 990 | 234 g | 15 g |
| bouillon | broth, stock | liquide | 1 000 | 237 g | 15 g |
| jus | juice | liquide | 1 040 | 246 g | 16 g |
| lait de coco | coconut milk | liquide | 1 000 | 237 g | 15 g |
| café, thé | coffee, tea | liquide | 1 000 | 237 g | 15 g |
| vinaigre | vinegar | liquide | 1 010 | 239 g | 15 g |
| sauce soja | soy sauce | liquide | 1 150 | 272 g | 17 g |
| rhum | rum | liquide | 950 | 225 g | 14 g |

Reconnaissance du nom : sans accents, majuscules ni pluriels, mot entier (comme la recherche),
en français et en anglais ; l'entrée la plus précise gagne (« sucre glace » avant « sucre »,
« beurre de cacahuète » avant « beurre », « lait de coco » avant « lait »). Un ingrédient absent
de la table garde son unité dans les vues qui demanderaient une densité.

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
- les recettes sont déjà surtout en unités françaises : « Comme dans la recette » reste le bon
  défaut, et la vue la plus utile sera « Tout en poids (g) » (20 lignes en volume, 12 en
  cuillères) ;
- 26 lignes datent d'avant la normalisation des unités (G, gram, gr, litre, litres, dl, cl,
  cuillères en toutes lettres, majuscules). La conversion les lit dans tous les cas, mais la vue
  « comme dans la recette » afficherait « 5 G ». Proposé : **une remise en ordre unique** des
  unités enregistrées avec `units::normalize_measurement` (le code déjà utilisé pour les photos
  et les lignes tapées), en passe au démarrage comme la numérotation des doublons : G, gram, gr
  → g ; litre(s) → l ; cl, dl → ml avec la quantité convertie (8 cl → 80 ml, toujours affichés
  en cl à un utilisateur français) ; cuillères → tbsp / tsp ; PINCÉES, SACHET en minuscules.
  C'est la tâche 5 de [measurement-standardization-tasks.md](measurement-standardization-tasks.md).

## Questions

1. Les **quatre vues** et leurs noms te vont ? Par défaut **« Comme dans la recette »** pour
   tout le monde (je le recommande), ou « Unités françaises » d'office pour les utilisateurs
   français ?
2. Les **défauts** du tableau te vont ? En particulier : la vue poids garde les cuillères sous
   5 g ; la vue française garde les cuillères ; la vue américaine met en cups les solides du
   tableau (farine, sucre…) et le reste en oz / lb.
3. La vue s'applique aux **fiches** ; la relecture et la modification montrent les valeurs
   enregistrées. D'accord ?
4. Après un choix dans `/unites`, la **fiche** qu'on regardait revient dans la nouvelle vue (ou
   la liste s'il n'y en avait pas). D'accord ?
5. La **table de conversion** te convient (unités et densités, ingrédients à ajouter) ?
6. La **remise en ordre** des 26 unités enregistrées avant la normalisation : d'accord ?
