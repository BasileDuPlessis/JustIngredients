# UX Improvements

## Goal
Make every interaction short and predictable: each action ends in a clean "home" state, recipes
are easy to find, and messages read well. Validated by the user on 2026-10-27 from a review of
`specs/` against the code.

## Scope
- In scope: message formatting, navigation (home message, short commands, Telegram menu),
  recipe creation (title, editing), the recipe list, search, outdated specs.
- Out of scope here: Tesseract removal (Phase 5 of
  [llm-extraction-migration.md](llm-extraction-migration.md)), language preference and export
  (listed at the end as later work). Code-quality items stay in
  [improvement-backlog.md](improvement-backlog.md).

## Findings (2026-10-27)
| # | Area | Finding |
|---|---|---|
| 1 | Creation | Default name "Recipe" when there is no caption; the name is asked only after Confirm. |
| 2 | Display | No `parse_mode`: `**bold**` is shown literally (66 lines of code, 6 locale lines; IMP-025). |
| 3 | Display | 21 user messages start with technical prefixes (`[FORMAT]`, `[EXTRACTION_BUSY]`, …). |
| 4 | Creation | Two buttons per ingredient (✏️ and 🗑️), labels cut at 20 characters: 30 buttons for 15 ingredients. |
| 5 | Creation | Editing an ingredient means retyping the whole line; quantity or name alone cannot be changed. |
| 6 | Creation | The Cancel button of the processing message only arrives after processing (one update per chat at a time). |
| 7 | Recipes | Recipes are grouped by name; same-name recipes (all "Recipe") open a second list by date. |
| 8 | Recipes | 5 recipes per page, alphabetical only. |
| 9 | Recipes | The "Statistics" action mixes recipe details with global user statistics. |
| 10 | Recipes | In group chats listed recipes open as "not found" (`chat.id` vs `from.id`, IMP-024). |
| 11 | Search | The search button answers "coming soon"; the database search uses the English text configuration and is never called. |
| 12 | Settings | The language follows the Telegram app; no choice. |
| 13 | Sharing | No export or sharing of a recipe. |
| 14 | Navigation | After an action the chat is left with prompts and intermediate messages; no way back to a nominal state (user request). |
| 15 | Navigation | `/recipes` is long to type; commands are not proposed after actions; no Telegram command menu (user request). |
| 16 | Specs | `telegram-bot.md` describes `/list`, `/search`, `/delete` and a webhook that do not exist; `ocr-*.md` describe the replaced OCR pipeline; `deployment.md` lists a missing `/health/ocr`; `README.md` links a missing file. |
| 17 | Messages | `use-buttons-instruction` and `error-no-mime-type` are missing in both locales: users see "Missing translation". |
| 18 | Navigation | `/start` and `/help` typed during a dialogue are taken as input (`/help` can become a recipe name). |
| 19 | Creation | Typing "confirm"/"ok" in the review saves the recipe as "Recipe" without asking a name; the button asks it. |
| 20 | Messages | Cancelling a rename answers "Recipe deletion cancelled". |
| 21 | Dead paths | ➕ Add ingredient does nothing in the first review, `cancel_empty` has no handler, `workflow_*` buttons run twice in `ReviewIngredients`, `WaitingForRecipeName` is never entered (IMP-007). |
| 22 | Recipes | The 5-minute recipe/ingredient cache is never cleared on writes: a card opened after a rename, edit or delete can be stale (IMP-010). |
| 23 | Ops | `monitoring/prometheus.yml` scrapes without the token (401 since `METRICS_AUTH_TOKEN`); staging has no token. |
| 24 | Ops | The repository secret `GOOGLE_AI_API_KEY` is unused. |

