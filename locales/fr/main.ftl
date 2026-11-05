# Ingredients Bot - Localisation Française
# Messages principaux de bienvenue et d'aide

welcome-title = Bienvenue sur Ingredients Bot !
welcome-description = Je suis votre assistant recettes : envoyez-moi une photo d'une liste d'ingrédients et je la transforme en recette enregistrée. Voici ce que je peux faire :
welcome-features =
    📸 Envoyez-moi des photos de listes d'ingrédients, de recettes ou de tout texte à extraire
    📄 Envoyez-moi des fichiers image (PNG, JPG, JPEG, BMP, TIFF, TIF)
    🔍 Je lirai les ingrédients et vous les ferai vérifier avant l'enregistrement
    💾 Tout texte extrait est stocké pour référence future
welcome-commands = Commandes :
welcome-send-image = Envoyez-moi simplement une image et je m'occupe du reste ! 🚀

help-title = 🆘 Aide d'Ingredients Bot
help-description = Comment m'utiliser :
help-step1 = 1. 📸 Envoyer une photo de texte à extraire (la légende devient le nom de la recette)
help-step2 = 2. 📎 Ou envoyer un fichier image (PNG, JPG, JPEG, BMP, TIFF, TIF)
help-step3 = 3. ⏳ Je lis les ingrédients sur l'image
help-step4 = 4. 📝 Vous recevrez le texte extrait
help-formats = Formats supportés : PNG, JPG, JPEG, BMP, TIFF, TIF
help-commands = Commandes :
help-tips = Conseils :
help-tip1 = • Utilisez des images claires et bien éclairées
help-tip2 = • Assurez-vous que le texte est lisible et pas trop petit
help-tip3 = • Évitez les images floues ou déformées
help-tip4 = • Langues supportées : Anglais + Français
help-final = Besoin d'aide ? Envoyez-moi simplement une image ! 😊

# Commandes (menu Telegram, messages d'accueil et d'aide)
command-list = Mes recettes
command-search = Chercher une recette
command-units = Affichage des unités
command-help = Aide et conseils
command-cancel = Annuler l'action en cours
unknown-command = Je ne connais pas cette commande. Voici celles que je connais :
private-chat-only = Je fonctionne seulement en conversation privée : écrivez-moi directement.
button-expired = Ce bouton n'est plus actif. Renvoyez la photo, ou ouvrez la recette depuis la liste en bas.

# Message d'accueil (fin de chaque action)
home-no-recipes = Vous n'avez pas encore de recette.
home-one-recipe = Vous avez 1 recette.
home-recipe-count = Vous avez {$count} recettes.
home-send-photo = 📸 Envoyez une photo d'une liste d'ingrédients pour ajouter une recette.

# Messages d'erreur
error-download-failed = Échec du téléchargement de l'image. Veuillez réessayer.
error-unsupported-format = Format d'image non supporté. Veuillez utiliser les formats PNG, JPG, JPEG, BMP, TIFF ou TIF.

# Ingredient extraction (Gemini)
error-extraction-unavailable = La lecture des recettes est momentanément indisponible. Réessayez dans quelques minutes.
error-extraction-busy = Trop de photos en même temps. Réessayez dans une minute.
error-extraction-daily-limit = Vous avez atteint le nombre de photos lues pour aujourd'hui. Réessayez demain.
error-extraction-unsupported-image = Ce fichier n'est pas une image que je sais lire. Envoyez une photo (JPEG, PNG ou WebP).
error-extraction-image-too-large = Cette image est trop lourde (10 Mo maximum). Envoyez une photo plus légère.
error-extraction-unreadable = Je n'ai pas réussi à lire cette image. Essayez une photo plus nette de la liste d'ingrédients.
error-no-mime-type = Je ne reconnais pas ce type de fichier. Envoyez une photo (JPEG, PNG ou WebP).
extraction-not-a-recipe = Je ne vois pas de liste d'ingrédients sur cette image. Envoyez une photo des ingrédients d'une recette.

# Messages de traitement des ingrédients
no-ingredients-found = Aucun ingrédient détecté
no-ingredients-suggestion = Je n'ai pas pu trouver de mesures ou d'ingrédients dans le texte. Essayez d'envoyer une image plus claire d'une recette ou d'une liste d'ingrédients.
unknown-ingredient = Ingrédient inconnu
error-processing-failed = Échec du traitement des ingrédients
save-failed-retry = L'enregistrement a échoué, rien n'a été enregistré. Réessayez dans un instant.

# Messages de traitement
processing-reading = ⏳ Lecture de la recette…

