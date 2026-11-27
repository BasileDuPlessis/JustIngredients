# Scaling a recipe's quantities: study

Status: **tasks 1 to 3 done (2026-11-25)**, reviewed (§8), on staging; the owner's trial changed the menu and the dishes' forms (§9). Validated by the owner (2026-11-24, §6: a view, the typed answer, linked recipes follow, servings later; **counts rounded to whole units**). The owner's request: "modifier les quantités
d'une recette proportionnellement": a recipe with 3 eggs made with 4; a recipe for a 20×30 cm dish
made in a 30×40 cm one; a recipe doubled, halved, two thirds.

## 1. What the code does today (checked 2026-11-24)

- Quantities are stored as `DECIMAL(10,3)` (`ingredients.quantity`, none for "sel"), read as
  `f64`; a unit is a canonical string (`g`, `cl`, `tbsp`, `tsp`, `cup`…) or a count unit
  (`gousses`, `tranches`, `pièces`…) or none (eggs: "3 oeufs").
- **One function shows every measure**: `unit_view::show_measure(quantity, unit, name, view,
  french, english_recipe)`, used by the card (`engine::recipes::build_view`), the review lists
  and the MCP door. It already rounds *computed* values (the unit views' conversions): grams to the
  gram under 100 g then to 5 g, centilitres to the half or whole, cups to ¼ ⅓ ½ ⅔ ¾; spoons and
  cups show fractions only when the value falls within 0.01 of an eighth, a quarter or a third
  ("1 ½ cuil. à café", but "1,4 cuil. à café"); saved metric values are shown as saved; **counts
  and unitless lines are shown as saved decimals** (4/3 of an egg would read "1,333").
- The card's buttons: rename, edit ingredients, memo, delete, up to 10 linked recipes, the list.
  The unit view (`/unites`) is the user's, saved; no per-card switch. Callback data is at most 64
  bytes (`recipe_action:<name>:<id>`).
- A recipe in several parts (a dish and its sauce) is several recipes, linked (`recipe_links`).
- The number of servings ("Pour 6 personnes") is not stored: the photo's reading drops it.
- MCP: `get_recipe(id, units?, lang?)` returns each ingredient as stored and as shown.

## 2. Design

### A view, not a change

Scaling shows the card with new quantities; the recipe stays as saved. The scale belongs to that
card ("⚖️ × 4/3 : pour 4 œufs au lieu de 3"), with a button back to the quantities as saved.
Opening the recipe again from the list shows it as saved. (Saving a scaled copy as a new recipe
can come later: §6.)

### How the user says it: two buttons and one typed answer

On the card, "⚖️ Ajuster les quantités" opens: **× 2**, **÷ 2**, and **"Autre…"**, which asks
for one typed answer read by a small parser:

| Typed | Means | Factor |
|---|---|---|
| `x2`, `×2`, `*2`, `2x`, `double` | a multiple | 2 |
| `/2`, `÷2`, `moitié` | a division | ½ |
| `2/3`, `1,5`, `150 %` | a fraction, a decimal, a percentage | ⅔, 1.5, 1.5 |
| `4 oeufs`, `400 g de farine` | the new quantity of one ingredient of the recipe (its name matched without accents or plural; the same unit) | 4/3, 400/250 |
| `20x30 → 30x40`, `de 20x30 à 30x40` | rectangular dishes, cm | area ratio: 1200/600 = 2 |
| `Ø24 → Ø28`, `rond 24 → 28` | round dishes, diameters | (28/24)² ≈ 1.36 |
| `Ø24 → 20x30` | round to rectangular (or back) | area ratio |
| `4 → 6 personnes`, `de 4 à 6 personnes` | servings, both given | 6/4 |

Dishes keep the same depth (the same thickness of batter): areas, not volumes. A factor between
1/10 and 10 is accepted; anything else is told why, with examples. The two buttons and the typed
answer go through the same parser.

### Rounding like a cook (the scaled values are computed)

A scaled measure is rounded as the unit views round a conversion, and more for counts:

- grams: to the gram under 100 g, to 5 g above (250 g × 4/3 → 335 g); kilograms to 0.05 kg;
- centilitres and millilitres: the unit views' metric rounding;
- spoons and cups: to the nearest quarter or third ("1 ½ cuil. à café", "⅔ tasse"), never zero;
- **eggs and other counts: to the nearest whole, at least 1** ("4 œufs", "2 gousses"; the
  owner's choice, rather than halves);
- a line without a quantity ("sel, poivre") stays as it is.

The card says the values are rounded ("valeurs arrondies", as for a conversion). The unit view
still applies (a scaled recipe in the weight view shows grams). The memo is not scaled: the card
reminds that **times and temperatures do not change with the quantities**, and a bigger dish may
need longer.

### Linked recipes follow

A dish in parts is scaled whole: from a scaled card, a linked recipe's 🔗 button opens it at the
same scale (the factor travels in the button: `recipe_link:<id>:<num>/<den>` fits in 64 bytes).

### Assistants

`get_recipe` gets an optional `scale` (a number, or the same typed forms) and answers the
quantities scaled and rounded by the same rules, with the factor and why; the recipe is not
changed.

## 3. Engine and code

- `engine::scaling`: the parser (`Scale { factor, reason }`, the reason: a factor, an ingredient
  from→to, dishes from→to, servings from→to), the rounding of scaled values; pure functions, unit
  tested with many typed forms (French and English).
- `unit_view::show_measure` takes the factor: a scaled value is a computed one (rounded); counts
  get their half rounding.
- `engine::recipes::recipe(.., scale)`; the card's text and keyboard (`cards`, `ui_builder`): the
  ⚖️ button, the scale's line, "↩️ Quantités d'origine"; a dialogue state for the typed answer (as
  the rename and memo prompts); the card's record keeps its scale, so a refresh (an assistant's
  memo, a rename) keeps it.
