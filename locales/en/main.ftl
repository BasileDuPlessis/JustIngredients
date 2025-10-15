# Ingredients Bot - English Localization
# Main welcome and help messages

welcome-title = Welcome to Ingredients Bot!
welcome-description = I'm your OCR assistant that can extract text from images. Here's what I can do:
welcome-features =
    📸 **Send me photos** of ingredient lists, recipes, or any text you want to extract
    � **Add captions** to automatically name your recipes
    �📄 **Send me image files** (PNG, JPG, JPEG, BMP, TIFF, TIF)
    🔍 **I'll process them with OCR** and send back the extracted text
    💾 **All extracted text is stored** for future reference
welcome-commands = Commands:
welcome-start = /start - Show this welcome message
welcome-help = /help - Get help and usage instructions
welcome-send-image = Just send me an image and I'll do the rest! 🚀

help-title = 🆘 Ingredients Bot Help
help-description = How to use me:
help-step1 = 1. 📸 Send a photo of text you want to extract
help-step2 = 2. � Add a caption to name your recipe (optional)
help-step3 = 3. �📎 Or send an image file (PNG, JPG, JPEG, BMP, TIFF, TIF)
help-step4 = 4. ⏳ I'll process it with OCR technology
help-step5 = 5. 📝 You'll receive the extracted text and can review/edit ingredients
help-formats = Supported formats: PNG, JPG, JPEG, BMP, TIFF, TIF
help-limits = File size limit: 10MB for JPEG, 5MB for other formats
help-commands = Commands:
help-start = /start - Welcome message
help-help = /help - This help message
help-tips = Tips:
help-tip1 = • Use clear, well-lit images
help-tip2 = • Ensure text is readable and not too small
help-tip3 = • Avoid blurry or distorted images
help-tip4 = • Add a caption to automatically name your recipe
help-tip5 = • Supported languages: English + French
help-final = Need help? Just send me an image! 😊

# Error messages
error-download-failed = [DOWNLOAD] Failed to download the image. Please try again.
error-unsupported-format = [FORMAT] Unsupported image format. Please use PNG, JPG, JPEG, BMP, TIFF, or TIF formats.
error-no-text-found = [OCR_RESULT] No text was found in the image. Please try a clearer image with visible text.
error-ocr-initialization = [OCR_INIT] OCR engine initialization failed. Please try again later.
error-ocr-extraction = [OCR_EXTRACT] Failed to extract text from the image. Please try again with a different image.
error-ocr-timeout = [OCR_TIMEOUT] OCR processing timed out: {$msg}
error-ocr-corruption = [OCR_CORRUPT] OCR engine encountered an internal error. Please try again.
error-ocr-exhaustion = [OCR_RESOURCE] System resources are exhausted. Please try again later.
error-validation = [VALIDATION] Image validation failed: {$msg}
error-image-load = [IMAGE_LOAD] The image format is not supported or the image is corrupted. Please try with a PNG, JPG, or BMP image.

# Success messages
success-extraction = ✅ **Text extracted successfully!**
success-extracted-text = 📝 **Extracted Text:**
success-photo-downloaded = Photo downloaded successfully! Processing...
success-document-downloaded = Image document downloaded successfully! Processing...

# Ingredient processing messages
ingredients-found = Ingredients Found!
no-ingredients-found = No Ingredients Detected
no-ingredients-suggestion = I couldn't find any measurements or ingredients in the text. Try sending a clearer image of a recipe or ingredient list.
line = Line
unknown-ingredient = Unknown ingredient
total-ingredients = Total ingredients found
original-text = Original extracted text
error-processing-failed = [INGREDIENT_PROCESSING] Failed to process ingredients
error-try-again = Please try again with a different image.

# Processing messages
processing-photo = Photo downloaded successfully! Processing...
processing-document = Image document downloaded successfully! Processing...