## Decisions
| Topic | Decision (2026-10-27) |
|---|---|
| Findings | Findings 1–16 accepted by the user (12 and 13 stay "later"); 17–24 found while updating the specs (Phase U0) and added. |
| Nominal state | Superseded by "Clean thread" (U5, 2026-10-28). Before: at the end of a transaction the intermediate messages were deleted, the result kept, then the home message posted. |
| Commands | `/liste`, `/cherche`, `/aide`; English aliases `/list`, `/search`, `/help`; `/start` and `/recipes` still accepted. |
| Free text | Any text sent outside a dialogue is a search in the user's recipes. |
| Photo | Kept while its ingredients are reviewed, deleted with the other messages when the review ends (2026-10-27). |
| Review list | Deleted once the review is confirmed (2026-10-27); the saved recipe stays shown as a card (✅ name, ingredients, card buttons) above the home message (2026-10-28). |
| Clean thread | 2026-10-28, replaces "Nominal state" once Phase U5 ships: the chat keeps only a history (recipe cards and one line per action: created, renamed, modified, deleted) and the recipe list as the last message; everything else is deleted when the action ends. No home message: the list takes its place (its header carries the recipe count and the photo hint), `/start` and `/aide` are shown on demand and deleted at the next action. The user's choices: a creation is one message (the card titled "✅ created"); opening a recipe adds its card to the history and moves the list below, one card per recipe; rename and edit update the cards already shown; a delete turns the card into "🗑️ deleted"; a cancelled creation leaves nothing. |
| Releases | One production deployment per feature, validated by the user in Telegram before the next one. Changed on 2026-10-27 (after v19): features are grouped, one deployment per phase (U1.3–U1.7 together after RU1). |

## Open Questions
_None at the moment._

## Telegram Constraints
- Callback data ≤ 64 bytes (see the `BUTTON_DATA_INVALID` hotfix of 2026-10-27): buttons carry ids.
- A bot can delete its own messages and, in private chats, the user's messages, for 48 hours.
- Commands: lowercase `a-z`, `0-9`, `_`, at most 32 characters; `setMyCommands` sets the menu,
  per language.
- HTML `parse_mode` requires escaping `<`, `>` and `&` in every user or model text.

## Practices
Same as the migration plan: tasks ≤ 1 hour with a validation, `cargo fmt`/`clippy -D warnings`/
tests green before each commit, one review gate per phase by a fresh reviewer, recorded below.
Handlers stay thin: rendering and decisions go into pure, unit-tested functions.

---

## Phase U0: Specs That Match the Code

### Task U0.1: Rewrite `telegram-bot.md` (<= 1 hour)
- [x] Done
- Description: actual commands, long polling, extraction by `IngredientExtractor`, real dialogue
  states and flows; mark this plan as the source of upcoming changes.
- How to validate: every command, callback prefix and state listed exists in `src/bot/`.

### Task U0.2: Retire the OCR specs and fix the index (<= 1 hour)
- [x] Done
- Description: mark `ocr-processing.md` and `ocr-text-cleaning.md` as superseded (deleted with
  migration Task 5.5), fix `deployment.md` health endpoints, rewrite `specs/README.md` index
  (feature specs, task plans, backlog).
- How to validate: no link in `specs/` points to a missing file.

### Review gate RU0
- [x] Done (2026-10-27, see the Review Log)

## Phase U1: Display and Navigation Foundations

### Task U1.1: HTML formatting with escaping (findings 2) (<= 1 hour per area)
- [x] Done
- Description: one helper to send/edit messages with `parse_mode = HTML`; every interpolated
  user or model text (recipe names, ingredients, captions, transcriptions) escaped; `**x**`
  becomes `<b>x</b>` in code and locales. Done area by area (review, recipes, help/welcome,
  errors) to keep commits small.
- How to validate: unit tests of the escaping helper (`<script>`, `&`, names with `*`); no `**`
  left in `src/bot` or `locales/`.
- Progress (feature 1, 2026-10-27): the literal `**` markers are gone from messages and locales
  (plain text everywhere, guarded by a localization test). Remaining: HTML with escaping for the
  key screens (review list, recipe card, home, help), built so text cannot break the markup;
  enabling HTML globally would break any message whose user text contains `<`, `>` or `&`.
