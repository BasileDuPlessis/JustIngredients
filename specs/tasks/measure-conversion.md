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
│ • 40 g beurre                            │
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
• 3 c. à s. beurre       • 40 g beurre         • 3 c. à s. beurre    • 3 tbsp beurre
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
américaine (« 1 ½ cups »). Une tasse US vaut 23,66 cl, une c. à s. 1,5 cl, une c. à c. 0,5 cl.

« Anglaises » s'entend ici au sens américain : les recettes en anglais lues par le bot viennent
surtout de sites américains, et les recettes britanniques actuelles sont déjà en g et ml.

## Densités (constantes)

Valeurs courantes des tableaux de pâtisserie (King Arthur Baking), à ajuster si tu as tes
propres repères. Le nom est reconnu en français et en anglais, sans accents ni pluriels (comme la
recherche) ; l'entrée la plus précise gagne (« sucre glace » avant « sucre », « beurre de
cacahuète » avant « beurre »).

**Solides** (1 tasse US =) :

| Ingrédient (fr / en) | 1 tasse = |
|---|---|
| farine / flour | 120 g |
| sucre, sucre en poudre / sugar | 200 g |
| sucre glace / powdered sugar, icing sugar | 115 g |
| cassonade, sucre roux, vergeoise / brown sugar | 215 g |
| beurre / butter | 225 g |
| beurre de cacahuète / peanut butter | 270 g |
| miel / honey | 335 g |
| sirop d'érable / maple syrup | 310 g |
| yaourt, fromage blanc / yogurt | 225 g |
| cacao / cocoa | 85 g |
| maïzena, fécule / cornstarch | 115 g |
| poudre d'amande, amandes en poudre / almond flour | 95 g |
| flocons d'avoine / oats | 90 g |
| riz / rice | 200 g |
| semoule, couscous / semolina, couscous | 170 g |
| pépites de chocolat / chocolate chips | 170 g |
| noix, noix de pécan hachées / chopped walnuts, pecans | 115 g |
| raisins secs / raisins | 150 g |
| noix de coco râpée / shredded coconut | 85 g |
| fromage râpé / shredded cheese | 115 g |

**Petites quantités** (1 c. à c. =) : sel 6 g, levure chimique / baking powder 4 g,
bicarbonate / baking soda 5 g, levure boulangère sèche / dry yeast 3 g, cannelle et épices
moulues / cinnamon, ground spices 2,5 g, extrait de vanille / vanilla extract 4 g.

**Liquides** (1 l =) : eau / water 1 000 g, lait / milk 1 030 g, crème / cream 1 000 g,
huile / oil 920 g, vin / wine 990 g, bouillon / broth, stock 1 000 g, jus / juice 1 040 g,
lait de coco / coconut milk 1 000 g, café, thé / coffee, tea 1 000 g, vinaigre / vinegar
1 010 g, sauce soja / soy sauce 1 200 g, rhum / rum 940 g.

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
5. Le **tableau des densités** te convient ?
6. La **remise en ordre** des 26 unités enregistrées avant la normalisation : d'accord ?