# Unsupported message types
unsupported-title = 🤔 I can only process text messages and images.
unsupported-description = What I can do:
unsupported-feature1 = 📸 Send photos of text you want to extract
unsupported-feature2 = 📄 Send image files (PNG, JPG, JPEG, BMP, TIFF, TIF)
unsupported-feature3 = 💬 Send /start to see the welcome message
unsupported-feature4 = ❓ Send /help for detailed instructions
unsupported-final = Try sending me an image with text! 📝

# Regular text responses
text-response = Received: {$text}
text-tip = 💡 Tip: Send me an image with text to extract it using OCR!

# Recipe name dialogue messages
recipe-name-prompt = 🏷️ What would you like to call this recipe?
recipe-name-prompt-hint = Please enter a name for your recipe (e.g., "Chocolate Chip Cookies", "Mom's Lasagna")
recipe-name-invalid = [RECIPE_NAME] Recipe name cannot be empty. Please enter a valid name for your recipe.
recipe-name-too-long = [RECIPE_NAME] Recipe name is too long (maximum 255 characters). Please enter a shorter name.
recipe-complete = ✅ Recipe "{$recipe_name}" saved successfully with {$ingredient_count} ingredients!

# Caption-related messages
caption-used = 📝 Using your caption "{$caption}" as the recipe name
caption-invalid = [CAPTION] Caption "{$caption}" is invalid, using default recipe name instead
caption-empty = 💡 Tip: Add a caption to your photo to automatically name your recipe!

# Ingredient review messages
review-title = Review Your Ingredients
review-description = Please review the extracted ingredients below. Use the buttons to edit or delete items, then confirm when ready.
review-confirm = Confirm and Save
review-cancelled = [REVIEW_CANCEL] Ingredient review cancelled. No ingredients were saved.
review-no-ingredients = No ingredients remaining
review-no-ingredients-help = All ingredients have been deleted. You can add more ingredients by sending another image, or cancel this recipe.
review-add-more = Add More Ingredients
review-add-more-instructions = Send another image with ingredients to add them to this recipe.
cancel = Cancel
edit-ingredient-prompt = Enter the corrected ingredient text
current-ingredient = Current ingredient
edit-empty = Ingredient text cannot be empty.
edit-invalid-format = Invalid ingredient format. Please enter something like "2 cups flour" or "3 eggs".
edit-try-again = Please try again with a valid ingredient format.
edit-too-long = Ingredient text is too long (maximum 200 characters). Please enter a shorter description.
edit-no-ingredient-name = Please specify an ingredient name (e.g., "2 cups flour" not just "2 cups").
edit-ingredient-name-too-long = Ingredient name is too long (maximum 100 characters). Please use a shorter name.
edit-invalid-quantity = Invalid quantity. Please use a positive number (e.g., "2.5 cups flour").
error-invalid-edit = [INGREDIENT_EDIT] Invalid ingredient index for editing.
review-help = Please reply with "confirm" to save these ingredients, or "cancel" to discard them.

# Document messages
document-image = Received image document from user {$user_id}
document-non-image = Received non-image document from user {$user_id}
document-no-mime = Received document without MIME type from user {$user_id}

# Photo messages
photo-received = Received photo from user {$user_id}

# Text messages
text-received = Received text message from user {$user_id}: {$text}

# Unsupported messages
unsupported-received = Received unsupported message type from user {$user_id}

# Pagination messages
previous = Previous
next = Next
page = Page
of = of

# Recipes command messages
no-recipes-found = No recipes found
no-recipes-suggestion = Send me some ingredient images to create your first recipe!
your-recipes = Your Recipes
select-recipe = Select a recipe to view its ingredients:
recipe-details-coming-soon = Recipe details coming soon!

# Post-confirmation workflow messages
workflow-recipe-saved = ✅ Recipe saved successfully!
workflow-what-next = What would you like to do next?
workflow-add-another = Add Another Recipe
workflow-list-recipes = List My Recipes
workflow-search-recipes = Search Recipes
caption-recipe-saved = Recipe saved as: "{$recipe_name}"
