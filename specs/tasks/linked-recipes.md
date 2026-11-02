# Recettes liées (validé)

Version du 2026-11-02. Remplace la proposition des sections d'ingrédients
([ingredient-sections.md](ingredient-sections.md), abandonnée). Demande de l'utilisateur :
« quand la recette en photo contient plusieurs sections, créer plusieurs recettes et mettre en
place un mécanisme de liens entre les recettes ; à la fin de la recette, une liste des recettes
liées (ex. malva pudding lié avec "sauce malva pudding") ; le titre de la recette liée est le
titre défini par l'utilisateur concaténé avec le nom de la section ». Rien n'est codé : ce
document décrit ce que je propose, avec des wireframes, et finit par les questions à trancher.

## Décisions (2026-11-02)

1. Découpage : d'office (choix A), « 🧩 Une seule recette » dans la relecture pour tout réunir.
2. Les noms : « Malva pudding (sauce) », la section entre parenthèses.
3. Le nom de section nettoyé (« Pour la sauce : » → « sauce »).
4. Les liens dans les deux sens.
5. Supprimer une recette ne supprime qu'elle (le lien disparaît).
6. Lier et délier à la main : plus tard.
7. La langue de la recette donnée par Gemini devait être enregistrée : écarté à la réalisation
   (la faire suivre jusqu'à l'enregistrement touchait une soixantaine d'endroits du code de la
   relecture) ; la vue « Comme dans la recette » garde sa détection par le texte de la photo.

## Pourquoi c'est une bonne approche

- Chaque partie est une vraie recette : elle a sa fiche, se cherche (« sauce » trouve la sauce),
  se convertit dans la vue d'unités, se modifie, et apparaît dans la liste.
- Le modèle du fil (U6) ne change pas : une fiche, une recette ; pas de titres ni de menus de
  section dans la relecture.
- Un lien sert aussi au-delà des photos : une « Pâte brisée » liée à plusieurs tartes.

## Le principe

1. **Lecture** : Gemini donne, pour chaque ingrédient, le titre de section sous lequel il est
   imprimé (aujourd'hui il a pour consigne de les ignorer). Sans titre, rien ne change.
2. **Découpage** : une photo à plusieurs sections donne plusieurs recettes. La première partie
   (les ingrédients sans titre, ou la première section) prend le titre choisi par l'utilisateur ;
   chaque autre section devient une recette liée nommée « Titre (section) » : « Malva pudding
   (sauce) ». Le titre d'abord : les recettes liées se suivent dans la liste alphabétique.
3. **Nom de section** : nettoyé de « Pour le / la / les / l' », « For the », des « : » ; première
   lettre en minuscule, le reste tel qu'imprimé (« Pour la sauce : » → « sauce », « Pour la
   crème Chantilly » → « crème Chantilly »). Un nom déjà pris est numéroté comme aujourd'hui
   (« Malva pudding (sauce) (2) »).
4. **Liens** : dans les deux sens. La fiche du malva pudding liste la sauce, la fiche de la sauce
   liste le malva pudding. Chaque recette reste indépendante : renommer, modifier, supprimer
   l'une ne touche pas l'autre (supprimer retire seulement le lien).

En base : une table `recipe_links (recipe_a, recipe_b)`, une ligne par paire de recettes d'un
même utilisateur, supprimée avec l'une des deux (migration 6). 

## Wireframes

Légende comme pour U6 : `[ … ]` est un bouton, `▼ bas du fil` marque le dernier message.

### 1. La relecture d'une photo à deux sections

Une seule relecture pour toute la photo : le texte montre les parties, les boutons restent un
par ingrédient, dans le même ordre. On corrige tout au même endroit, comme aujourd'hui.

```
┌──────────────────────────────────────────┐
│ 📝 Vérifiez les ingrédients              │ ▼ bas du fil
│ Cette photo contient 2 parties : elles   │
│ deviendront 2 recettes liées.            │
│                                          │
│ Malva pudding                            │
│ 1. 200 g → sucre                         │
│ 2. 2 → œufs                              │
│ 3. 1 c. à s. → confiture d'abricots      │
│ 4. 150 g → farine                        │
│ Sauce                                    │
│ 5. 25 cl → crème                         │
│ 6. 100 g → beurre                        │
│ [ 200 g → sucre ]                        │
│ [ 2 → œufs ]  …                          │
│ [ 25 cl → crème ]  …                     │
│ [ ➕ Ajouter ]                           │
│ [ 🧩 Une seule recette ]                 │
│ [ ✅ Valider ]                           │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

« 🧩 Une seule recette » réunit les parties (la relecture redevient celle d'aujourd'hui) ;
« ➕ Ajouter » ajoute à la dernière partie. Une partie dont on a supprimé tous les ingrédients
disparaît.

### 2. J'enregistre

Le nom est demandé comme aujourd'hui (ou pris de la légende ou du titre lu par Gemini) ; il
nomme la première partie. Il reste une ligne par recette créée.

```
┌──────────────────────────────────────────┐
│ ✅ Recette « Malva pudding » créée       │   historique
├──────────────────────────────────────────┤
│ ✅ Recette « Malva pudding (sauce) »     │
│    créée, liée à « Malva pudding »       │
├──────────────────────────────────────────┤
│ 📚 Vos recettes …                        │ ▼ bas du fil
│ [ Malva pudding ]                        │
│ [ Malva pudding (sauce) ]                │
└──────────────────────────────────────────┘
```

### 3. La fiche, avec ses recettes liées

Les recettes liées sont listées à la fin de la fiche et ont chacune un bouton, avant la sortie
« 📚 Liste des recettes ».

```
┌──────────────────────────────────────────┐
│ 📖 Malva pudding                         │ ▼ bas du fil
│                                          │
│ • 200 g sucre                            │
│ • 2 œufs                                 │
│ • 1 c. à s. confiture d'abricots         │
│ • 150 g farine                           │
│                                          │
│ 🔗 Recettes liées : Malva pudding (sauce)│
│ [ ✏️ Renommer ]  [ 📝 Modifier ]          │
│ [ 🗑️ Supprimer ]                          │
│ [ 🔗 Malva pudding (sauce) ]             │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

Toucher « 🔗 Malva pudding (sauce) » ouvre la sauce à la place, en bas ; la fiche du malva
pudding reste dans l'historique (comme toute consultation, règle B d'U6).

### 4. Lier ou délier à la main (si tu le veux maintenant)

Un bouton « 🔗 Lier » sur la fiche ouvre la liste des recettes (alphabétique, par pages) : en
choisir une crée le lien. Le menu d'une recette liée propose « Délier ». Utile pour une « Pâte
brisée » commune à plusieurs tartes, ou pour lier deux recettes déjà enregistrées.

## Ce qui ne change pas

- Une photo sans sections : exactement comme aujourd'hui.
- Les recettes déjà enregistrées : sans lien (à lier à la main si la partie 4 est retenue).
- La recherche, les vues d'unités, l'ordre alphabétique.

## Limites

- Au plus 5 parties par photo, et 10 recettes liées affichées par fiche (Telegram limite la
  taille des claviers).
- Un nom de recette liée suit la limite des noms (255 caractères) : le titre et la section sont
  coupés si besoin.

C'est la phase U9 du [plan UX](ux-improvements.md).
