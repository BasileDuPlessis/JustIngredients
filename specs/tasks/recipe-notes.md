# Recipe note: a short free text per recipe (study)

> Wording (owner, 2026-11-05, after the staging test): the user sees **"Mémo"** with 📌 ("📌 Un
> mémo pour « Tarte » ?", card button "📌 Mémo", card line "📌 Four 180 °C"), not "Note" / 🗒️;
> the skip button is "Passer" without an emoji. The code, the column and the counters keep
> "note". The rest of this study is kept as written.

Status: implemented (2026-11-05, owner validated the five recommendations and the name-length fix); not in production yet. One difference from the study: at creation the bot does not show the new recipe's card before the question, the prompt names the recipe and the history already has its "créée" line. Requested by the owner: keep a short free text
with a recipe ("four 180 °C, 25 min"), asked right after the recipe's name at creation, editable
later. Concerns named: short, filtered, safe.

## 1. What exists today (where the change lands)

- Creation: ✅ on the review → `confirm_review` (`bot/callbacks/review_callbacks/end.rs`). With a
  name from the caption or title it saves at once; otherwise `ask_recipe_name` sets
  `WaitingForRecipeNameAfterConfirm`, and the typed name saves (`dialogue_manager/recipe_name.rs`).
  The saved recipe is `recipes (id, telegram_id, content, recipe_name, created_at, content_tsv)`;
  `content` is the text read from the photo, not a user field.
- Edit model to copy: ✏️ on the card → `RenamingRecipe` state → typed answer →
  `validate_recipe_name` → write scoped by owner → `cards::refresh_cards` (every recorded card of
  the recipe is edited in place) → list back. Cancel returns to the card.
- Display: `ui_builder::render_recipe_card` builds an `Html` value; `bot/html.rs` escapes `& < >`
  and is the only way to send HTML (`Html::text` for user text). Telegram limit 4096 characters:
  the ingredient list is cut at `MAX_LIST_CHARS = 3500`, linked recipes at 400.
- Rules already in force: typed validation reasons (`ValidationError` + `l10n_key`), every
  query bound by parameters, owner checks in every write, no user text in `info/warn/error`
  logs (`deployment.md`), dialogue states are JSON kept in Postgres (new variant = fine, new field
  needs `#[serde(default)]`), every locale key in `fr` and `en`.
- The note never reaches Gemini: extraction only sees the photo.

## 2. Proposal

### Data
Migration 10 `recipe_note`: `ALTER TABLE recipes ADD COLUMN IF NOT EXISTS note TEXT
CHECK (char_length(note) <= 300)`. Nullable, no default; `NULL` = no note. Down: drop the column.
The code limit is 200 characters (below), the CHECK is a second wall with room. A release from
before the column ignores it (rollback safe, and the migrator ignores unknown versions since
IMP-026). `Recipe` gets `note: Option<String>`; `validate_database_schema` checks the column.

### Flow at creation: save first, then ask (recommended)
After the recipe is saved (name typed, or name from the caption), the bot shows the new recipe's
card and a prompt "🗒️ Une info à noter ? (four, cuisson…)" with **[Passer]**. The typed text is
written to the saved recipe; the card is refreshed and the list comes back, as after a rename.
- Same state and same handler as the later edit (`EditingRecipeNote { recipe_id, creation }`):
  only the buttons differ (Passer at creation, Annuler / 🗑 Effacer when editing).
- Nothing is lost if the user walks away: the recipe is already saved, only the note is missing.
  Asking before the save (the literal order) would carry the ingredients and the extracted text
  into a new state, make "annuler" ambiguous (it cancels the creation at the name step), and
  change the tested creation paths (`handler_tests`: save, name, cancel, parts).
- Uniform: the caption-named path (no name step) gets the same question.
- Several parts (U9, "Tarte" + "Tarte (sauce)"): the question concerns the main recipe; each part
  has its own note button on its card.
- Friction: one extra tap per recipe. Counters `note_added` / `note_skipped` tell whether to keep
  asking, to ask once and then offer only the button, or to stop.

### Edit
Card button **🗒️ Note** (row of 🗑️ Supprimer, so row 1 keeps its two buttons). Prompt shows the
current note; the typed text replaces it; **🗑 Effacer** clears it (only this button clears: no
magic "-", no accidental clear); ❌ Annuler returns to the card. Pressed twice or after a delete:
`recipe-not-found` / `button-expired`, as today.

### Display
On the card, under the name, one line: `📖 **Tarte aux pommes**` / `🗒️ Four 180 °C · 25 min`, through
`Html::text`. `refresh_cards` already redraws every recorded card from the database, so history
cards follow. The list budget on the card drops by the note's length so the 4096 limit holds; a
test with the longest name, a 200-line list, 10 links and a 200-character note.

### Not in the first version
Search on notes (it would rank "180" before real matches), suggestion from the photo (Gemini could
read "180 °C, 25 min" from the page: prompt and schema change, to judge later), sharing.

## 3. Security and filtering

The note is private (private chats only, scoped by owner, no sharing, not sent to a model), so the
real risks are technical, not moderation.

| Risk | Answer |
|---|---|
| Markup injection (`<b>`, `</pre>`, `<a href>`, `&`) | Only `Html::text`; test with each. |
| SQL injection | Bound parameter; test with `'; DROP TABLE recipes; --` stored as is. |
| NUL / control characters (Postgres TEXT refuses NUL) | Stripped before validation. |
| Invisible or reversing characters (zero width space, BOM, bidi overrides U+202A–E, U+2066–9, LRM/RLM, Hangul and braille blanks, tag characters) | Stripped. The joiner U+200D stays (family and chef emoji); a note with nothing else is empty. |
| Layout abuse (blank lines, 4000 spaces) | All whitespace runs, newlines included, become one space: the note is one line. |
| Oversize (Telegram allows 4096 characters per message) | Counted in characters, not bytes; over 200 is **refused with a message**, never cut silently (`ValidationError::NoteTooLong`). |
| Empty after cleaning (only zero-width characters) | Not saved; the prompt stays with a hint. Never clears. |
| Another user's recipe id (forged callback) | `UPDATE … WHERE id = $1 AND telegram_id = $2`; 0 rows = `recipe-not-found`. |
| Link or @mention in the note | Allowed (a source link is a good use). Telegram makes it tappable; link previews are switched off on cards in `html.rs` (one place). |
| Leak in logs / metrics | Never logged; only counters and the length in characters. |
| Prompt injection | The note is never given to a model. If a later feature does, it is data, not instructions. |
| Growth / abuse | At most 200 characters per recipe; no new write amplification. |

**No word filter** (insults, profanity): the content is seen only by its author, a list needs
upkeep and fails on French ("concombre"), and it blocks nothing dangerous. This changes if
"share a recipe as text" ships: then text reaches other people and a reporting or length-limited
policy has to be decided, which the escaping above already prepares.

## 4. Tests
- `db_tests`: set, replace, clear, owner scoping, CHECK refuses 301, migration up/down, a
  database from before the column still starts.
- Unit: cleaning (NUL, bidi, zero width, ZWJ kept, whitespace, 200 vs 201 characters in accents
  and emoji), `<b>x</b>`, `&amp;`, `</pre>`.
- `handler_tests`: creation then note, creation then Passer, caption-named creation, edit,
  Effacer, Annuler, too long, empty, a photo or a command sent at the prompt (same as at the rename
  prompt), stale button.
- Card length test with the note.

## 5. Tasks (about one day)
1. Migration 10, `Recipe.note`, `set_recipe_note` (owner scoped), schema check. (S)
2. `note.rs`: cleaning + `validate_recipe_note`, `ValidationError::NoteTooLong`. (S)
3. State `EditingRecipeNote`, `action_input` route, cancel in `workflow_callbacks`. (M)
4. Card: line, button, budget, link previews off. (S)
5. Creation hook after both save paths, Passer. (S)
6. Locales fr/en (~9 keys), counters, tests, specs. (M)

## 6. Decisions for the owner
1. Save first then ask (recommended), or ask before saving?
2. 200 characters, one line?
3. Label: "Note" (🗒️) or "Infos"?
4. Under the name, or after the ingredients?
5. Ask after every recipe, or measure first (`note_added` / `note_skipped`)?

Side finding: `validate_recipe_name` counts bytes (`len() > 255`) while the message and the
column say characters; an accented name is refused earlier than announced. One-line fix, to do
with this feature or apart.