- Done (2026-10-27, part 2): `bot::html` (`Html` built only from escaped text, `<b>`, `<pre>`;
  `send_html`/`edit_html`; `Html` does not convert to `String`). Renderers in `ui_builder`
  (review list, saved-recipe editing view, recipe card, card after editing) and for home, welcome
  and help; the no-ingredients transcription moved from literal ``` to `<pre>`. Escaping and
  renderer unit tests. Manual check pending in Telegram.

### Task U1.2: User messages without technical prefixes (finding 3) (<= 1 hour)
- [x] Done
- Description: drop `[FORMAT]`, `[EXTRACTION_BUSY]`… from the 21 locale messages; the codes stay
  in logs and metrics.
- How to validate: localization test that no message value starts with `[`; existing tests updated.

### Task U1.3: Short commands and the Telegram menu (finding 15) (<= 1 hour)
- [x] Done
- Description: `/liste`, `/cherche`, `/aide`, English aliases `/list`, `/search`, `/help`, `/start` and `/recipes`
  kept for compatibility; `setMyCommands` at startup for `fr` and `en` so the menu button lists them.
- How to validate: command parsing tests; menu visible in the Telegram app after deploy.
- Done (2026-10-27): `bot::commands` parses the commands (case-insensitive, `@botname` ignored)
  and sets the menu (`/liste` `/aide` in French, `/list` `/help` otherwise); welcome and help list
  them. `/cherche` answers "coming soon" and joins the menu in U4.2. Also fixed: three broken
  characters (U+FFFD) in the English welcome and help, now caught by the localization test.

### Task U1.4: Home message (findings 14, 15) (<= 1 hour)
- [x] Done
- Description: one pure renderer for the home message (number of recipes, last recipes as
  buttons, clickable `/commands`, "send a photo" hint) and one function that sends it, called at
  the end of every finished transaction (save, cancel, delete, rename, edit of a saved recipe).
- How to validate: renderer unit tests (0, 1, many recipes; both languages); manual check that
  each listed transaction ends with it.
- Done (2026-10-27): `bot::home` (`render_home`, `send_home`) and
  `db::get_user_recipes_newest_first` (reusable for U3.1). It replaces the post-save
  "What next?" buttons. Also fixed: an invalid new name no longer ends the rename dialogue, and
  the caption save confirmation no longer shows ✅ twice. Manual check pending in Telegram.

### Task U1.5: Transaction cleanup (finding 14, hybrid decision) (<= 1 hour)
- [x] Done
- Description: dialogue states record the message ids a transaction creates (bot prompts and
  user replies); at the end they are deleted, ignoring messages older than 48 h, then the home
  message is posted.
- Done (2026-10-27): `bot::cleanup` records message ids per chat, outside the dialogue states
  (they are rebuilt at every transition): the user's texts in a dialogue and the prompts sent
  with `send_transient`. `send_home` deletes them and the previous home message. The photo is
  kept during the review and deleted when it ends (user decision, 2026-10-27). Kept: ingredient
  list, saved or cancelled notice, recipe card. Manual check pending in Telegram.
- Note: the quantity question prompt id is still never stored in the dialogue state
  (`handle_quantity_correction_input` edits a copy returned by `dialogue.get()`); cleanup does not
  need it any more, the field can go with U2.5.
- How to validate: unit test of the id bookkeeping; manual run of a full creation: only the
  saved recipe (if kept) and the home message remain.

### Task U1.6: Commands always win (finding 18) (<= 1 hour)
- [x] Done
- Description: a message starting with `/` is a command in every dialogue state: it ends the current
  transaction (with the U1.5 cleanup) and runs; it is never taken as a name, quantity or ingredient.
- How to validate: dialogue tests: `/aide` while naming a recipe shows the help and no recipe is saved.
- Done (2026-10-27): `commands::route_text` (pure, tested) sends every `/` text to the commands,
  in every state; a running action is exited and its recorded messages deleted first. `/liste`
  no longer bypasses the dialogue. Unknown commands answer `unknown-command` with the menu.
  Known limit: the interrupted review or edit message keeps its (now inactive) buttons.

### Task U1.7: Small message fixes (findings 17, 20) (<= 1 hour)
- [x] Done
- Description: add the missing `use-buttons-instruction` and `error-no-mime-type` messages; the
  rename cancellation says "rename cancelled"; a test checks that every key used in code exists in
  both locales (IMP-045).
- How to validate: the locale-key test fails when a key is missing.
- Done (2026-10-27): `use-buttons-instruction`, `error-no-mime-type` and `rename-cancelled` added
  in both locales; the new key test failed on the first two before they were added.

### Review gate RU1
- [x] Done (2026-10-27, see the Review Log)

## Phase U2: Recipe Creation

### Task U2.1: Recipe title read by Gemini (finding 1) (<= 1 hour)
- [x] Done
- Description: nullable `title` in the extraction schema and prompt; precedence caption (without
  links) > title > ask; benchmark and live test rerun.
- How to validate: schema/prompt tests; live test returns a title for a titled page; saved name.
- Done (2026-10-27): `title` (string or null, required) in the schema and `Extraction`; the prompt
  asks for the printed title and forbids inventing one; `known_recipe_name` (caption > title >
  ask, unit-tested). The live test now checks the title (null on the only fixture, which has
  none); it could not run locally (the `.env` key is the revoked one): check on staging with a
  titled and an untitled page. The benchmark was not rerun (it only scores ingredients).

### Task U2.2: One button per ingredient (finding 4) (<= 1 hour)
- [x] Done
- Description: the review keyboard shows one button per ingredient (full label, Telegram limit
  instead of 20 characters) that opens a small menu: change quantity, change name, delete, back.
- How to validate: keyboard tests (15 ingredients → 15 + action buttons, callback data ≤ 64 bytes).
- Done (2026-10-27): one `item_<i>` button per ingredient (full line, cut beyond 60 characters)
  opens a menu in the same message (quantity, name, whole line, delete, back), for both the review
  and saved-recipe editing; `parse_ingredient_action` and keyboard tests (15 ingredients → 15 + 2
  rows, callback data ≤ 64 bytes).

### Task U2.3: Field-level editing (finding 5) (<= 1 hour)
- [x] Done
- Description: "change quantity" asks only the quantity (`parse_quantity_answer`, `-` for none);
  "change name" asks only the name; retyping the whole line still works.
- How to validate: dialogue tests for both paths and the flag cleared.
- Done (2026-10-27): `IngredientField` (`Line`, `Quantity`, `Name`) in both editing states;
  `ingredient_editing::apply_edit`, `with_quantity` (clears the ⚠️ flag) and `with_name`, unit
  tested. Also fixed: saved ingredients without quantity were turned into "1" when editing a saved
  recipe.

### Task U2.4: Honest processing message (finding 6) (<= 1 hour)
- [x] Done (2026-10-28): the processing message is "⏳ Lecture de la recette…" without a button.
  Running the extraction outside the chat's update was not cheap (dialogue updates racing a new
  photo), so the Cancel button was removed: a press was only handled after the extraction and
  then cancelled the review that had just appeared. An old `cancel_processing` button answers
  "no longer active" and loses its keyboard.
- Description: remove the Cancel button that cannot act during processing (or run extraction
  outside the per-chat update so it can, if cheap); show "⏳ Reading the recipe…".
- How to validate: no dead button; message replaced by the review or an error.

### Task U2.5: One confirm behaviour, no dead buttons (findings 19, 21) (<= 1 hour)
- [x] Done (2026-10-28): ✅ and a typed confirm word call one `confirm_review` (name asked when
  there is no caption or title; flagged quantities sent back to the ⚠️ lines), ❌ and a typed
  cancel word call one `cancel_review` (the list loses its buttons); ➕ works in the first review
  (same focused prompt as an edit); the emptied list offers ➕ and a working ❌ (first review and
  saved recipe); `cancel_empty`, the duplicate `workflow_*` dispatch, `AwaitingQuantityCorrection`
  and `WaitingForRecipeName` are gone. `noop` stays inert on purpose. Not covered by a handler
  test (no fake Telegram, IMP-031): both paths call the same function. Left: a command typed
  during a review leaves the list, whose buttons then answer "no longer active".
- Description: typed "confirm" behaves like the button (asks the name when there is no caption or
  title); ➕ Add ingredient works in the first review; remove `cancel_empty`, the duplicate
  `workflow_*` dispatch and the unused `WaitingForRecipeName` state.
- How to validate: dialogue tests for typed and button confirm; grep shows no unhandled callback.
- Found by RU0 (2026-10-27): typed "cancel"/"stop" in a review leaves the list with inactive
  buttons; `add_more` is offered after deleting the last line of a saved recipe but has no handler
  there; `noop` has no handler (harmless, the query is answered).

### Review gate RU2
- [x] Done (2026-10-28, see the Review Log)

## Phase U3: My Recipes

### Task U3.1: One entry per recipe, newest first (findings 7, 8) (<= 1 hour)
- [x] Done (2026-10-28): `bot/recipe_list.rs`, used by `/liste`, the home's 📚 button and the page
  buttons; 8 per page, `recipe_instance:<id>`. Labels: the name alone (the date was dropped at
  the user's request, 2026-10-28: hard to read), with the day, and the time if needed, only for
  names that appear twice on the page. The name-grouped list, its
  fingerprints (`recipe_selection.rs`) and the same-name screen are gone; their old buttons are
  retired (stale). Removed with them: the unused legacy `callback_handler`,
  `get_user_recipes_paginated`, `get_user_recipe_names`, `get_recipes_by_name`. Tested: the
  existing `get_user_recipes_newest_first` DB test (owner, order, paging, same names) and keyboard
  unit tests.
- Description: list recipes by id (no grouping by name), newest first, 8 per page, label
  "name · date"; buttons carry the recipe id; the same-name "instances" screen disappears.
- How to validate: DB test of the query (owner-scoped, order, paging); keyboard tests.

### Task U3.2: Recipe card (finding 9) (<= 1 hour)
- [x] Done (2026-10-28): the card keeps rename, edit ingredients, delete and back; Statistics is
  gone with `handle_recipe_statistics`, `get_user_recipe_statistics` and 11 locale keys, and the
  📊 buttons of older cards are retired (stale).
- Description: the card shows the ingredients and the actions rename, edit ingredients, delete,
  back; "Statistics" is removed (a `/stats` command can come later).
- How to validate: card renderer tests.

### Task U3.3: One owner identity (finding 10, IMP-024) (<= 1 hour)
- [x] Done (2026-10-28) by IMP-024: groups are politely refused (`private-chat-only`), so the chat
  id is always the user id; buttons use `from.id`, text paths the chat id, which are equal. No DB
  test with chat id ≠ user id: that case cannot reach the handlers any more.
- Description: use the Telegram user id for listing and saving everywhere (private chats are
  unaffected: chat id = user id); decide whether groups are supported or politely refused.
- How to validate: DB tests with chat id ≠ user id.

### Task U3.4: Fresh recipe data after every change (finding 22, IMP-010) (<= 1 hour)
- [x] Done (2026-10-28, ahead of U3: the recipe reads no longer use a cache, rather than invalidating it
  at every write; queries take a few milliseconds)
- Description: invalidate the cached recipe and ingredient entries on rename, edit, delete and save,
  so the card and the home message never show stale data.
- How to validate: cache tests: read, rename, read again returns the new name.

### Review gate RU3
- [x] Done (2026-10-28, see the Review Log)

## Phase U5: Clean Thread (before U4, user priority of 2026-10-28)
Goal: the chat reads as a history of recipes and actions, with the recipe list always last (see
"Clean thread" in the Decisions). Telegram deletes only messages under 48 hours, but a bot can
edit its own messages at any time: older messages that should go are edited down instead.

### Task U5.1: Message registry in the database (<= 1 hour)
- [x] Done (2026-10-28): migration 3, `db::{record,forget,take}_chat_message(s)`, `bot::cleanup`
  on top of them (pool given once at startup); DB tests for kinds, chats, taking once and the
  48-hour cut-off. Staging restart check pending.
- Description: migration 3, table `chat_messages (chat_id, message_id, kind, recipe_id,
  sent_at)`, kind `list`, `card` or `transient`; `bot::cleanup` records and takes messages there
  instead of in memory, so a deploy or restart no longer forgets the list or an action's
  messages. Only message ids are stored, never their content (user's agreement, 2026-10-28): one
  list, one card per shown recipe and the running action's messages per user.
- How to validate: DB tests (record, take once, per chat, kinds, 48-hour cut-off); after a
  staging restart the next action still deletes the previous list.

### Task U5.2: The recipe list as the last message (<= 1 hour)
- [x] Done (2026-10-28): `home::send_home` deletes the action's messages and the previous list
  (an old list loses its buttons instead), then sends page 1 silently with the header that
  replaced the home message (`recipe_list::render_list_header`); `/liste`, `/annuler` and the
  older 📚 buttons do the same, `/start` keeps its welcome above the list (`move_list_down`).
- Description: `send_home` becomes "move the list down": delete the previous list (or, older
  than 48 hours, edit it to one neutral line without buttons) and send page 1 without
  notification. Its header replaces the home message: the recipe count and the photo hint, an
  onboarding text when there is no recipe. `/liste` does the same; `/start` shows the welcome as a
  transient message, then the list.
- How to validate: renderer tests of the header; manual: every action ends with exactly one list
  at the bottom.

### Task U5.3: One line per action (<= 1 hour)
- [x] Done (2026-10-28): creation card titled "✅ Recette « X » créée"; rename, edit (when
  something changed) and delete lines, silent; the deleted recipe's card becomes its line; a
  cancel leaves nothing; end-of-action errors and photos that gave no review stay until the next
  action (`home::send_home_keeping`). Four replaced locale keys removed.
- Description: a creation leaves the saved card titled "✅ Recette « X » créée"; rename, edit and
  delete leave "✏️ « X » renommée en « Y »", "📝 « X » modifiée", "🗑️ « X » supprimée", sent
  without notification; a cancelled creation leaves nothing; errors (not a recipe, daily limit,
  save failed) stay until the next action, then go.
- How to validate: renderer tests of each line in fr and en; manual run of each action.

### Task U5.4: One card per recipe, always current (<= 1 hour)
- [x] Done (2026-10-28): `bot/cards.rs` (`show_card`, `refresh_cards`), cards recorded with
  their recipe (`chat_messages`, `find_recipe_messages`). Opening replaces the earlier card and
  moves the list below; rename and edit update the cards in place; the editing view of a saved
  recipe is now transient (before, it became a second card and the first one kept the old
  ingredients); a delete turns its card into the line and removes the others; ⬅️ forgets the
  card. DB test of the card records; the rest is checked manually on staging.
- Description: cards are recorded with their recipe id. Opening a recipe deletes its previous
  card (or strips its buttons after 48 hours) and sends the card above the list; a rename or an
  edit updates every recorded card of the recipe in place; a delete turns the card into the
  "🗑️ supprimée" line.
- How to validate: DB-backed tests of the card bookkeeping; manual: open, rename, open again,
  delete.

### Task U5.5: Everything else is transient (<= 1 hour)
- [x] Done (2026-10-28): typed commands, `/aide`, `/cherche`, unknown commands, free text and
  unsupported messages with their answers, and an ignored delete confirmation are transient; the
  answers appear above the list (`move_list_down`). Telegram toasts leave nothing anyway.
  `telegram-bot.md` rewritten along U5.2–U5.5.
- Description: `/aide`, `/start`, unknown commands, free text and the bot's answers, delete
  confirmations and "button no longer active" toasts leave nothing once the next action ends;
  the "Nominal state" decision and `telegram-bot.md` are rewritten for the new model.
- How to validate: manual review of a long session on staging: only cards, action lines and one
  list remain.

### Review gate RU5
- [ ] Done

## Phase U4: Search

### Task U4.1: French-aware search (finding 11) (<= 1 hour)
- [ ] Done
- Description: search on recipe names and ingredient names with a `simple` text configuration
  plus `unaccent` (French and English recipes), owner-scoped, ranked; index migration.
- How to validate: DB tests: "creme" finds "crème brûlée", "farine" finds recipes by ingredient,
  another user's recipes never appear.

### Task U4.2: Search command, button and free text (finding 11) (<= 1 hour)
- [ ] Done
- Description: `/cherche <mots>` (and `/search`), the search button asks for words, and any text sent
  outside a dialogue is searched; results use the recipe list component of U3.1.
- How to validate: handler-level tests of parsing; manual search in Telegram.

### Review gate RU4
- [ ] Done

## Later (not planned yet)
- Ops: give `monitoring/prometheus.yml` the metrics token and set `METRICS_AUTH_TOKEN` on staging
  (finding 23). Finding 24 done: the unused `GOOGLE_AI_API_KEY` repository secret was deleted by
  the user on 2026-10-27, with the repository-level `FLY_API_TOKEN` (replaced by one deploy token
  per environment).
- Language preference (`/langue`) instead of the Telegram app language (finding 12).
- Export/share a recipe as text (finding 13); several photos at once.

---

## Review Log
| Gate | Date | Commits | Findings | Outcome |
|---|---|---|---|---|
| RU0 | 2026-10-27 | specs at `fd59224` (done late, after U1 and migration Phase 5) | All commands, states, callback prefixes and message keys of the specs exist in `src/bot/`; all 33 relative links resolve. 18 wrong or missing claims: deployment (observability variables never read, deleted repository token, rate limit per TCP peer, CI trigger, `--ha=false`), database design (newest-first query, unused functions, compile-time checks, startup schema check, delete not transactional, two metrics, `update_recipe_ingredients` owner id), bot (typed cancel, `add_more` without handler, query not answered on handler error, `html.rs`), extraction (100-byte names, empty answers not cached), README (variables). | Pass with fixes: all fixed in the specs; code issues to IMP-051 and IMP-052, dead buttons to U2.5. |
| RU1 | 2026-10-27 | `80a6941`, `503f4c9`, `04b6430`, `e1515ba`, `f3ac39a` (U1.3–U1.7) | 1 bug: a delete confirmed or cancelled during another action (rename, quantity question) sent the home message and deleted that action's prompts while it kept running. 4 nits: a command during an action also deleted the previous home message; saved-recipe editing ended without a home message when the recipe was gone; cancelling a delete also removed the card; a new `expect` outside tests. | Pass with fixes, all fixed in the RU1 fix commit. U1.1 part 2 (HTML) is not covered: still open. |
| RU2 | 2026-10-28 | `497f154..877bb8a`: all of U2 and the backlog fixes going into the release (two independent reviewers: bot flows; database, startup, HTTP, extraction guard) | Flows, 4 bugs: an emptied review could be saved with no ingredient (➕ hidden on an empty list); a quantity typed with its unit ("250 g") was refused with a line-format message; list buttons said "expired" while an ingredient was being added to a saved recipe; buttons of an older review acted on the current one (IMP-007 left). Risks and nits: `/annuler` left the review list and the name prompt; the cancel notice vanished after a failed save; groups got a reply to every message when the bot saw them all; typed cancel did not cancel a saved-recipe edit; outdated `review-no-ingredients-help`; missing `edit-invalid-input`. Server side, 1 bug: ingredients saved in one transaction share `created_at` and had no tiebreaker, so edited lines could move. 3 risks: readiness answered OK before the heartbeat existed and stayed 503 up to a minute after the startup `getMe` succeeded; `/health/live` counted against the rate limit. Nits: ingredient names in DB error contexts; quota used by calls refused before the service. Checked clean: HTML escaping, locale keys, callback data length, 4096 limit, panics, migration 2 on a v1 copy and its rollback, transactions and owner checks, `/metrics` auth, `.instrument` conversions, Fluent output. | Pass with fixes, all fixed in the RU2 fix commits. Left: Prometheus scraping gets 401 until it sends the token (finding 23); the migration splitter ignores `$$` and block comments (IMP-028). |
| RU3 | 2026-10-28 | `267a13c` (U3.1–U3.3) | No bug. 1 risk: two recipes with the same name saved the same day had identical buttons (the day-only date). Nits: a page over 1,250 failed the query's offset limit before the past-the-end clamp; 5 dead locale keys; no test of the card keyboard; a test bound compared characters with bytes. Checked clean: owner checks, paging (page count, last page, empty list, stable order), entry points, callback data length, multi-byte truncation, retired-button prefixes, removed keys and functions. | Pass with fixes: same-day duplicates show the time, the page is clamped, dead keys removed, card keyboard test added. |
