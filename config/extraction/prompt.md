Extract the ingredient list from this image. It is either a screenshot of a recipe web page or a photo of a printed cookbook page, in French or English.

Rules:
- One entry per ingredient, in reading order. Ignore instructions, nutrition facts and page decorations. Headings are not entries: they go in the section of the ingredients printed under them.
- name: the ingredient in the source language, lowercase, without quantity, unit, leading article or preposition (de, d', du, des, la, le, les, l', of, the, a, an) and without preparation notes (e.g. "unsalted butter, cold and cubed" -> "unsalted butter").
- quantity: a decimal number (½ -> 0.5, 1 ½ -> 1.5, 1/4 -> 0.25; French fraction words are quantities, not units: "un quart de beurre" -> 0.25, "un demi-litre" -> 0.5 l). For a range like "2-3", use the lower bound and set needs_review. Use null when no quantity is given ("salt to taste", "sel").
- Vague amounts such as "une pincée", "un filet", "un peu", "à volonté", "a pinch", "to taste" are neither a quantity nor a unit: use quantity null and unit null ("une pincée de sel" -> name "sel").
- unit: one of g, kg, mg, l, ml, cup, tsp, tbsp, lb, oz, pint, quart, gallon when it applies. A spoon "à café" (or "à thé") is always tsp, a spoon "à soupe" is always tbsp, however abbreviated: cuillère à café, cuil. à café, c. à café, c. à c., càc, cc, cuillère à thé, teaspoon -> tsp; cuillère à soupe, cuil. à soupe, c. à soupe, c. à s., càs, cs, tablespoon -> tbsp. Convert cl and dl to ml (20 cl -> quantity 200, unit ml). Use null for countable items without unit ("3 oeufs").
- Counted pieces and containers are part of the name, not a unit: "2 gousses d'ail" -> quantity 2, unit null, name "gousse d'ail"; "1 sachet de levure" -> quantity 1, unit null, name "sachet de levure" (singular).
- needs_review: true when the text is hard to read or the quantity is ambiguous.
- source_text: the ingredient line exactly as printed.
- section: the heading of the part of the recipe the ingredient is printed under, exactly as printed without a trailing colon ("Pour la sauce", "For the frosting"); null when the ingredient is under no such heading. Generic headings are not sections: "Ingrédients", "Ingredients", "Pour 6 personnes", "Serves 4", "Pour servir", "Pour décorer", "To serve", "To garnish" -> null.
- transcription: the full ingredient list as plain text, one line per ingredient.
- title: the recipe title exactly as printed (the dish name, e.g. "Tarte aux pommes"), without the site name, author, rating or category. Use null when no title is visible: never invent or summarise one from the ingredients.
- is_recipe: false when the image shows no recipe or ingredient list; then return an empty ingredients list.
