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

## Decisions
| Topic | Decision (2026-10-27) |
|---|---|
| Findings | All 16 findings are accepted for improvement (12 and 13 stay "later"). |
| Nominal state | Hybrid: at the end of a transaction the intermediate messages (prompts, user replies, previews) are deleted, the result is kept (e.g. the saved recipe card), then the home message is posted. |
| Commands | `/liste`, `/cherche`, `/aide`; English aliases `/list`, `/search`, `/help`; `/start` and `/recipes` still accepted. |
| Free text | Any text sent outside a dialogue is a search in the user's recipes. |
| Releases | One production deployment per feature, validated by the user in Telegram before the next one. |

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
- [ ] Done
- Description: actual commands, long polling, extraction by `IngredientExtractor`, real dialogue
  states and flows; mark this plan as the source of upcoming changes.
- How to validate: every command, callback prefix and state listed exists in `src/bot/`.

### Task U0.2: Retire the OCR specs and fix the index (<= 1 hour)
- [ ] Done
- Description: mark `ocr-processing.md` and `ocr-text-cleaning.md` as superseded (deleted with
  migration Task 5.5), fix `deployment.md` health endpoints, rewrite `specs/README.md` index
  (feature specs, task plans, backlog).
- How to validate: no link in `specs/` points to a missing file.

### Review gate RU0
- [ ] Done

## Phase U1: Display and Navigation Foundations

### Task U1.1: HTML formatting with escaping (findings 2) (<= 1 hour per area)
- [ ] Done
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

### Task U1.2: User messages without technical prefixes (finding 3) (<= 1 hour)
- [x] Done
- Description: drop `[FORMAT]`, `[EXTRACTION_BUSY]`… from the 21 locale messages; the codes stay
  in logs and metrics.
- How to validate: localization test that no message value starts with `[`; existing tests updated.

### Task U1.3: Short commands and the Telegram menu (finding 15) (<= 1 hour)
- [ ] Done
- Description: `/liste`, `/cherche`, `/aide`, English aliases `/list`, `/search`, `/help`, `/start` and `/recipes`
  kept for compatibility; `setMyCommands` at startup for `fr` and `en` so the menu button lists them.
- How to validate: command parsing tests; menu visible in the Telegram app after deploy.

### Task U1.4: Home message (findings 14, 15) (<= 1 hour)
- [ ] Done
- Description: one pure renderer for the home message (number of recipes, last recipes as
  buttons, clickable `/commands`, "send a photo" hint) and one function that sends it, called at
  the end of every finished transaction (save, cancel, delete, rename, edit of a saved recipe).
- How to validate: renderer unit tests (0, 1, many recipes; both languages); manual check that
  each listed transaction ends with it.

### Task U1.5: Transaction cleanup (finding 14, hybrid decision) (<= 1 hour)
- [ ] Done
- Description: dialogue states record the message ids a transaction creates (bot prompts and
  user replies); at the end they are deleted, ignoring messages older than 48 h, then the home
  message is posted.
- How to validate: unit test of the id bookkeeping; manual run of a full creation: only the
  saved recipe (if kept) and the home message remain.

### Review gate RU1
- [ ] Done

## Phase U2: Recipe Creation

### Task U2.1: Recipe title read by Gemini (finding 1) (<= 1 hour)
- [ ] Done
- Description: nullable `title` in the extraction schema and prompt; precedence caption (without
  links) > title > ask; benchmark and live test rerun.
- How to validate: schema/prompt tests; live test returns a title for a titled page; saved name.

### Task U2.2: One button per ingredient (finding 4) (<= 1 hour)
- [ ] Done
- Description: the review keyboard shows one button per ingredient (full label, Telegram limit
  instead of 20 characters) that opens a small menu: change quantity, change name, delete, back.
- How to validate: keyboard tests (15 ingredients → 15 + action buttons, callback data ≤ 64 bytes).

### Task U2.3: Field-level editing (finding 5) (<= 1 hour)
- [ ] Done
- Description: "change quantity" asks only the quantity (`parse_quantity_answer`, `-` for none);
  "change name" asks only the name; retyping the whole line still works.
- How to validate: dialogue tests for both paths and the flag cleared.

### Task U2.4: Honest processing message (finding 6) (<= 1 hour)
- [ ] Done
- Description: remove the Cancel button that cannot act during processing (or run extraction
  outside the per-chat update so it can, if cheap); show "⏳ Reading the recipe…".
- How to validate: no dead button; message replaced by the review or an error.

### Review gate RU2
- [ ] Done

## Phase U3: My Recipes

### Task U3.1: One entry per recipe, newest first (findings 7, 8) (<= 1 hour)
- [ ] Done
- Description: list recipes by id (no grouping by name), newest first, 8 per page, label
  "name · date"; buttons carry the recipe id; the same-name "instances" screen disappears.
- How to validate: DB test of the query (owner-scoped, order, paging); keyboard tests.

### Task U3.2: Recipe card (finding 9) (<= 1 hour)
- [ ] Done
- Description: the card shows the ingredients and the actions rename, edit ingredients, delete,
  back; "Statistics" is removed (a `/stats` command can come later).
- How to validate: card renderer tests.

### Task U3.3: One owner identity (finding 10, IMP-024) (<= 1 hour)
- [ ] Done
- Description: use the Telegram user id for listing and saving everywhere (private chats are
  unaffected: chat id = user id); decide whether groups are supported or politely refused.
- How to validate: DB tests with chat id ≠ user id.

### Review gate RU3
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
- Language preference (`/langue`) instead of the Telegram app language (finding 12).
- Export/share a recipe as text (finding 13); several photos at once.

---

## Review Log
| Gate | Date | Commits | Findings | Outcome |
|---|---|---|---|---|
