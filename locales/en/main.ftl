# Ingredients Bot - English Localization
# Main welcome and help messages

welcome-title = Welcome to Ingredients Bot!
welcome-description = I'm your recipe assistant: send me a photo of an ingredient list and I'll turn it into a saved recipe. Here's what I can do:
welcome-features =
    📸 Send me photos of ingredient lists, recipes, or any text you want to extract
    🏷️ Add captions to automatically name your recipes
    📄 Send me image files (PNG, JPG, JPEG, BMP, TIFF, TIF)
    🔍 I'll read the ingredients and let you check them before saving
    💾 All extracted text is stored for future reference
welcome-commands = Commands:
welcome-send-image = Just send me an image and I'll do the rest! 🚀

help-title = 🆘 Ingredients Bot Help
help-description = How to use me:
help-step1 = 1. 📸 Send a photo of text you want to extract
help-step2 = 2. 🏷️ Add a caption to name your recipe (optional)
help-step3 = 3. 📎 Or send an image file (PNG, JPG, JPEG, BMP, TIFF, TIF)
help-step4 = 4. ⏳ I'll read the ingredients from the image
help-formats = Supported formats: PNG, JPG, JPEG, BMP, TIFF, TIF
help-commands = Commands:
help-tips = Tips:
help-tip1 = • Use clear, well-lit images
help-tip2 = • Ensure text is readable and not too small
help-tip3 = • Avoid blurry or distorted images
help-tip4 = • Add a caption to automatically name your recipe
help-final = Need help? Just send me an image! 😊

# Commands (Telegram menu, welcome and help messages)
command-list = My recipes
command-search = Search your recipes
command-units = How units are shown
command-help = Help and tips
command-cancel = Cancel the current action
unknown-command = I don't know this command. Here are the ones I know:
private-chat-only = I only work in private chats: message me directly.
button-expired = This button is no longer active. Send the photo again, or open the recipe from the list at the bottom.

# Home message (end of every action)
home-no-recipes = You have no recipe yet.
home-one-recipe = You have 1 recipe.
home-recipe-count = You have {$count} recipes.
home-send-photo = 📸 Send a photo of an ingredient list to add a recipe.

# Error messages
error-download-failed = Failed to download the image. Please try again.
error-unsupported-format = Unsupported image format. Please use PNG, JPG, JPEG, BMP, TIFF, or TIF formats.

# Ingredient extraction (Gemini)
error-extraction-unavailable = Reading recipes is temporarily unavailable. Please try again in a few minutes.
error-extraction-busy = Too many photos at the same time. Please try again in a minute.
error-extraction-daily-limit = You have reached today's number of photos. Please try again tomorrow.
error-extraction-unsupported-image = This file is not an image I can read. Please send a photo (JPEG, PNG or WebP).
error-extraction-image-too-large = This image is too large (10 MB maximum). Please send a smaller photo.
error-extraction-unreadable = I couldn't read this image. Please try a sharper photo of the ingredient list.
error-no-mime-type = I can't tell what kind of file this is. Please send a photo (JPEG, PNG or WebP).
extraction-not-a-recipe = I don't see an ingredient list in this image. Please send a photo of a recipe's ingredients.

# Ingredient processing messages
no-ingredients-found = No Ingredients Detected
no-ingredients-suggestion = I couldn't find any measurements or ingredients in the text. Try sending a clearer image of a recipe or ingredient list.
unknown-ingredient = Unknown ingredient
error-processing-failed = Failed to process ingredients
save-failed-retry = Saving failed and nothing was saved. Please try again in a moment.

# Processing messages
processing-reading = ⏳ Reading the recipe…

# Unsupported message types
unsupported-title = 🤔 I can only process text messages and images.
unsupported-description = What I can do:
unsupported-feature1 = 📸 Send photos of text you want to extract
unsupported-feature2 = 📄 Send image files (PNG, JPG, JPEG, BMP, TIFF, TIF)
unsupported-feature3 = 💬 Send /start to see the welcome message
unsupported-feature4 = ❓ Send /help for detailed instructions
unsupported-final = Try sending me an image with text! 📝

# Recipe name dialogue messages
recipe-name-prompt = 🏷️ What would you like to call this recipe?
recipe-name-prompt-hint = Please enter a name for your recipe (e.g., "Chocolate Chip Cookies", "Mom's Lasagna")
recipe-name-invalid = Recipe name cannot be empty. Please enter a valid name for your recipe.
recipe-name-too-long = Recipe name is too long (maximum 255 characters). Please enter a shorter name.

