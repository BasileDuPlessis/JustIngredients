# Telegram Bot Feature Specification

> Describes the code as it is (verified against `main` at `ec2e7d3`, 2026-10-27). Planned changes
> are not described here: see [UX Improvements](tasks/ux-improvements.md) and
> [LLM Extraction Migration](tasks/llm-extraction-migration.md).

## Overview
The bot reads the ingredient list of a recipe photo, lets the user review and correct it, and
saves it as a recipe. Saved recipes can be listed, opened, renamed, edited and deleted. Messages
are in English or French, following the Telegram app language.

## Architecture

### Runtime (`src/main.rs`)
- teloxide 0.17 `Dispatcher` with **long polling** (no webhook, no public bot endpoint). The
  HTTP server on port 8080 only serves health checks and metrics (see [Deployment](deployment.md)).
- Two branches: messages go to `bot::message_handler_with_cache`, callback queries to
  `bot::callback_handler_with_cache`.
- Updates of one chat are handled one at a time (teloxide default); different chats run
  concurrently.
- Private chats only: a command, photo or document from a group or channel gets
  `private-chat-only` (other text and service messages get nothing, so a group where the bot sees
  every message is not flooded), and a button pressed in a group answers it as a toast. So the
  chat id is always the user id.
- Dialogue state: `InMemStorage<RecipeDialogueState>` keyed by chat id. It is lost on every
  restart or deploy.
- Duplicate deliveries of a message are dropped by `deduplication` (key: chat id + message id,
  5 minutes, 10,000 entries). Callback queries are not deduplicated.
- Telegram client timeout 30 s. A heartbeat (`getMe` every minute) feeds the health checks.
- Before the dispatcher starts, `telegram_connection::connect_with_retry` calls `getMe`: while
  Telegram is unreachable it retries (1 s doubling up to 30 s, for 10 minutes, then exits and Fly
  restarts it); a rejected token exits at once with `Telegram rejected TELEGRAM_BOT_TOKEN`. The
  dispatcher uses `try_dispatch_with_listener`, so no startup path panics. The heartbeat starts
  before this wait, so `/health/ready` answers 503 until Telegram has answered.

### Components
```
src/main.rs                 # Startup, extractor choice, dispatcher
src/telegram_connection.rs  # getMe with retry before the dispatcher starts
src/dialogue.rs             # RecipeDialogueState, RecipeDialogue
src/localization.rs         # Fluent bundles (en, fr), language detection
src/extraction/             # IngredientExtractor trait, Gemini client, guard (see llm-extraction.md)
src/bot/
├── message_handler.rs      # MessageServices, routing of messages by type and dialogue state
├── cleanup.rs              # Messages of an action, deleted when it ends
├── commands.rs             # Command parsing, Telegram command menu
├── command_handlers.rs     # Welcome, help, recipe list, search command, unsupported messages
├── media_handlers.rs       # Photo and image document messages
├── image_processing.rs     # Download, extraction call, review message, error keys
├── dialogue_manager.rs     # Text input of each dialogue state, save to database
├── home.rs                 # End of an action: cleanup, recipe list sent again last
├── html.rs                 # Html type (escaped text, <b>, <pre>), send_html / edit_html
├── ui_builder.rs           # Review list and keyboards, recipe list and card keyboards
├── ui_components.rs        # Buttons, pagination row, focused edit keyboard
├── recipe_list.rs          # Recipe list: one button per recipe, newest first, 8 per page
└── callbacks/
    ├── callback_handler.rs    # Dispatch by dialogue state, then by data prefix
    ├── review_callbacks.rs    # ReviewIngredients buttons (edit, delete, confirm, cancel)
    ├── editing_callbacks.rs   # EditingSavedIngredients buttons
    ├── recipe_callbacks.rs    # Card actions, delete confirmation
    ├── workflow_callbacks.rs  # Recipe list, pagination, "all my recipes" and older post-save buttons
    └── callback_types.rs      # Parameter structs
```

### `MessageServices`
Built once in `main.rs` and cloned per message (all fields reference-counted):
`pool` (Postgres), `localization`, `cache` (`CacheManager`), `deduplicator`, `extractor`
(`SharedExtractor`). The callback branch receives the pool, localization and cache directly.