# Types de messages non supportés
unsupported-title = 🤔 Je ne peux traiter que les messages texte et les images.
unsupported-description = Ce que je peux faire :
unsupported-feature1 = 📸 Envoyer des photos de texte à extraire
unsupported-feature2 = 📄 Envoyer des fichiers image (PNG, JPG, JPEG, BMP, TIFF, TIF)
unsupported-feature3 = 💬 Envoyer /start pour voir le message de bienvenue
unsupported-feature4 = ❓ Envoyer /aide pour des instructions détaillées
unsupported-final = Essayez d'envoyer une image avec du texte ! 📝

# Messages de dialogue pour le nom de recette
recipe-name-prompt = 🏷️ Comment souhaitez-vous nommer cette recette ?
recipe-name-prompt-hint = Veuillez entrer un nom pour votre recette (par ex. "Cookies aux pépites de chocolat", "Lasagnes de Maman")
recipe-name-invalid = Le nom de recette ne peut pas être vide. Veuillez entrer un nom valide pour votre recette.
recipe-name-too-long = Le nom de recette est trop long (maximum 255 caractères). Veuillez entrer un nom plus court.

# Messages de révision des ingrédients
review-title = Révisez vos ingrédients
review-description = Touchez un ingrédient pour changer sa quantité, son nom ou le supprimer, puis validez avec ✅.
review-confirm = Confirmer et sauvegarder
review-no-ingredients = Aucun ingrédient restant
review-no-ingredients-help = Tous les ingrédients ont été supprimés. Ajoutez-en avec ➕, ou annulez avec ❌.
review-empty-confirm = Ajoutez au moins un ingrédient avec ➕ avant de valider, ou annulez avec ❌.
review-add-more = Ajouter plus d'ingrédients
edit-empty = Le texte d'ingrédient ne peut pas être vide.
edit-try-again = Veuillez réessayer avec un format d'ingrédient valide.
edit-too-long = Le texte d'ingrédient est trop long (maximum 200 caractères). Veuillez entrer une description plus courte.
edit-no-ingredient-name = Veuillez spécifier un nom d'ingrédient (par ex. "2 tasses de farine" et non pas seulement "2 tasses").
edit-one-line = Une ligne à la fois : tapez un seul ingrédient.
edit-ingredient-name-too-long = Le nom d'ingrédient est trop long (maximum 100 caractères). Veuillez utiliser un nom plus court.
edit-invalid-quantity = Quantité invalide. Veuillez utiliser un nombre positif (par ex. "2,5 tasses de farine").
edit-invalid-quantity-answer = Quantité non reconnue. Tapez un nombre, avec ou sans unité (par ex. 250, 250 g, 1/2), ou - s'il n'y en a pas.
edit-invalid-input = Saisie non reconnue.
review-quantities-to-check = ⚠️ Vérifiez la quantité de : {$ingredients}. Touchez un ingrédient pour le corriger, puis confirmez à nouveau.
error-invalid-edit = Index d'ingrédient invalide pour l'édition.
confirm = Confirmer
cancel = Annuler
review-not-understood = 🤔 Je n'ai pas compris ce message. Pour corriger un ingrédient, touchez-le dans la liste ci-dessous, puis « 🔢 Quantité », « 🏷️ Nom » ou « ✏️ Toute la ligne ». ✅ enregistre la recette, ❌ l'abandonne.

# Messages de pagination
previous = Précédent
next = Suivant
page = Page
of = sur

# Messages de commande recettes
your-recipes = Vos recettes
search-prompt = 🔍 Tapez un nom de recette ou un ingrédient, par exemple « farine ».
search-no-results = 🔍 Aucune recette ne correspond à « {$query} »
search-one-result = 🔍 1 recette pour « {$query} »
search-results = 🔍 {$count} recettes pour « {$query} »
search-more-results = Les {$shown} premières sont affichées : précisez votre recherche.
list-search-hint = 🔍 Tapez un mot pour chercher une recette.
search-failed = 🔍 La recherche n'a pas abouti. Réessayez dans un instant.
free-text-tip = 📸 Pour ajouter une recette, envoyez une photo de sa liste d'ingrédients.
    🔍 Pour chercher une recette, tapez un ou deux mots.
recipes-list-button = 📚 Liste des recettes

# Messages de workflow post-confirmation
workflow-what-next = Que souhaitez-vous faire ensuite ?

# Messages de gestion des recettes dupliquées
edit-recipe-name = Renommer la recette
edit-ingredients = Modifier les ingrédients
delete-recipe = Supprimer la recette
back-to-recipes = Retour aux recettes

# Messages de gestion de recette
rename-recipe-title = Renommer la recette
rename-recipe-instructions = Entrez le nouveau nom pour cette recette :
current-recipe-name = Nom actuel

