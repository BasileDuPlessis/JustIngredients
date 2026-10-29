# Fil de discussion : modèle et wireframes (à valider)

Version du 2026-10-29, après le test de la v25 sur staging. Ce document remplace la règle
« la liste toujours en dernier » de la phase U5. Validé par l'utilisateur le 2026-10-29
(A oui, B non, C oui, D résolue) : c'est la phase U6 du plan.

## La règle en une phrase

Le fil a deux zones :

- **l'historique**, en haut, **sans aucun bouton** : soit des fiches recettes, soit des lignes
  de résumé d'action (« ✅ créée », « ✏️ renommée », « 📝 modifiée », « 🗑️ supprimée ») ;
- **le dernier message**, en bas, le seul qui a des boutons : soit **la liste des recettes**,
  soit **l'action en cours** avec ses boutons.

Le message du bas a **un seul bouton de sortie**, toujours sur sa dernière ligne :

- **« ❌ Annuler »** pendant une action qui enregistre quelque chose (relecture d'une photo,
  question du nom, renommage, modification des ingrédients, confirmation de suppression) : elle
  est abandonnée sans rien enregistrer, et on revient d'un cran — à la liste pour une création,
  à la fiche (de nouveau active) pour une action lancée depuis une fiche ;
- **« 📚 Liste des recettes »** quand rien n'est en cours (une fiche affichée, l'aide, une
  erreur) : ses boutons disparaissent, ce qu'il affichait passe dans l'historique (ou disparaît
  si ce n'est ni une recette ni un résumé), et la liste revient en bas.

Légende des wireframes : `[ … ]` est un bouton, `───` sépare les messages, `▼ bas du fil`
marque le dernier message.

## 1. Au repos : la liste en bas

```
┌──────────────────────────────────────────┐
│ ✅ Recette « Tarte aux pommes » créée    │   historique
├──────────────────────────────────────────┤
│ Crêpes                                   │   (fiche, sans bouton)
│ 1. 250 g → farine                        │
│ 2. 3 → œufs                              │
├──────────────────────────────────────────┤
│ 🗑️ « Pain » supprimée                    │
├──────────────────────────────────────────┤
│ 📚 Vos recettes                          │ ▼ bas du fil
│ Vous avez 12 recettes.                   │
│ 📸 Envoyez une photo pour en ajouter.    │
│ [ Tarte aux pommes ]                     │
│ [ Crêpes ]                               │
│ [ Tarte aux pommes (2) ]                 │
│ [ ⬅️ ]  [ Page 1 sur 2 ]  [ ➡️ ]          │
└──────────────────────────────────────────┘
```

Pas de bouton « 📚 Liste des recettes » : la liste est déjà affichée.

## 2. J'ouvre une recette

Je touche « Crêpes » dans la liste. **La liste disparaît** et la fiche prend sa place en bas,
avec ses boutons et le bouton de la liste. Rien en dessous.

```
┌──────────────────────────────────────────┐
│ ✅ Recette « Tarte aux pommes » créée    │   historique
├──────────────────────────────────────────┤
│ Crêpes                                   │ ▼ bas du fil
│ Créée le 27 octobre 2026 à 21:36         │
│ 1. 250 g → farine                        │
│ 2. 3 → œufs                              │
│ [ ✏️ Renommer ] [ 📝 Modifier ]           │
│ [ 🗑️ Supprimer ]                         │
│ [ 📚 Liste des recettes ]                │
└──────────────────────────────────────────┘
```

« ⬅️ Retour » devient « 📚 Liste des recettes ».

## 3. Je reviens à la liste depuis la fiche

Je touche « 📚 Liste des recettes ». **La fiche perd ses boutons** et reste dans l'historique ;
la liste revient en bas.

```
┌──────────────────────────────────────────┐
│ ✅ Recette « Tarte aux pommes » créée    │   historique
├──────────────────────────────────────────┤
│ Crêpes                                   │   (fiche, plus aucun bouton)
│ Créée le 27 octobre 2026 à 21:36         │
│ 1. 250 g → farine                        │
│ 2. 3 → œufs                              │
├──────────────────────────────────────────┤
│ 📚 Vos recettes … [ Tarte aux pommes ] … │ ▼ bas du fil
└──────────────────────────────────────────┘
```

## 4. Je renomme une recette

Depuis la fiche (écran 2), je touche « ✏️ Renommer ». La question remplace les boutons de la
fiche : la fiche passe dans l'historique, la question est l'action en cours.

```
┌──────────────────────────────────────────┐
│ Crêpes  (fiche, sans bouton)             │   historique
├──────────────────────────────────────────┤
│ 🏷️ Renommer « Crêpes »                   │ ▼ bas du fil
│ Tapez le nouveau nom :                   │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

« ❌ Annuler » : la question disparaît, la fiche redevient le message du bas avec ses boutons
(écran 2).

Je tape « Crêpes de Mamie ». Ma réponse et la question disparaissent :

```
┌──────────────────────────────────────────┐
│ Crêpes de Mamie  (fiche mise à jour)     │   historique
├──────────────────────────────────────────┤
│ ✏️ « Crêpes » renommée en                │
│    « Crêpes de Mamie »                   │
├──────────────────────────────────────────┤
│ 📚 Vos recettes …                        │ ▼ bas du fil
└──────────────────────────────────────────┘
```

(Question A ci-dessous : après le renommage, la liste en bas, ou la fiche de nouveau active ?)

## 5. Je modifie les ingrédients

Depuis la fiche, « 📝 Modifier ». La fiche passe dans l'historique, la vue d'édition est
l'action en cours :

```
┌──────────────────────────────────────────┐
│ Crêpes  (fiche, sans bouton)             │   historique
├──────────────────────────────────────────┤
│ ✏️ Modification de « Crêpes »            │ ▼ bas du fil
│ Touchez un ingrédient pour le changer…   │
│ [ 250 g farine ]                         │
│ [ 3 œufs ]                               │
│ [ ➕ Ajouter un ingrédient ]             │
│ [ ✅ Valider ]                           │
│ [ ❌ Annuler ]                           │
└──────────────────────────────────────────┘
```

« ❌ Annuler » : la vue d'édition disparaît, rien n'est enregistré, la fiche redevient active
(écran 2). Je valide : la vue d'édition disparaît, la fiche de l'historique est mise à jour, une ligne
s'ajoute, la liste revient.

```
│ Crêpes  (fiche mise à jour, sans bouton) │   historique
│ 📝 « Crêpes » modifiée                   │
│ 📚 Vos recettes …                        │ ▼ bas du fil
```

## 6. Je supprime une recette

Depuis la fiche, « 🗑️ Supprimer » : la confirmation est l'action en cours.

```
│ Crêpes  (fiche, sans bouton)             │   historique
├──────────────────────────────────────────┤
│ 🗑️ Supprimer « Crêpes » ?                │ ▼ bas du fil
│ [ 🗑️ Oui, supprimer ]                    │
│ [ ❌ Annuler ]                           │
```

« Oui » : la fiche de l'historique devient la ligne de résumé, la liste revient.

```
│ 🗑️ « Crêpes » supprimée                  │   historique
│ 📚 Vos recettes …                        │ ▼ bas du fil
```

« ❌ Annuler » : retour à la fiche active (écran 2).

## 7. Je crée une recette (photo)

J'envoie une photo. Pendant la relecture, la photo et la liste des ingrédients sont l'action
en cours :

```
│ … historique …                           │
├──────────────────────────────────────────┤
│ (ma photo)                               │
├──────────────────────────────────────────┤
│ Révisez vos ingrédients                  │ ▼ bas du fil
│ [ 200 g farine ]                         │
│ [ 2 œufs ]                               │
│ [ ➕ Ajouter un ingrédient ]             │
│ [ ✅ Valider ]                           │
│ [ ❌ Annuler ]                           │
```

Je valide (et je tape le nom s'il est demandé) : la photo, la relecture et la question du nom
disparaissent.

```
│ … historique …                           │
│ ✅ Recette « Tarte » créée               │
│ 📚 Vos recettes …                        │ ▼ bas du fil
```

« ❌ Annuler » pendant la relecture : tout disparaît, la liste revient, rien dans l'historique
(une création annulée ne laisse rien). Toucher un ingrédient ouvre son menu (🔢, 🏷️, ✏️, 🗑️,
⬅️ Retour à la relecture) ; une retouche en cours a son propre « ❌ Annuler », qui revient à la
relecture : on revient toujours d'un cran.

## 8. Messages hors action

`/aide`, `/start`, une erreur (« ce n'est pas une recette »), un texte libre : le message et la
réponse s'affichent en bas avec le bouton « 📚 Liste des recettes », et disparaissent à
l'action suivante.

```
│ … historique …                           │
├──────────────────────────────────────────┤
│ 🍳 Aide : envoyez une photo…             │ ▼ bas du fil
│ [ 📚 Liste des recettes ]                │
```

## 9. Je choisis l'affichage des unités (`/unites`, phase U8)

Comme `/aide`, le sélecteur prend la place du message du bas ; il a ses propres boutons, dont
« 📚 Liste des recettes » en dernier. Un choix le transforme en ligne d'historique (« ⚖️
Unités : … »), et la fiche qui était en bas avant `/unites` revient dans la nouvelle vue (ou
la liste). Le détail est dans [measure-conversion.md](measure-conversion.md).

```
│ … historique …                           │
├──────────────────────────────────────────┤
│ ⚖️ Affichage des unités                  │ ▼ bas du fil
│ [ Comme dans la recette ]                │
│ [ Tout en poids (g) ]                    │
│ [ ✓ Unités françaises (g, cl) ]          │
│ [ Unités américaines (cups, oz) ]        │
│ [ 📚 Liste des recettes ]                │
```

## Ce que je retire de la v25 actuelle

- La liste n'est plus renvoyée en dessous de la fiche quand j'ouvre une recette (écran 2).
- Les fiches de l'historique n'ont plus jamais de boutons.
- Le bouton « ⬅️ Retour » de la fiche devient « 📚 Liste des recettes ».
- Un seul bouton de sortie en bas de chaque message actif : « ❌ Annuler » pendant une action,
  « 📚 Liste des recettes » sinon ; ✅ Valider et ❌ Annuler ne sont plus côte à côte.

## Questions

- **A.** Validé : la liste. Après un renommage ou une modification réussis : je propose la **liste** en bas
  (écrans 4 et 5). L'autre possibilité : la fiche redevient le message du bas avec ses boutons,
  pour enchaîner (renommer puis modifier).
- **B.** Décidé : **chaque consultation reste dans l'historique** (pas de retrait de l'ancienne
  copie). Si j'ouvre une recette déjà présente dans l'historique : je proposais de retirer
  l'ancienne copie ; l'utilisateur préfère garder chaque consultation.
- **C.** Validé : mises à jour. Les fiches de l'historique : je propose de les **mettre à jour** après un renommage ou
  une modification (elles montrent toujours la recette actuelle). L'autre possibilité : les
  garder telles qu'elles étaient au moment de l'affichage.
- **D.** Résolue (2026-10-29) : pendant une action, le seul bouton de sortie est « ❌ Annuler »
  (rien n'est enregistré), il n'y a pas de « 📚 Liste des recettes » en plus.