# Ingredient review messages
review-title = Review Your Ingredients
review-description = Tap an ingredient to change its quantity or name, or to delete it, then confirm with ✅.
review-confirm = Confirm and Save
review-no-ingredients = No ingredients remaining
review-no-ingredients-help = All ingredients have been deleted. Add some with ➕, or cancel with ❌.
review-empty-confirm = Add at least one ingredient with ➕ before confirming, or cancel with ❌.
review-add-more = Add More Ingredients
confirm = Confirm
cancel = Cancel
edit-empty = Ingredient text cannot be empty.
edit-try-again = Please try again with a valid ingredient format.
edit-too-long = Ingredient text is too long (maximum 200 characters). Please enter a shorter description.
edit-no-ingredient-name = Please specify an ingredient name (e.g., "2 cups flour" not just "2 cups").
edit-one-line = One line at a time: type a single ingredient.
edit-ingredient-name-too-long = Ingredient name is too long (maximum 100 characters). Please use a shorter name.
edit-invalid-quantity = Invalid quantity. Please use a positive number (e.g., "2.5 cups flour").
edit-invalid-quantity-answer = Quantity not recognised. Type a number, with or without a unit (e.g. 250, 250 g, 1/2), or - if there is none.
edit-invalid-input = Input not recognised.
review-quantities-to-check = ⚠️ Please check the quantity of: {$ingredients}. Tap an ingredient to fix it, then confirm again.
error-invalid-edit = Invalid ingredient index for editing.
review-not-understood = 🤔 I did not understand this message. To fix an ingredient, tap it in the list below, then “🔢 Quantity”, “🏷️ Name” or “✏️ Whole line”. ✅ saves the recipe, ❌ discards it.

# Pagination messages
previous = Previous
next = Next
page = Page
of = of

# Recipes command messages
your-recipes = Your Recipes
search-prompt = 🔍 Type a recipe name or an ingredient, for example “flour”.
search-no-results = 🔍 No recipe matches “{$query}”
search-one-result = 🔍 1 recipe for “{$query}”
search-results = 🔍 {$count} recipes for “{$query}”
search-more-results = The first {$shown} are shown: refine your search.
list-search-hint = 🔍 Type a word to search your recipes.
search-failed = 🔍 The search did not work. Try again in a moment.
free-text-tip = 📸 To add a recipe, send a photo of its ingredient list.
    🔍 To search your recipes, type a word or two.
recipes-list-button = 📚 Recipe list

# Post-confirmation workflow messages
workflow-what-next = What would you like to do next?

# Duplicate recipe handling messages
edit-recipe-name = Rename Recipe
edit-ingredients = Edit Ingredients
delete-recipe = Delete Recipe
back-to-recipes = Back to Recipes

# Recipe management messages
rename-recipe-title = Rename Recipe
rename-recipe-instructions = Enter the new name for this recipe:
current-recipe-name = Current name
recipe-created-line = ✅ Recipe “{$name}” created
recipe-renamed-line = ✏️ “{$old_name}” renamed to “{$new_name}”
recipe-edited-line = 📝 “{$name}” updated
recipe-deleted-line = 🗑️ “{$name}” deleted
delete-recipe-title = Delete Recipe
delete-recipe-confirmation = Are you sure you want to delete this recipe? This action cannot be undone.
delete-confirm-button = 🗑️ Yes, delete

# Recipe viewing messages
recipe-not-found = Recipe not found

# Error messages for recipe operations
error-deleting-recipe = Failed to delete recipe
error-deleting-recipe-help = An error occurred while deleting the recipe. Please try again later.
error-renaming-recipe = Failed to rename recipe
error-renaming-recipe-help = An error occurred while renaming the recipe. Please try again later.

# Ingredient editing messages
editing-recipe = Editing recipe
editing-instructions = Tap an ingredient to change its quantity or name, or to delete it, ➕ to add one, then save with ✅.
editing-not-understood = 🤔 I did not understand this message. To fix an ingredient, tap it in the list below, then “🔢 Quantity”, “🏷️ Name” or “✏️ Whole line”. ✅ saves the changes, ❌ cancels them.
no-ingredients-to-edit = No ingredients to edit
no-ingredients-to-edit-help = This recipe has no ingredients to edit. Try adding some ingredients first.
error-updating-ingredients = Failed to update ingredients
add-ingredient = Add Ingredient
add-ingredient-prompt = Send me the new ingredient (e.g., "2 cups flour" or "3 eggs")

# Focused editing interface messages
edit-ingredient-title = Edit Ingredient
edit-ingredient-current = Current
edit-ingredient-instruction = Enter the new ingredient text (e.g., "3 cups whole wheat flour"):
ingredient-menu-quantity = 🔢 Quantity
ingredient-menu-name = 🏷️ Name
ingredient-menu-line = ✏️ Whole line
ingredient-menu-delete = 🗑️ Delete
ingredient-menu-back = ⬅️ Back
edit-quantity-instruction = Type the new quantity (or - if there is none):
edit-name-instruction = Type the new name of the ingredient:

# Unit views (UX plan U8)
unit-view-title = ⚖️ How units are shown
unit-view-explain = Recipes do not change: only how cards show them follows this choice.
unit-view-recipe = As in the recipe
unit-view-weight = All in weight (g)
unit-view-french = French units (g, cl)
unit-view-us = US units (cups, oz)
unit-view-chosen = ⚖️ Units: { $view }
unit-view-footer-weight = ⚖️ Weight view (g), rounded values
unit-view-footer-french = ⚖️ French units view (g, cl), rounded values
unit-view-footer-us = ⚖️ US units view, rounded values

# Linked recipes (UX plan U9)
review-parts-intro = 🧩 This photo has { $count } parts: they will become { $count } linked recipes. The first one takes the recipe name, the others "Name (part)".
review-part-main = 🧩 The recipe
review-part-main-heading = 🧩 The recipe: { $heading }
review-part-linked = 🧩 Linked recipe: { $heading }
recipe-created-linked-line = ✅ Recipe “{ $name }” created, linked to “{ $main }”
card-linked-recipes = 🔗 Linked recipes: { $names }
