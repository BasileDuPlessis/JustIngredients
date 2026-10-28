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
help-limits = Limite de taille de fichier : 10 Mo
help-commands = Commandes :
help-tips = Conseils :
help-tip1 = • Utilisez des images claires et bien éclairées
help-tip2 = • Assurez-vous que le texte est lisible et pas trop petit
help-tip3 = • Évitez les images floues ou déformées
help-tip4 = • Langues supportées : Anglais + Français
help-final = Besoin d'aide ? Envoyez-moi simplement une image ! 😊

# Commandes (menu Telegram, messages d'accueil et d'aide)
command-list = Mes recettes
command-help = Aide et conseils
command-cancel = Annuler l'action en cours
search-coming-soon = La recherche arrive bientôt. En attendant, /liste affiche toutes vos recettes.
unknown-command = Je ne connais pas cette commande. Voici celles que je connais :
private-chat-only = Je fonctionne seulement en conversation privée : écrivez-moi directement.
button-expired = Ce bouton n'est plus actif. Renvoyez la photo, ou ouvrez la recette depuis l'accueil.

# Message d'accueil (fin de chaque action)
home-title = 🏠 Accueil
home-no-recipes = Vous n'avez pas encore de recette.
home-one-recipe = Vous avez 1 recette.
home-recipe-count = Vous avez {$count} recettes.
home-send-photo = 📸 Envoyez une photo d'une liste d'ingrédients pour ajouter une recette.
home-all-recipes = 📚 Toutes mes recettes

# Messages d'erreur
error-download-failed = Échec du téléchargement de l'image. Veuillez réessayer.
error-unsupported-format = Format d'image non supporté. Veuillez utiliser les formats PNG, JPG, JPEG, BMP, TIFF ou TIF.
error-no-text-found = Aucun texte n'a été trouvé dans l'image. Essayez avec une image plus claire contenant du texte visible.
error-validation = La validation de l'image a échoué : {$msg}
error-image-load = Le format d'image n'est pas supporté ou l'image est corrompue. Essayez avec une image PNG, JPG ou BMP.

# Ingredient extraction (Gemini)
error-extraction-unavailable = La lecture des recettes est momentanément indisponible. Réessayez dans quelques minutes.
error-extraction-busy = Trop de photos en même temps. Réessayez dans une minute.
error-extraction-daily-limit = Vous avez atteint le nombre de photos lues pour aujourd'hui. Réessayez demain.
error-extraction-unsupported-image = Ce fichier n'est pas une image que je sais lire. Envoyez une photo (JPEG, PNG ou WebP).
error-extraction-image-too-large = Cette image est trop lourde (10 Mo maximum). Envoyez une photo plus légère.
error-extraction-unreadable = Je n'ai pas réussi à lire cette image. Essayez une photo plus nette de la liste d'ingrédients.
error-no-mime-type = Je ne reconnais pas ce type de fichier. Envoyez une photo (JPEG, PNG ou WebP).
extraction-not-a-recipe = Je ne vois pas de liste d'ingrédients sur cette image. Envoyez une photo des ingrédients d'une recette.

# Messages de succès
success-extraction = ✅ Texte extrait avec succès !
success-extracted-text = 📝 Texte extrait :
success-photo-downloaded = Photo téléchargée avec succès ! Traitement en cours...
success-document-downloaded = Document image téléchargé avec succès ! Traitement en cours...

# Messages de traitement des ingrédients
ingredients-found = Ingrédients trouvés !
no-ingredients-found = Aucun ingrédient détecté
no-ingredients-suggestion = Je n'ai pas pu trouver de mesures ou d'ingrédients dans le texte. Essayez d'envoyer une image plus claire d'une recette ou d'une liste d'ingrédients.
line = Ligne
unknown-ingredient = Ingrédient inconnu
total-ingredients-found = Total des ingrédients trouvés
original-text = Texte extrait original
error-processing-failed = Échec du traitement des ingrédients
save-failed-retry = L'enregistrement a échoué, rien n'a été enregistré. Réessayez dans un instant.
error-try-again = Veuillez réessayer avec une image différente.

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

# Réponses texte régulières
text-response = Reçu : {$text}
text-tip = 💡 Conseil : Envoyez-moi une photo de la liste d'ingrédients d'une recette, je la lirai pour vous !

