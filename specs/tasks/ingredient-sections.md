# Sections d'ingrédients : proposition (abandonnée)

> Abandonnée le 2026-11-02 : l'utilisateur préfère une recette par section, liées entre elles.
> Voir [linked-recipes.md](linked-recipes.md).

Version du 2026-10-30. Demande de l'utilisateur : « pouvoir regrouper les ingrédients dans des
sections (ex. pour le malva pudding, des ingrédients pour le gâteau et une autre section des
ingrédients pour la sauce) mais dans la même recette ». Rien n'est codé : ce document décrit ce
que je propose, avec des wireframes, et finit par les questions à trancher.

## Ce qui existe aujourd'hui

- Une recette a une seule liste d'ingrédients, dans l'ordre de la photo (lecture de Gemini),
  les ajouts à la fin.
- Gemini voit les titres de sections (« Pour le gâteau », « Sauce »), mais la consigne lui dit
  de les ignorer (`config/extraction/prompt.md`).
- Gemini renvoie aussi la langue de la recette (`fr` / `en`), qui n'est pas enregistrée : la vue
  « Comme dans la recette » la devine aujourd'hui depuis le texte de la photo.

## Le principe

**Une section est un titre posé sur des ingrédients qui se suivent.** Chaque ingrédient peut
porter le nom de sa section ; une recette sans section s'affiche exactement comme aujourd'hui.
Les ingrédients gardent leur ordre, et les sections apparaissent dans l'ordre de leur premier
ingrédient.

En base : une colonne `ingredients.section` (texte, vide pour « sans section »), migration 6.
J'écarte une table `recipe_sections` à part : plus lourde (jointures, sections vides à gérer),
pour un besoin qui tient dans un nom par ingrédient. Renommer une section, c'est changer ce nom
sur ses ingrédients.

Profitant de la migration, `recipes.language` enregistre la langue que Gemini donne déjà : la vue
« Comme dans la recette » s'en sert au lieu de deviner (les recettes déjà enregistrées gardent
la devinette).

## Lecture d'une photo

La consigne de Gemini change : au lieu d'ignorer les titres de sections, il donne pour chaque
ingrédient le titre sous lequel il est imprimé (`section`, tel qu'écrit, sans « : » ; vide sans
titre). « Pour le gâteau » reste « Pour le gâteau » : on garde le texte de la recette.

Un titre qui n'est pas une section d'ingrédients (« Ingrédients », « Pour 6 personnes ») est
ignoré comme aujourd'hui. Une recette sur deux photos n'est pas concernée (une photo à la fois).

## Wireframes

Légende comme pour U6 : `[ … ]` est un bouton, `▼ bas du fil` marque le dernier message.

### 1. La relecture d'une photo avec sections

Le texte montre les sections en gras ; les boutons suivent le même ordre, chaque section
commençant par un bouton de titre qui ouvre le menu de la section.

```
┌──────────────────────────────────────────┐
│ 📝 Vérifiez les ingrédients              │ ▼ bas du fil
│                                          │
│ Pour le gâteau                           │
│ 1. 200 g → sucre                         │
│ 2. 2 → œufs                              │
│ 3. 1 c. à s. → confiture d'abricots      │
│ 4. 150 g → farine                        │
│ Pour la sauce                            │
│ 5. 25 cl → crème                         │
│ 6. 100 g → beurre                        │
│ [ 📂 Pour le gâteau ]                    │
│ [ 200 g → sucre ]                        │
│ [ 2 → œufs ]                             │
│ [ 1 c. à s. → confiture d'abricots ]     │
│ [ 150 g → farine ]                       │
│ [ 📂 Pour la sauce ]                     │
│ [ 25 cl → crème ]                        │
│ [ 100 g → beurre ]                       │
│ [ ➕ Ajouter ]                           │
│ [ ✅ Valider ]                           │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

### 2. Le menu d'une section (bouton « 📂 Pour la sauce »)

```
┌──────────────────────────────────────────┐
│ 📂 Pour la sauce                         │ ▼ bas du fil
│ [ ✏️ Renommer la section ]               │
│ [ 🗑️ Retirer la section ]                │
│ [ ⬅️ Retour ]                            │
└──────────────────────────────────────────┘
```

« Retirer la section » ne supprime aucun ingrédient : ils perdent leur titre et rejoignent la
section du dessus (ou « sans section » s'il n'y en a pas).

### 3. Le menu d'un ingrédient gagne « 📂 Section »

```
┌──────────────────────────────────────────┐
│ 25 cl → crème                            │ ▼ bas du fil
│ [ 🔢 Quantité ]  [ 🏷️ Nom ]              │
│ [ ✏️ Ligne entière ]  [ 🗑️ Supprimer ]    │
│ [ 📂 Section ]                           │
│ [ ⬅️ Retour ]                            │
└──────────────────────────────────────────┘

  📂 Section →