- No migration, unless the card's record needs a column for the scale.

## 4. Not in scope (now)

Scaling times or temperatures; converting units as quantities grow (12 cuil. à café stay 12, the
French view keeps spoons); servings read from photos (§6); saving the scaled recipe (§6).

## 5. Tasks (about 3 days)

1. `engine::scaling`: parser and rounding, unit tests. (M)
2. `show_measure` and `engine::recipes` with a scale; golden cards with a scale. (S)
3. The card: ⚖️ button, × 2 / ÷ 2 / Autre…, the typed answer, the scale line, back to saved,
   linked recipes at the same scale, refreshes keeping it; handler tests. (M)
4. MCP `get_recipe(scale)`; docs; staging test; production. (S)

## 6. Decisions for the owner

1. A view on the card, the recipe unchanged (recommended); saving a scaled copy later if wanted.
2. Two buttons (× 2, ÷ 2) and one typed answer for everything else: a factor, an ingredient's new
   quantity, dishes' sizes, servings (recommended).
3. A cook's rounding (grams to 5 g, spoons to ¼ or ⅓), marked as rounded; **eggs and other
   counts to whole units, at least 1** (the owner's choice).
4. Linked recipes follow the same scale (recommended).
5. Assistants can ask a recipe scaled (`get_recipe(scale)`) (recommended).
6. Later, separately: read "Pour 6 personnes" on photos and store it, to offer "pour 4 personnes"
   in one tap.

## 7. As built (2026-11-25)

- `engine::scaling` (task 1): `parse(text, lines) -> Scale { ratio, reason }`; the factor is kept
  as a fraction (`Ratio`, a denominator up to 12 when it is within 0.5 %, else thousandths),
  written "4/3" in buttons; `scaled(quantity, unit, factor)` rounds as a cook would. The label
  shows fractions up to quarters ("× 4/3", "÷ 2"), else a decimal ("× 1,36").
- `engine::recipes::scaled_recipe(.., scale)` (task 2): a measure the unit view converts is
  rounded by the view from its exact scaled value; one shown in its saved unit is rounded by
  `scaled`. `get_recipe(scale)` for assistants (specs/mcp.md §3).
- The card (task 3): ⚖️ opens × 2, ÷ 2, "Autre…" and ◀️; a scaled card shows "⚖️ Quantités × 2,
  arrondies." under the memo and "⏱️ Temps et températures inchangés…" under the list, with "↩️
  Quantités d'origine". Only the card at the bottom acts. **A migration was needed**:
  `chat_messages.scale` (17) records each card's scale, so refreshes (a rename, an edit, an
  assistant's memo) show each card at its own scale, and a card sent into the history while the
  "Autre…" question is open gets its buttons back at its scale. Handler tests cover the menu,
  × 2 and back, a refused then accepted typed scale, a rename keeping the scale, ❌, and a linked
  recipe opened at the same scale.
- Staging test of the assistants' side: `~/.claude/ji-scripts/scale_e2e.sh` (a test recipe
  created with the staging key, scaled seven ways, three refusals, unchanged after).