# Messages de dialogue pour le nom de recette
recipe-name-prompt = 🏷️ Comment souhaitez-vous nommer cette recette ?
recipe-name-prompt-hint = Veuillez entrer un nom pour votre recette (par ex. "Cookies aux pépites de chocolat", "Lasagnes de Maman")
recipe-name-invalid = Le nom de recette ne peut pas être vide. Veuillez entrer un nom valide pour votre recette.
recipe-name-too-long = Le nom de recette est trop long (maximum 255 caractères). Veuillez entrer un nom plus court.
recipe-complete = ✅ Recette "{$recipe_name}" sauvegardée avec succès avec {$ingredient_count} ingrédients !

# Messages de révision des ingrédients
review-title = Révisez vos ingrédients
review-description = Veuillez réviser les ingrédients extraits ci-dessous. Utilisez les boutons pour modifier ou supprimer des éléments, puis confirmez quand vous êtes prêt.
review-confirm = Confirmer et sauvegarder
review-cancelled = Révision des ingrédients annulée. Aucun ingrédient n'a été sauvegardé.
review-no-ingredients = Aucun ingrédient restant
review-no-ingredients-help = Tous les ingrédients ont été supprimés. Vous pouvez ajouter plus d'ingrédients en envoyant une autre image, ou annuler cette recette.
review-add-more = Ajouter plus d'ingrédients
edit-ingredient-prompt = Entrez le texte d'ingrédient corrigé
current-ingredient = Ingrédient actuel
edit-empty = Le texte d'ingrédient ne peut pas être vide.
edit-invalid-format = Format d'ingrédient invalide. Veuillez entrer quelque chose comme "2 tasses de farine" ou "3 œufs".
edit-try-again = Veuillez réessayer avec un format d'ingrédient valide.
edit-too-long = Le texte d'ingrédient est trop long (maximum 200 caractères). Veuillez entrer une description plus courte.
edit-no-ingredient-name = Veuillez spécifier un nom d'ingrédient (par ex. "2 tasses de farine" et non pas seulement "2 tasses").
edit-ingredient-name-too-long = Le nom d'ingrédient est trop long (maximum 100 caractères). Veuillez utiliser un nom plus court.
edit-invalid-quantity = Quantité invalide. Veuillez utiliser un nombre positif (par ex. "2,5 tasses de farine").
quantity-correction-prompt = Nous n'avons pas pu lire la quantité exacte pour {$ingredient}. Veuillez taper la quantité (ou - s'il n'y en a pas) :
review-quantities-to-check = ⚠️ Vérifiez la quantité de : {$ingredients}. Touchez un ingrédient pour le corriger, puis confirmez à nouveau.
error-invalid-edit = Index d'ingrédient invalide pour l'édition.
confirm = Confirmer
cancel = Annuler
review-help = Utilisez les boutons, ou répondez "valider" pour enregistrer ces ingrédients ou "annuler" pour abandonner.

# Messages de document
document-image = Document image reçu de l'utilisateur {$user_id}
document-non-image = Document non-image reçu de l'utilisateur {$user_id}
document-no-mime = Document sans type MIME reçu de l'utilisateur {$user_id}

# Messages photo
photo-received = Photo reçue de l'utilisateur {$user_id}

# Messages texte
text-received = Message texte reçu de l'utilisateur {$user_id} : {$text}

# Messages non supportés
unsupported-received = Type de message non supporté reçu de l'utilisateur {$user_id}

# Messages de pagination
previous = Précédent
next = Suivant
page = Page
of = sur

# Messages de commande recettes
no-recipes-found = Aucune recette trouvée
no-recipes-suggestion = Envoyez-moi des images d'ingrédients pour créer votre première recette !
your-recipes = Vos Recettes
select-recipe = Sélectionnez une recette pour voir ses ingrédients :
recipe-details-coming-soon = Détails de la recette bientôt disponibles !

