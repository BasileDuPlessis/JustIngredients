# Conversion des mesures : proposition (à valider)

Version du 2026-10-29, après la v26. Demande de l'utilisateur : « proposer de mettre toutes les
mesures en unités françaises : g / cl / l… » et « proposer un outil de conversion des volumes en
poids (ex. 1 cup butter ⇒ … g de beurre), avec la densité des principaux ingrédients (farine,
beurre, lait, sucre…) en constantes ». Rien n'est codé : ce document décrit ce que je propose,
avec des wireframes, et finit par les questions à trancher.

## Ce qui existe aujourd'hui

- Les unités sont **stockées sous une forme canonique** (`src/units.rs`) : `g`, `kg`, `mg`,
  `ml`, `l`, `cup`, `tbsp`, `tsp`, `lb`, `oz`, `pint`, `quart`, `gallon`, plus des unités de
  comptage gardées telles quelles (« gousses », « pincée »). Les cl et dl sont déjà convertis
  en ml à l'entrée (« 20 cl » est stocké 200 ml).
- Elles sont **affichées telles quelles**, aussi aux utilisateurs français : la fiche montre
  « 2 tbsp huile », « 1 cup farine », « 200 ml lait ».
- Aucune conversion entre unités n'existe (c'était hors périmètre de la standardisation des
  unités).

## La proposition en deux parties

### A. Afficher les unités en français (sans toucher aux données)

Pour un utilisateur français, l'affichage traduit les unités canoniques : `tbsp` → « c. à s. »,
`tsp` → « c. à c. », `cup` → « tasse », `lb` → « livre », `oz` → « once ». Les volumes en ml
s'affichent en cl à partir de 10 cl (« 200 ml » → « 20 cl », « 15 ml » reste « 15 ml »), et en l
à partir d'1 l. Rien ne change en base ni pour les utilisateurs anglais. C'est petit, sans
risque, et utile même sans conversion.

```
Avant                          Après (utilisateur français)
• 2 tbsp huile d'olive         • 2 c. à s. huile d'olive
• 1 cup farine                 • 1 tasse farine
• 200 ml lait                  • 20 cl lait
```

### B. Convertir une recette en g / cl / l

Un bouton **« ⚖️ g / cl »** propose de convertir les mesures américaines d'une recette :

| Mesure | Devient | Comment |
|---|---|---|
| lb, oz | g | facteur fixe (1 lb = 453,6 g, 1 oz = 28,35 g) |
| cup, pint, quart, gallon d'un **liquide** (lait, eau, crème, huile, vin, bouillon, jus…) | cl, ou l à partir d'1 l | volume (1 cup US = 23,7 cl) |
| cup, pint… d'un **solide connu** (farine, sucre, beurre…) | g | volume × densité du tableau ci-dessous |
| cup d'un solide **inconnu** (« noix de pécan entières ») | inchangé | signalé « pas de conversion » |
| c. à s., c. à c. | inchangé | les recettes françaises les gardent pour les petites quantités |
| g, kg, ml, l, unités de comptage, sans quantité | inchangé | |

Arrondis, pour des valeurs « de cuisine » : au gramme sous 20 g, aux 5 g au-dessus (227 g →
225 g) ; au demi-cl sous 10 cl, au cl au-dessus (236,6 ml → 24 cl) ; au dixième de litre à
partir d'1 l (3,785 l → 3,8 l).

#### Densités (constantes, grammes par tasse US)

Valeurs courantes des tableaux de pâtisserie (King Arthur Baking), à ajuster si tu as tes
propres repères. Le nom est reconnu en français et en anglais, sans accents ni pluriels (comme la
recherche) ; l'entrée la plus précise gagne (« sucre glace » avant « sucre », « beurre de
cacahuète » avant « beurre »).

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

Liquides (convertis en volume, sans densité) : lait, eau, crème, huile, vin, bouillon, jus,
lait de coco, café, thé, rhum, vinaigre, sauce soja / milk, water, cream, oil, wine, broth,
stock, juice, coconut milk, coffee, tea, rum, vinegar, soy sauce.

#### Où est le bouton

Selon le modèle du fil (U6) : la conversion est une action, avec un aperçu en bas, un seul
« ❌ Annuler », et une ligne de résumé dans l'historique une fois enregistrée. Le bouton
n'apparaît que si la recette a quelque chose à convertir.