## 8. What the code review changed (2026-11-25)

An independent review of tasks 1 to 3 checked the parser's ranges, `Ratio` and the buttons' 64
bytes, the recorded scale on every path, buttons of other cards and users, the dialogue's
endings, and the MCP refusals. Fixed, with tests:

- **Metric values converted by the view were not rounded**: the French view writes millilitres
  and litres in cl, the weight view kilograms in g, exactly. A scaled measure is now shown
  through `unit_view::show_computed_measure`, which rounds those too (25 cl × 4/3: "33 cl", not
  "33,333 cl"). Kilograms and litres under 1 round to the hundredth ("0,17 l", not "0,15 l").
- **`/unites` brought the card back as saved**: it comes back at its scale, in the new view.
- **The scale is read from the card pressed** (◀️ of the menu, ❌ of an action on a named card),
  not from the recipe's newest card.
- **The same scale tapped twice** said "recipe not found" (Telegram's "message is not modified"):
  only a recipe gone says so now.
- **An ingredient twice** ("beurre" for the dough and the mould): the same measure twice gives
  one scale; different ones are told to give a factor instead (`scale-repeated`), rather than
  "write its exact name", which nothing could satisfy.
- Small: a typed range ("2-3 oeufs") is not read as its lower bound; dishes' areas no longer
  overflow; an assistant's number in exponent form (1e-7) is read as a number; the card's
  "(lait : 25 cl → 50 cl)" keeps the saved unit on both sides.

Left as is: a card refreshed after a rename or a memo keeps its factor but not its reason ("× 4/3"
instead of "× 4/3 (œufs : 3 → 4)"): only the factor is recorded.

## 9. The owner's trial on staging (2026-11-27)

"C'est bien", with two changes, made:

- **The menu stays open while adjusting**: a tap on ÷ 2 or × 2 updates the card's quantities
  above and keeps the menu, so the result shows at once. The steps now apply to the scale shown
  (× 2 twice is × 4, ÷ 2 goes back), within a tenth and ten times (a step beyond is not offered);
  each button carries the scale it leads to (`pick:<id>:<num>/<den>`), so a double tap changes
  nothing. The menu also has "↩️ Quantités d'origine"; ◀️ gives the card its buttons back. After
  a typed answer ("Autre…") the card comes back with the menu open.
- **A typed factor applies to the quantities shown** (the owner: ÷ 2, then "x2" typed, went back
  to the recipe's quantities ×2 instead of the ones shown): "x2", "/2", "2/3", "150 %" multiply
  the scale shown, as the buttons do; a target ("4 oeufs", dishes, servings) describes the
  recipe as saved and starts from it. The question says so when the card is scaled
  ("Quantités affichées : ÷ 2. Un facteur s'y applique ; une quantité d'ingrédient, un plat ou
  des portions partent de la recette."). The dialogue keeps the scale shown (`shown`).
- **No Ø to type**: round dishes are read as "d24 à d28" (the owner's form), "rond 24 à 28",
  "diamètre 24 en 28", "un moule de 24 cm de diamètre à 28"; "plat", "moule", "un", "de" are
  skipped. The examples shown say "20x30 à 30x40, rond : d24 à d28" (no → either: also hard to
  type); the card still writes "plat Ø24 → Ø28".