# Messages de workflow post-confirmation
workflow-recipe-saved = ✅ Recette sauvegardée avec succès !
workflow-what-next = Que souhaitez-vous faire ensuite ?
workflow-list-recipes = Lister mes recettes
workflow-search-recipes = Rechercher des recettes
workflow-search-coming-soon = Recherche de recettes bientôt disponible ! Pour l'instant, utilisez le bouton 'Lister mes recettes'.
caption-recipe-saved = Recette sauvegardée sous : "{$recipe_name}"

# Messages de gestion des recettes dupliquées
multiple-recipes-found = {$count} recettes trouvées avec ce nom :
select-recipe-instance = Sélectionnez quelle recette consulter :
recipe-created = Créé : {$date}
recipe-details-title = 📖 Détails de la recette
recipe-actions = Que souhaitez-vous faire ?
edit-recipe-name = Renommer la recette
edit-ingredients = Modifier les ingrédients
delete-recipe = Supprimer la recette
back-to-recipes = Retour aux recettes
recipe-statistics = Statistiques de recette
recipe-statistics-title = Statistiques de Recette
recipe-details = Détails de la Recette
ingredients-count = Ingrédients
created-date = Créé
your-statistics = Vos Statistiques
total-recipes = Total Recettes
total-ingredients = Total Ingrédients
avg-ingredients-per-recipe = Moy. Ingrédients/Recette
recent-activity = Activité Récente
recipes-today = Recettes Aujourd'hui
recipes-this-week = Recettes Cette Semaine
favorite-units = Unités Préférées
back-to-recipe = Retour à la Recette

# Messages de gestion de recette
rename-recipe-title = Renommer la recette
rename-recipe-instructions = Entrez le nouveau nom pour cette recette :
current-recipe-name = Nom actuel
delete-recipe-title = Supprimer la recette
delete-recipe-confirmation = Êtes-vous sûr de vouloir supprimer cette recette ? Cette action ne peut pas être annulée.
recipe-deleted = Recette supprimée avec succès
recipe-deleted-help = La recette et tous ses ingrédients ont été supprimés définitivement.
delete-cancelled = Suppression de recette annulée
rename-cancelled = Renommage annulé. La recette garde son nom.
rename-recipe-success = Recette renommée avec succès
rename-recipe-success-details = Recette renommée de "{$old_name}" à "{$new_name}"

# Messages de consultation des recettes
# Messages de visualisation de recette
recipe-not-found = Recette introuvable
recipe-not-found-help = Cette recette a peut-être été supprimée ou vous n'y avez pas accès.

# Messages d'erreur pour les opérations de recette
error-deleting-recipe = Échec de la suppression de la recette
error-deleting-recipe-help = Une erreur s'est produite lors de la suppression de la recette. Veuillez réessayer plus tard.
error-renaming-recipe = Échec du renommage de la recette
error-renaming-recipe-help = Une erreur s'est produite lors du renommage de la recette. Veuillez réessayer plus tard.

# Messages d'édition d'ingrédients
editing-recipe = Modification de la recette
editing-instructions = Utilisez les boutons ci-dessous pour modifier ou supprimer des ingrédients, puis confirmez vos modifications.
ingredients-updated = Ingrédients mis à jour avec succès
ingredients-updated-help = Les ingrédients de votre recette ont été mis à jour.
no-changes-made = Aucune modification n'a été apportée aux ingrédients.
editing-cancelled = Édition des ingrédients annulée
use-buttons-instruction = Utilisez les boutons sous la liste d'ingrédients pour modifier la recette.
no-ingredients-to-edit = Aucun ingrédient à modifier
no-ingredients-to-edit-help = Cette recette n'a pas d'ingrédients à modifier. Essayez d'ajouter des ingrédients d'abord.
error-updating-ingredients = Échec de la mise à jour des ingrédients
error-adding-ingredients = Échec de l'ajout de nouveaux ingrédients
error-deleting-ingredients = Échec de la suppression des ingrédients
add-ingredient = Ajouter un ingrédient
add-ingredient-prompt = Envoyez-moi le nouvel ingrédient (ex: "2 tasses de farine" ou "3 œufs")
ingredient-added = Ingrédient ajouté avec succès !

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

# Messages de légende photo
caption-used = 📝 Utilisation de la légende de la photo comme nom de recette : "{$caption}"
caption-invalid = La légende de la photo était invalide, utilisation du nom par défaut : "{$default_name}"
