Extract the ingredient list from this image. It is either a screenshot of a recipe web page or a photo of a printed cookbook page, in French or English.

Rules:
- One entry per ingredient, in reading order. Ignore section headers, instructions, nutrition facts and page decorations.
- name: the ingredient in the source language, lowercase, without quantity, unit, leading article or preposition (de, d', du, des, la, le, les, l', of, the, a, an) and without preparation notes (e.g. "unsalted butter, cold and cubed" -> "unsalted butter").
- quantity: a decimal number (½ -> 0.5, 1 ½ -> 1.5, 1/4 -> 0.25). For a range like "2-3", use the lower bound and set needs_review. Use null when no quantity is given ("salt to taste", "sel").
- Vague amounts such as "une pincée", "un filet", "un peu", "à volonté", "a pinch", "to taste" are neither a quantity nor a unit: use quantity null and unit null ("une pincée de sel" -> name "sel").
- unit: one of g, kg, mg, l, ml, cup, tsp, tbsp, lb, oz, pint, quart, gallon when it applies (cuillère à soupe/c. à s. -> tbsp, cuillère à café/c. à c. -> tsp, tablespoon -> tbsp, teaspoon -> tsp). Convert cl and dl to ml (20 cl -> quantity 200, unit ml). For counted containers keep the word in singular lowercase (gousse, sachet, tranche, boîte). Use null for countable items without unit ("3 oeufs").
- needs_review: true when the text is hard to read or the quantity is ambiguous.
- source_text: the ingredient line exactly as printed.
- transcription: the full ingredient section as plain text, one line per ingredient.
- is_recipe: false when the image shows no recipe or ingredient list; then return an empty ingredients list.