## Wireframes

Légende comme pour U6 : `[ … ]` est un bouton, `▼ bas du fil` marque le dernier message.

### 1. La fiche d'une recette américaine

```
┌──────────────────────────────────────────┐
│ 📖 Chocolate chip cookies                │ ▼ bas du fil
│ 📅 29 oct. 2026                          │
│ • 2.25 tasse farine                      │
│ • 1 tasse beurre                         │
│ • 0.75 tasse sucre                       │
│ • 2 œufs                                 │
│ • 1 c. à c. sel                          │
│ • 1 tasse noix de pécan entières         │
│ [ ✏️ Renommer ]  [ 📝 Modifier ]          │
│ [ ⚖️ g / cl ]   [ 🗑️ Supprimer ]          │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

### 2. Je touche « ⚖️ g / cl » : l'aperçu remplace la fiche en bas

La fiche passe dans l'historique (sans boutons), l'aperçu est l'action en cours.

```
┌──────────────────────────────────────────┐
│ ⚖️ Chocolate chip cookies en g / cl      │ ▼ bas du fil
│ • 2.25 tasse farine → 270 g              │
│ • 1 tasse beurre → 225 g                 │
│ • 0.75 tasse sucre → 150 g               │
│ • 1 tasse noix de pécan entières :       │
│   pas de conversion (densité inconnue)   │
│ Les autres lignes ne changent pas.       │
│ Valeurs arrondies.                       │
│ [ ✅ Enregistrer ]                       │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

« ❌ Annuler » : l'aperçu disparaît, la fiche redevient le message du bas avec ses boutons.

### 3. J'enregistre

Comme après une modification (A et C du modèle U6) : la ligne de résumé reste, les fiches de la
recette dans l'historique sont mises à jour, la liste revient en bas.

```
┌──────────────────────────────────────────┐
│ 📖 Chocolate chip cookies                │   historique (fiche mise à jour)
│ • 270 g farine                           │
│ • 225 g beurre  …                        │
├──────────────────────────────────────────┤
│ ⚖️ « Chocolate chip cookies » convertie  │
├──────────────────────────────────────────┤
│ 📚 Vos recettes …                        │ ▼ bas du fil
└──────────────────────────────────────────┘
```

### 4. Pendant la relecture d'une photo

Une recette lue sur un site américain arrive en tasses : la relecture propose aussi
« ⚖️ g / cl ». Rien n'est encore enregistré, donc pas d'aperçu : la liste de relecture est
convertie sur place, on peut encore tout corriger, puis « ✅ Valider » comme d'habitude.

```
┌──────────────────────────────────────────┐
│ 📝 Vérifiez les ingrédients              │ ▼ bas du fil
│ [ 2.25 tasse → farine ]                  │
│ [ 1 tasse → beurre ]                     │
│ [ ➕ Ajouter ]  [ ⚖️ g / cl ]             │
│ [ ✅ Valider ]                           │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

## Plus tard, pas dans cette phase

- Un réglage « toujours convertir les photos en g / cl ».
- Convertir les cuillères en grammes pour les solides (« 2 c. à s. de beurre » → 30 g).
- Les températures (°F → °C) : elles sont dans les instructions, que le bot ne lit pas.

## Pour calibrer

Si tu veux savoir quelles unités tes recettes utilisent vraiment, cette requête sur la base de
production (lecture seule) les compte :

```sql
SELECT unit, count(*) FROM ingredients GROUP BY unit ORDER BY count(*) DESC;
```

## Questions

1. **A** (unités affichées en français, ml en cl) : oui ?
2. **B** : conversion **enregistrée** dans la recette, après un aperçu avec ✅ / ❌ (je le
   recommande : c'est simple, et la recette reste cohérente partout, y compris dans la
   recherche) ? Ou seulement un **affichage** converti, la recette gardant ses tasses ?
3. Le bouton sur la **fiche** et dans la **relecture** d'une photo, ou seulement sur la fiche ?
4. Les **cuillères** restent des cuillères (recommandé), ou faut-il aussi les convertir en g
   pour les solides ?
5. Le **tableau des densités** te convient-il (ingrédients et valeurs) ?