┌──────────────────────────────────────────┐
│ Dans quelle section mettre « crème » ?   │ ▼ bas du fil
│ [ Pour le gâteau ]                       │
│ [ ✓ Pour la sauce ]                      │
│ [ ➕ Nouvelle section ]                  │
│ [ Sans section ]                         │
│ [ ⬅️ Retour ]                            │
└──────────────────────────────────────────┘
```

Changer de section met l'ingrédient à la fin de sa nouvelle section. « ➕ Nouvelle section »
demande un nom (comme le renommage d'une recette) et crée la section avec cet ingrédient, à la
fin de la recette. C'est aussi le moyen de créer des sections dans une recette qui n'en a pas.

« ➕ Ajouter » ajoute l'ingrédient à la fin de la recette, donc dans la dernière section ; on le
change de section avec 📂 si besoin.

### 4. La fiche

```
┌──────────────────────────────────────────┐
│ 📖 Malva pudding                         │ ▼ bas du fil
│                                          │
│ Pour le gâteau                           │
│ • 200 g sucre                            │
│ • 2 œufs                                 │
│ • 1 c. à s. confiture d'abricots         │
│ • 150 g farine                           │
│                                          │
│ Pour la sauce                            │
│ • 25 cl crème                            │
│ • 100 g beurre                           │
│ [ ✏️ Renommer ]  [ 📝 Modifier ]          │
│ [ 🗑️ Supprimer ]                          │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

La modification d'une recette enregistrée (📝) montre les sections comme la relecture, avec les
mêmes menus.

## Ce qui ne change pas

- Les recettes déjà enregistrées : sans section, affichées comme aujourd'hui. Pour leur en
  donner : 📝 Modifier puis 📂 sur un ingrédient, ou renvoyer la photo.
- Les vues d'unités, la recherche par nom et ingrédient, l'ordre alphabétique de la liste.
- Une ligne tapée (ajout, ligne entière) reste un ingrédient : pas de syntaxe pour créer une
  section en tapant.

## Limites

- Un nom de section : 50 caractères au plus.
- Telegram limite un clavier à une centaine de boutons : une recette de 40 ingrédients en
  5 sections en utilise 50.

## Questions

1. **Lecture automatique** des sections sur la photo (Gemini) : oui ?
2. **Modifier les sections** : le menu de section (renommer, retirer) et le choix de section sur
   un ingrédient (dont « ➕ Nouvelle section ») te suffisent ? Ou faut-il aussi pouvoir déplacer
   une section entière avant une autre ?
3. Un ingrédient **changé de section** va à la fin de cette section : d'accord ?
4. Garder le **titre tel qu'imprimé** (« Pour le gâteau ») plutôt qu'un titre raccourci
   (« Gâteau ») : d'accord ?
5. La **recherche** doit-elle aussi trouver les noms de sections (« sauce » trouverait le malva
   pudding par sa section « Pour la sauce ») ? Je proposerais non.
6. Enregistrer la **langue** de la recette donnée par Gemini (pour la vue « Comme dans la
   recette ») en même temps : d'accord ?