## Messages and Commands
| Input | Behaviour |
|---|---|
| Any text starting with `/` | A command, in every dialogue state (`commands::route_text`): a running action ends first (dialogue exited, its recorded messages deleted; the recipe list stays and is not sent again), then the command runs. The typed command is recorded as transient: it goes with its answer at the next action's end, at once for `/liste` and `/annuler`. |
| `/liste`, `/list`, `/recipes` | Moves the recipe list to the bottom (like the end of an action). |
| `/start` | Welcome message at the bottom (it replaces the list or the active card) with "📚 Liste des recettes"; it goes at the next action's end. |
| `/aide`, `/help` | Help message at the bottom with "📚 Liste des recettes" (`home::answer_at_bottom`); it goes at the next action's end. |
| `/annuler`, `/cancel` | Ends the running action (like any command) and moves the recipe list down. |
| `/cherche <mots>`, `/search` | Searches the user's recipes (`recipe_list::show_search_results`, `crate::search`): names and ingredient names as whole words (a query word of 6 letters or more also finds the words it starts), ignoring case, accents, plurals and small words (also "recette"); every word must be found; names rank above ingredients, then the newest. The results replace the bottom message: a bold title with the count, up to 8 recipe buttons (`recipe_instance:<id>`, list labels), "precisez" beyond 8, then 📚 `show_list`; recorded as a list. Nothing found: the `home-send-photo` tip under the title. Database error: `search-failed` at the bottom with the list button. Without words: `search-prompt` with the list button. |
| Unknown `/command` | `unknown-command` and the menu commands at the bottom with the list button, transient. |
| Other text, no dialogue | A search, like `/cherche <text>` (U4.2); the typed text goes with the previous answers. Several lines or more than 80 characters (`search::is_query`) are a pasted recipe, not a search: the text stays and `free-text-tip` (photos add recipes, a word or two searches) comes at the bottom with the list button. |
| Other text in a dialogue | Handled by the current state (see [Dialogue States](#dialogue-states)). |
| Photo | Largest size, caption used as recipe name candidate. Handled whatever the state: a successful extraction replaces the current dialogue with a new review. |
| Document | Only `image/*` MIME types, caption ignored; otherwise `error-unsupported-format` or `error-no-mime-type`. |
| Anything else | `unsupported-*` message. |

Commands are read by `commands::parse_command`: case-insensitive, an `@botname` suffix is
ignored, words after the command are kept for search and ignored otherwise. At startup the bot
sets the Telegram command menu (`setMyCommands`) in the background, a failure is only logged:
`/liste`, `/cherche`, `/annuler` and `/aide` for French users, `/list`, `/search`, `/cancel` and
`/help` for everyone else. The welcome and help messages list the same commands, in
the user's language.

## Photo Flow
1. `download_file_bytes` downloads the file into memory, at most 10 MB (`MAX_IMAGE_BYTES`),
   checked on the declared size, `Content-Length` and the body. Errors never contain the file
   URL (it embeds the bot token). Too large: `error-extraction-image-too-large`; other failures:
   `error-download-failed`.
2. A processing message `processing-reading` ("⏳ Lecture de la recette…") is sent, without a
   button: the chat's next update waits for this one, so a Cancel press could only act once the
   review is shown (it used to cancel the review that had just appeared).
3. `ImageInput::prepare`: JPEG, PNG and WebP are sent as they are; other formats the `image`
   crate decodes (BMP, TIFF, GIF…) are converted to JPEG, one conversion at a time, with
   8,000 px and 96 MB decoding limits.
4. `SharedExtractor` (`Arc<dyn IngredientExtractor>`): `GeminiExtractor` (model
   `GEMINI_MODEL`, default `gemini-3.5-flash-lite`; retries, 20 s per attempt, 40 s per image,
   circuit breaker) wrapped in `GuardedExtractor` (`GEMINI_MAX_CONCURRENCY` slots, default 2;
   a photo waiting more than 30 s for a slot gets the busy message; successful results cached by
   SHA-256 of the image, 100 entries, 24 h). Details: [Ingredient Extraction](llm-extraction.md).
   There is no OCR fallback (Tesseract removed in migration Task 5.1).
5. The processing message is edited with the result:
   - an error: the `error-extraction-*` message of the error (see [Errors](#errors));
   - `is_recipe = false`: `extraction-not-a-recipe`;
   - no ingredient: `no-ingredients-found` with the transcription;
   - otherwise the review message, and the state becomes `ReviewIngredients`.
6. Recipe name (`image_processing::known_recipe_name`): the caption without links (`http://`,
   `https://`, `www.`) if a valid name remains, else the `title` read on the page by Gemini (if
   valid); such a name is used at confirm without asking. Otherwise the default name "Recipe" and
   the name is asked after confirm.

## Review, Confirm, Name, Save
The review message lists one line per ingredient ("150 g → farine", or "sel" without quantity).
A ⚠️ prefix marks `requires_quantity_confirmation`: `needs_review` from the model, quantity
outside (0, `MAX_QUANTITY`], not storable in `DECIMAL(10,3)`, or unit cut to 50 characters
(`extraction/mapping.rs`).

Keyboard (`create_ingredient_review_keyboard`): one button per ingredient, labelled with its full
line (cut only beyond 60 characters), `item_<i>`; then ✅ `confirm`, ❌ `cancel_review`, and ➕
`add_ingredient`. The same keyboard is used when editing a saved recipe.

- `item_<i>`: the keyboard of the same message becomes the ingredient's menu
  (`create_ingredient_menu_keyboard`, `callbacks::ingredient_menu`): 👉 the line (`noop`),
  🔢 `qty_<i>`, 🏷️ `name_<i>`, ✏️ `edit_<i>`, 🗑️ `delete_<i>`, ⬅️ `back_review` (list keyboard
  again).
- `qty_<i>`, `name_<i>`, `edit_<i>`: the message becomes a focused prompt with only a Cancel
  button (`cancel_ingredient_editing`); state `EditingIngredient` with `field` `Quantity`, `Name`
  or `Line` (`ingredient_editing::apply_edit`). A quantity answer is a number in range or `-`
  for none (`parse_quantity_answer`, "½" accepted), optionally followed by a unit that then
  replaces the unit, stored like a typed line's (`parse_quantity_with_unit`: "250 g", "250g",
  "250 grammes" → g, "20 cl" → 200 ml, "2 gousses"); anything else gets
  `edit-invalid-quantity-answer`. It clears the ⚠️ flag; a name answer replaces only the name
  (1–100 characters); a line answer is parsed by `validation::parse_ingredient_from_text`: a
  list bullet or number ignored ("- 2 œufs", "1) 2 œufs"), a quantity first ("2", "2,5", "1/2",
  "1 1/2", "1-1/2", "½") or last after the name ("Farine : 200 g"; "Farine type 00" stays a
  name), then maybe a unit of `units.rs` (canonical spelling, cl/dl converted to ml, count units
  like "gousses" or "pincée"; "bouquet" and "filet" only before "de"/"d'"/"of", so "1 bouquet
  garni" stays a name), "de"/"d'"/"of" dropped after a unit. The line is no longer flagged,
  except a range ("2-3", "2 à 3", "2 ou 3"): its lower bound, flagged ⚠️ like the extraction
  does. Refused with the reason: several lines (`edit-one-line`), no name after a quantity, a
  name over 100 characters, a quantity of 0, negative, over 10,000 or unreadable ("1/0"), a line
  over 200 characters. A cancel word (see below) or the button restore the review.
- `add_ingredient`: the same focused prompt (`add-ingredient-prompt`), state `EditingIngredient`
  with `editing_index` = the list's length; the parsed line becomes the last ingredient
  (`ingredient_editing::put_ingredient`).
- `delete_<i>`: removes the line. When none is left, the message offers ➕ (`add_ingredient`,
  same as above) and ❌ `cancel_review`; ➕ is on every review keyboard, also an empty one. Old messages may still send `add_more`, handled like
  `add_ingredient`.
- `confirm` (and a typed confirm word, see below; both call `review_callbacks::confirm_review`):
  an empty list gets `review-empty-confirm` (a recipe without ingredients could not be edited
  later); **quantity guard**: if a line has ⚠️, the bot sends `review-quantities-to-check`
  with their names and stays in the review until they are edited or deleted. The review list is
  recorded for cleanup as soon as it is sent (with the photo), so a command that ends the review
  deletes it too, and so does a cancel. Then:
  - name from the caption or the page title: saved at once; the review loses its buttons; the
    line "✅ Recette « X » créée" follows (`home::send_created_line`, `recipe-created-line`, no
    card: the card shows when the recipe is opened), then the
    [recipe list](#end-of-an-action-the-recipe-list-last);
  - otherwise: buttons removed, `recipe-name-prompt` sent, state
    `WaitingForRecipeNameAfterConfirm`. The typed name (a cancel word cancels; empty
    or over 255 bytes refused) is saved; the prompt is deleted with the review messages and the
    `recipe-created-line` is sent, followed by the recipe list.
- `cancel_review` (and a typed cancel word; both call `review_callbacks::cancel_review`): a
  cancelled creation leaves nothing behind (photo, list and prompts deleted), dialogue ended,
  recipe list sent again. The same for a cancel at the name prompt.
- Older messages may still carry the post-save buttons that the home message replaced:
  📚 `workflow_list_recipes` (recipe list) and 🔍 `workflow_search_recipes` (the pressed message
  goes, `search-prompt` at the bottom, under the action's message when one runs); both are still
  handled.

Typed text in `ReviewIngredients`: a confirm word ("confirm", "ok", "yes", "save", "valider",
"valide", "confirmer", "oui", "enregistrer") does exactly what ✅ does (quantity guard, then save
under the caption or title name, or ask for the name); a cancel word cancels; anything else
answers `review-help`. Cancel words, everywhere a prompt is open: "cancel",
"stop", "back", "annuler", "annule", "retour", "arrêter" (`is_cancellation_command`).

Saving (`save_ingredients_to_database` → `db::save_recipe`): the user, the recipe with its name
(content = the transcription) and its ingredients in one transaction, nothing written on error;
quantities are parsed with `parse_quantity` ("1/2", "1 1/2"). Owner id: the callback sender (`from.id`) on the caption path, the chat id on the
typed paths; they are equal in private chats (IMP-024).

## End of an Action: the Recipe List Last
`home::send_home` ends every finished action:
- saving a recipe: caption path, typed name, typed "confirm";
- cancelling: the review (button or typed word), the name prompt;
- a saved recipe: rename (also when it fails), delete confirmed or cancelled, ingredient editing
  confirmed or cancelled (also when the recipe was deleted meanwhile). A delete confirmed or
  cancelled while another action runs (e.g. a rename prompt is open) does not end it, so the
  running action keeps its prompts;
- `/liste`, `/annuler` and the 📚 button of older messages (`workflow_list_recipes`).

It deletes what the action left behind (below) and the previous recipe list, then sends the
list again as the last message, without notification (`recipe_list::show_recipe_list`, page 1).
Since U5.2 the list replaces the former home message: its header
(`recipe_list::render_list_header`) is the bold 📚 title, the number of recipes ("Vous avez 12
recettes.", "Vous n'avez pas encore de recette.") and the photo hint; no keyboard without
recipes. A failure is logged and no list is sent; the action itself is not affected. `/start`
uses `move_list_down` instead, which replaces the list but keeps the action's messages, so its
welcome stays above the list until the next action ends.

Before sending the list, `send_home` deletes what the action left behind (`bot::cleanup`, one
`deleteMessages` call per 100 ids, failures logged at debug level):
- the previous recipe list (and the home message of older versions), so a chat has one list at
  the bottom; one Telegram no longer lets the bot delete (older than 48 h) shrinks to "📚 ⬇️"
  without buttons. A home message or list of a version before U5 (never recorded) is deleted
  when one of its buttons is pressed (`cleanup::remove_legacy_message`); a paged one is recorded
  as a list from then on (`adopt_legacy_list`);
- the messages recorded during the action: every text the user typed in a dialogue state
  (recorded in `handle_text_message`) and the bot prompts sent with `cleanup::send_transient`:
  quantity questions and the "check the quantities" warning, "try again" answers (name, edit,
  quantity), `review-help`, `use-buttons-instruction`, the rename and add-ingredient prompts,
  and the name prompt when the name is cancelled;
- the user's photo or image document, recorded once its review starts: it stays while the
  ingredients are reviewed and goes when the review ends (saved, cancelled or interrupted by a
  command). A photo whose extraction fails is not recorded;
- the review message (ingredient list), recorded when it is sent, and the name prompt: after
  saving, the saved recipe card and the list remain.

Kept (the history): recipe cards and one line per action (U5.3), sent without notification:
`recipe-created-line` ("✅ Recette « X » créée"), `recipe-renamed-line` ("✏️ « X » renommée en
« Y »"), `recipe-edited-line` ("📝 « X » modifiée", only when something changed),
`recipe-deleted-line` ("🗑️ « X » supprimée", which replaces the deleted recipe's card in place).
A cancel leaves nothing. An error at the end of an action (recipe gone, database error) and a
photo that gave no review (download or extraction error, not a recipe, no ingredients) stay at
the bottom instead of the list, the last one carrying "📚 Liste des recettes", until the next
action ends (`home::send_home_keeping`, U6.3). A photo sent during an action ends it first. Only recorded messages are deleted. The
records are message ids (never content) in the `chat_messages` table (kind `transient`, `list`,
or `home` for older versions), so a deploy or a restart does not forget them; records older than 48 h are dropped
without deleting (Telegram refuses them). A database error only leaves the messages in the chat.

## My Recipes
- **List** (`/liste`, 📚 button, `recipe_list::show_recipe_list`): one button per named recipe
  of the user, newest first, 8 per page, labelled with the name (cut at 30 characters); a name
  that appears more than once on the page adds the day ("Tarte · 27 oct. 2026"), and the time
  when they were saved the same day (Paris). Two recipes with the same name are two entries. Callback data `recipe_instance:<id>`. Pagination row: ⬅️ `page:<n>`, "Page x of y"
  (`noop`, ignored), ➡️ `page:<n>`; it edits the list message, and a page past the end (recipes
  deleted meanwhile) shows the last one.
- **Selection** (owner = callback sender): `recipe_instance:<id>` opens the card
  (`cards::show_card`): the list goes and the card is sent at the bottom with its buttons,
  recorded as the active card (`chat_messages` kind `active_card`, at most one per chat; U6.1).
  Earlier cards of the recipe stay in the history: every consultation is kept. A recipe that is
  gone or another user's answers `recipe-not-found` until the next action.
- **Recipe card**: name, creation date (user's language, Paris time: `format_recipe_date`), ingredients. Buttons:
  ✏️ `recipe_action:rename:<id>`, 📝 `recipe_action:edit_ingredients:<id>`, then
  🗑️ `recipe_action:delete:<id>`, then 📚 `show_list` ("Liste des recettes"): the card goes into
  the history without buttons and the list comes back at the bottom (`handle_show_list`; the ⬅️
  `back_to_recipes` of older cards and the old home's 📚 do the same). History cards never have
  buttons; a rename or an edit updates every card of the recipe in place (`refresh_cards`). The 📊
  statistics of older cards (`recipe_action:statistics:<id>`) are retired.
- **Actions from the card** (U6.2): rename, edit and delete put the card into the history
  without buttons and show their message at the bottom, whose last button is "❌ Annuler":
  it deletes the action's messages and gives the card its buttons back (`home::back_to_card`);
  typed cancel words do the same. Review and edit keyboards end with ➕, ✅, then ❌ alone.
- **Rename**: prompt, state `RenamingRecipe`, typed name saved with `update_recipe_name`
  (a cancel word leaves nothing); an empty or too long name is refused and the prompt stays
  open. Names are unique per user: renaming to a taken name stores "X (2)" (`db::rename_recipe`),
  and so does saving a second "X"; the lines show the stored name. A rename updates the recipe's recorded cards in place (`cards::refresh_cards`) and leaves
  `recipe-renamed-line`.
- **Edit ingredients**: a separate editing view, recorded as transient (it goes when the edit
  ends); state `EditingSavedIngredients` with the review keyboard and the same
  ingredient menu; 🔢, 🏷️ and ✏️ open `EditingSavedIngredient` (typed quantity, name or line,
  `cancel_ingredient_editing`). Saved ingredients without quantity stay without quantity (they
  were shown and could be saved as "1"). ➕ `add_ingredient` opens
  `AddingIngredientToSavedRecipe` (typed line), 🗑️ removes, ✅ `confirm` writes the differences
  in one transaction (`db::apply_ingredient_changes`: exact values, so a quantity can be cleared)
  and updates the recipe's recorded cards in place; a recipe without a recorded card (shown
  before U5.4) gets the editing view turned into its card instead,
  ❌ `cancel_review` (or a typed cancel word, both `editing_callbacks::cancel_saved_editing`)
  drops them and shows the card again; ✅ on an emptied list gets `review-empty-confirm`. Other
  typed text outside the prompts answers `use-buttons-instruction`.
- **Delete**: confirmation with `confirm_delete_recipe:<id>:<card message id>` and
  `cancel_delete_recipe:<id>:<card message id>`; confirming deletes the recipe, its ingredients
  and the confirmation, turns the card into `recipe-deleted-line` (no buttons, no longer a
  recorded card) and removes any other card of the recipe; cancelling
  deletes the confirmation only (the card stays).
- Recipe lookups and card ingredients are read from the database each time (no cache), so a card
  opened right after a rename, edit or delete shows the new data.

## Callback Data
`callback_handler_with_cache` first runs the handler of the current state, then the prefix
handlers below, which work in any state. The query is always answered, also when a handler
failed (the error is logged). A review or edit button pressed while no review or edit is running
(`is_stale_button`: the bot restarted since, dialogues being in memory, or a newer action replaced
it), or on a message older than the running review or edit (message ids grow within a chat, so
the list of a previous photo cannot save or edit the current one), answers `button-expired`,
loses its keyboard and is not passed to the state handler.

| Data | Handled in | Action |
|---|---|---|
| `item_<i>`, `back_review`, `qty_<i>`, `name_<i>`, `edit_<i>`, `delete_<i>`, `confirm`, `cancel_review` | `ReviewIngredients`, `EditingSavedIngredients` | Review or saved-recipe editing (above) |
| `add_ingredient`, `add_more` (old messages) | `ReviewIngredients`, `EditingSavedIngredients` | Add a line (focused prompt in a review, `AddingIngredientToSavedRecipe` for a saved recipe) |
| `cancel_ingredient_editing` | `EditingIngredient`, `EditingSavedIngredient` | Leave the focused edit prompt |
| `recipe_instance:<id>` | any | Open a recipe by id (recipe list, home message of older versions) |
| `select_recipe:`, `select_recipe_h:`, `recipe_action:statistics:` | any | Retired (2026-10-28): stale button |
| `recipe_action:<action>:<id>` | any | `rename`, `edit_ingredients`, `delete` |
| `confirm_delete_recipe:`, `cancel_delete_recipe:` | any | Delete confirmation |
| `back_to_recipes` | any | Delete the current message |
| `page:<n>` | any | Recipe list page |
| `workflow_list_recipes`, `workflow_search_recipes` | any | Recipe list (home "all my recipes" button), how to search; `workflow_add_another` is handled but never sent |
| `cancel_processing` | any | Retired (2026-10-28): answered as a stale button (`button-expired`, keyboard removed) |

`noop` (the line shown at the top of an ingredient menu) is inert on purpose: the query is
answered and nothing else happens. `workflow_*` data is handled once, by the prefix handler. A
command typed during a review (`/liste`, `/annuler`) ends it, but the review list is not one of
the action's tracked messages, so it stays; its buttons then answer `button-expired`.

## Dialogue States
`RecipeDialogueState` (`src/dialogue.rs`); every variant except `Start` keeps the language code
of the message that opened it.

| State | Entered by | Text input |
|---|---|---|
| `Start` (default) | no state | Commands and echo |
| `ReviewIngredients` | Successful extraction; end of an edit | "confirm"/"ok"/"yes"/"save", "cancel"/"stop", else help |
| `EditingIngredient` | `qty_<i>`, `name_<i>`, `edit_<i>` or `add_ingredient` in a review | Quantity, name or whole line (`field`); a new line when adding |
| `WaitingForRecipeNameAfterConfirm` | `confirm` (button or typed) without a caption or title name | Recipe name |
| `RenamingRecipe` | `recipe_action:rename` | New name |
| `EditingSavedIngredients` | `recipe_action:edit_ingredients` | Refused (use the buttons) |
| `EditingSavedIngredient` | `qty_<i>`, `name_<i>` or `edit_<i>` on a saved recipe | Quantity, name or whole line (`field`) |
| `AddingIngredientToSavedRecipe` | `add_ingredient` on a saved recipe | New ingredient line |

## Errors
Extraction errors map to locale keys in `extraction_error_key` (`image_processing.rs`):

| Key | `ExtractionError` |
|---|---|
| `error-extraction-unsupported-image` | `UnsupportedImage` |
| `error-extraction-image-too-large` | `ImageTooLarge` (also before download) |
| `error-extraction-busy` | `RateLimited` (HTTP 429 or no free slot within 30 s) |
| `error-extraction-daily-limit` | `DailyLimitReached` (`EXTRACTION_DAILY_LIMIT_PER_USER` photos read today, UTC) |
| `error-extraction-unreadable` | `Blocked`, `Recitation`, `Truncated`, `InvalidResponse`, other 4xx about the image |
| `error-extraction-unavailable` | `Config`, `Timeout`, `Network`, 5xx, invalid key or model, `CircuitOpen` |

Other keys: `extraction-not-a-recipe`, `no-ingredients-found`, `error-download-failed`,
`error-unsupported-format`, `error-no-mime-type`, `save-failed-retry` (the save failed and nothing was
written: the review or name prompt stays open to try again), `error-processing-failed`.
Configuration errors (invalid key, unknown model) are logged as errors, the others as warnings;
all are counted in `errors_total{type, component=<backend>}`. Handler errors are logged by the
dispatcher.

## Localization
- Fluent files `locales/en/main.ftl` and `locales/fr/main.ftl`, embedded at build time and
  parsed once at startup (`LocalizationManager::new`). Variables are inserted without Unicode
  isolation marks; a formatting error (e.g. a missing argument) is logged at `warn` with the key
  and the message is sent with the placeholder.
- Language: `language_code` of the message or callback sender, `fr-FR` → `fr`; `fr` and `en`
  are supported, anything else gets English. A key missing in French falls back to English,
  then to "Missing translation: <key>". Tests check that every literal key used in `src/`
  exists in both files, that both files have the same keys, and that every message formats.
- `users.language_code` is stored when the user is created but not used to pick the language.
- The key screens are sent in HTML (`parse_mode = HTML`) through `bot::html`: an `Html` value is
  built only from escaped text, `<b>` and `<pre>`, and sent only with `send_html`/`edit_html`. Bold:
  the titles of the review list, the saved-recipe editing view, the recipe card (name), the home,
  welcome and help messages (and their section headers); the no-ingredients message shows the
  transcription in a `<pre>` block. Every other message is plain text. Locale values have no
  `**` markers or `[CODE]` prefixes (guarded by `tests/localization_tests.rs`).

## Testing
- Unit tests next to the code: error keys present in both locales, caption cleaning, callback
  data length, quantity guard, duplicate detection.
- `tests/bot_tests.rs`, `tests/dialogue_tests.rs`, `tests/localization_tests.rs`: keyboards,
  formatting, state values, locale files. `tests/integration/` (one binary, `main.rs` declares
  `bot_flow_tests`): caption and naming flows, dialogue state values. No test drives the
  handlers against a fake Telegram.
- `tests/gemini_live.rs`: ignored by default, calls the paid API.
- Manual check in Telegram on staging, then production.

## Planned Changes
Source of truth: [UX Improvements](tasks/ux-improvements.md).
- Phase U1: HTML formatting with escaping, short commands (`/liste`, `/cherche`, `/aide` and
  English aliases) with a Telegram menu, a home message and cleanup at the end of each
  transaction.
- Phase U2: recipe title read by Gemini, one button per ingredient, field-level editing, no dead
  Cancel button during processing.
- Phase U3: one list entry per recipe (newest first, 8 per page), simpler recipe card, one owner
  identity (IMP-024).
- Phase U4: French-aware search from a command, the search button and free text.
- Target CI/CD pipeline: Task 5.4 of [LLM Extraction Migration](tasks/llm-extraction-migration.md).