# Note d'une recette : une info courte et libre (four, cuisson, lien de la source…)
edit-recipe-note = Note
note-prompt-create = Une info à noter pour « {$name} » ?
note-prompt-edit = Note de « {$name} »
note-prompt-hint = Par exemple : four 180 °C, 25 min, ou le lien de la recette. Une seule ligne, 200 caractères au maximum. Écrivez-la ci-dessous.
note-current = Note actuelle : {$note}
note-none-yet = Pas encore de note.
note-skip-button = Passer
note-clear-button = Effacer la note
note-empty = La note est vide. Écrivez quelques mots, ou utilisez le bouton ci-dessous.
note-too-long = La note est trop longue (200 caractères au maximum). Raccourcissez-la.
note-saved-line = 🗒️ Note enregistrée pour « {$name} »
note-cleared-line = 🗒️ Note effacée pour « {$name} »
delete-recipe-title = Supprimer la recette
delete-recipe-confirmation = Êtes-vous sûr de vouloir supprimer cette recette ? Cette action ne peut pas être annulée.
delete-confirm-button = 🗑️ Oui, supprimer
recipe-created-line = ✅ Recette « {$name} » créée
recipe-renamed-line = ✏️ « {$old_name} » renommée en « {$new_name} »
recipe-edited-line = 📝 « {$name} » modifiée
recipe-deleted-line = 🗑️ « {$name} » supprimée

# Messages de visualisation de recette
recipe-not-found = Recette introuvable

# Messages d'erreur pour les opérations de recette
error-deleting-recipe = Échec de la suppression de la recette
error-deleting-recipe-help = Une erreur s'est produite lors de la suppression de la recette. Veuillez réessayer plus tard.
error-renaming-recipe = Échec du renommage de la recette
error-renaming-recipe-help = Une erreur s'est produite lors du renommage de la recette. Veuillez réessayer plus tard.

# Messages d'édition d'ingrédients
editing-recipe = Modification de la recette
editing-instructions = Touchez un ingrédient pour changer sa quantité, son nom ou le supprimer, ➕ pour en ajouter un, puis enregistrez avec ✅.
editing-not-understood = 🤔 Je n'ai pas compris ce message. Pour corriger un ingrédient, touchez-le dans la liste ci-dessous, puis « 🔢 Quantité », « 🏷️ Nom » ou « ✏️ Toute la ligne ». ✅ enregistre les modifications, ❌ les annule.
no-ingredients-to-edit = Aucun ingrédient à modifier
no-ingredients-to-edit-help = Cette recette n'a pas d'ingrédients à modifier. Essayez d'ajouter des ingrédients d'abord.
error-updating-ingredients = Échec de la mise à jour des ingrédients
add-ingredient = Ajouter un ingrédient
add-ingredient-prompt = Envoyez-moi le nouvel ingrédient (ex: "2 tasses de farine" ou "3 œufs")

# Messages d'interface d'édition focalisée
edit-ingredient-title = Modifier l'ingrédient
edit-ingredient-current = Actuel
edit-ingredient-instruction = Entrez le nouveau texte d'ingrédient (ex: "3 tasses de blé entier") :
ingredient-menu-quantity = 🔢 Quantité
ingredient-menu-name = 🏷️ Nom
ingredient-menu-line = ✏️ Toute la ligne
ingredient-menu-delete = 🗑️ Supprimer
ingredient-menu-back = ⬅️ Retour
edit-quantity-instruction = Tapez la nouvelle quantité (ou - s'il n'y en a pas) :
edit-name-instruction = Tapez le nouveau nom de l'ingrédient :

# Unit views (UX plan U8)
unit-view-title = ⚖️ Affichage des unités
unit-view-explain = Les recettes ne changent pas : seul l'affichage des fiches suit ce choix.
unit-view-recipe = Comme dans la recette
unit-view-weight = Tout en poids (g)
unit-view-french = Unités françaises (g, cl)
unit-view-us = Unités américaines (cups, oz)
unit-view-chosen = ⚖️ Unités : { $view }
unit-view-footer-weight = ⚖️ Vue poids (g), valeurs arrondies
unit-view-footer-french = ⚖️ Vue unités françaises (g, cl), valeurs arrondies
unit-view-footer-us = ⚖️ Vue unités américaines, valeurs arrondies

# Linked recipes (UX plan U9)
review-parts-intro = 🧩 Cette photo contient { $count } parties : elles deviendront { $count } recettes liées. La première prend le nom de la recette, les autres « Nom (partie) ».
review-part-main = 🧩 La recette
review-part-main-heading = 🧩 La recette : { $heading }
review-part-linked = 🧩 Recette liée : { $heading }
recipe-created-linked-line = ✅ Recette « { $name } » créée, liée à « { $main } »
card-linked-recipes = 🔗 Recettes liées : { $names }
