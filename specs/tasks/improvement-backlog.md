# Improvement Backlog

## Goal
Keep one prioritized list of verified code-quality, correctness and security improvements, so
they can be pulled into 1-hour micro-tasks without another review.

## Source and Scope
- Source: Rust best-practices review of 2026-10-26 (read-only). All items were checked in the
  code at commit `6537576`. `cargo clippy --all-targets -W clippy::pedantic -W clippy::nursery`
  and `cargo audit` were run. The test suite was not run.
- In scope: `src/main.rs`, `src/lib.rs`, `src/bot/**`, `src/db.rs`, `src/cache.rs`,
  `src/text_processing.rs` (manual-input parts), `src/validation.rs`, `src/ingredient_editing.rs`,
  `src/deduplication.rs`, `src/dialogue.rs`, `src/localization.rs`, `src/config.rs`,
  `src/errors.rs`, `src/circuit_breaker.rs`, `src/observability*`, `tests/`, `Cargo.toml`.
- Out of scope: code that the LLM migration deletes (`src/ocr*`, `src/preprocessing/`,
  `src/error_correction.rs`, `src/instance_manager.rs`, `src/path_validation.rs`, the hOCR recovery in
  `src/bot/image_processing.rs`).
- Checked and found OK: no SQL injection (all user input goes through `.bind()`; only the static
  migration SQL runs as dynamic SQL). No `std::sync::Mutex` guard is held across an `.await`.

## How to Use
1. Pick the highest-priority open item. Turn it into one or more micro-tasks (<= 1 hour each) in
   the task spec that touches the same code, or work on it directly if it is S effort.
2. Use the IMP id in the commit message (e.g. `fix(bot): IMP-001 ...`).
3. Tick `- [x] Done` here once its Validation steps pass.
4. Items marked **Related: LLM migration** must be done in coordination with
   [llm-extraction-migration.md](llm-extraction-migration.md). Do not do them twice.

## Legend
- Priority: **P0** bug or security issue, fix now. **P1** should fix soon. **P2** quality.
  **P3** nice to have.
- Effort: **S** < 1 h. **M** 1-4 h. **L** > 4 h.

## Summary
| ID | Title | Priority | Effort | Area |
|---|---|---|---|---|
| IMP-001 | Every message except `/recipes` is dropped as a duplicate | P0 | S | Bot routing |
| IMP-002 | Users can read, rename, edit and delete other users' recipes (IDOR) | P0 | M | Security / DB |
| IMP-003 | Bot token can leak into logs; file download has no timeout | P0 | S | Security / HTTP |
| IMP-004 | Quantities > 1000 become `0`; confirm button saves `0`; manual "1500 g" rejected | P1 | M | Parsing / Bot |
| IMP-005 | Quantity parsing is inconsistent; mixed numbers are saved as NULL | P1 | M | Parsing / DB |
| IMP-006 | Multi-row writes are not transactional | P1 | M | DB |
| IMP-007 | Buttons that do nothing, and stale buttons that act on the wrong recipe | P1 | M | Bot callbacks |
| IMP-008 | Commands are swallowed inside dialogue states; no `/cancel` | P1 | S | Dialogue |
| IMP-009 | Telegram limits not enforced (`callback_data` 64 bytes, message 4096 chars) | P1 | M | Bot UI |
| IMP-010 | Cache is never invalidated or purged; stale recipes shown after edits | P1 | S | Cache |
| IMP-011 | Metrics and health server is publicly reachable; weak auth and rate limiting | P1 | S | Security / Ops |
| IMP-012 | `cargo audit` reports 18 vulnerabilities; `deny.toml` unused | P1 | S | Dependencies |
| IMP-013 | Integration tests never compiled; CI runs `--lib` only; DB tests destructive | P1 | M | Tests / CI |
| IMP-014 | `DATABASE_URL` logged at startup (latent); startup logs lost | P1 | S | Security / Logging |
| IMP-015 | `mem::forget(NamedTempFile)` leaks one file descriptor per image | P1 | S | Resources |
| IMP-016 | `AppConfig` is not the source of truth; many settings ignored | P2 | M | Config |
| IMP-017 | Error types are strings; validation errors matched by string | P2 | L | Error handling |
| IMP-018 | No-panic policy not enforced (~70 `expect`/`unreachable!` in scope) | P2 | M | Error handling |
| IMP-019 | `span.enter()` held across `.await` (9 sites) | P2 | S | Async / Tracing |
| IMP-020 | Misleading metrics and OpenTelemetry that exports nothing | P2 | M | Observability |
| IMP-021 | Localization re-parses the `.ftl` file on every message | P2 | S | Localization |
| IMP-022 | Dialogue state is in memory only; lost on every deploy | P2 | M | Dialogue |
| IMP-023 | Small dialogue handler bugs (rename, cancel text, lost state) | P2 | S | Dialogue |
| IMP-024 | User identity mixes `chat.id` and `from.id`; group chats share data | P2 | S | Security / Bot |
| IMP-025 | `**bold**` and code fences shown literally (no `parse_mode`) | P2 | M | Bot UI |
| IMP-026 | Schema: missing index, no cascade, raw text duplicated per ingredient | P2 | S | DB |
| IMP-027 | Positional `row.get(i)` (panics) and duplicated row mapping | P2 | M | DB |
| IMP-028 | Inline migrations with a hand-written SQL splitter | P2 | M | DB |
| IMP-029 | `update_recipe_ingredients` binds `telegram_id` as `user_id` | P2 | S | DB |
| IMP-030 | One global circuit breaker without a half-open state | P2 | M | Resilience |
| IMP-031 | Tests do not exercise handlers; several tests are tautological | P2 | L | Tests |
| IMP-032 | Logging hygiene: user content at `info`, `error!` on normal paths | P2 | S | Logging |
| IMP-033 | Locale placeholder bug: `.replace("{}", …)` vs `{$msg}` | P2 | S | Localization |
| IMP-034 | Dependency hygiene: unused crates, duplicates, two TLS stacks, no MSRV | P3 | M | Dependencies |
| IMP-035 | Dead code (unused caches, DB functions, handlers, helpers) | P3 | M | Code organization |
| IMP-036 | Oversized modules and duplicated handlers | P3 | L | Code organization |
| IMP-037 | `docs/schema.sql` does not match the live migration | P3 | S | Docs |
| IMP-038 | `u64 as i64` casts for Telegram ids; curated clippy pedantic set | P3 | S | Lints |
| IMP-039 | Health checks load Tesseract every minute; bot-token check is a no-op | P3 | S | Observability |
| IMP-040 | Pre-push hook untracked, slow, and missing `cargo fmt --check` | P2 | S | Tooling |
| IMP-041 | Docker image built twice, pushed on PRs, no build cache, unused CI artifact | P2 | M | CI/CD |
| IMP-042 | Deprecated Node 20 actions; smoke tests on Node 18 check health only | P3 | S | CI/CD |
| IMP-043 | README describes a pipeline that does not exist | P3 | S | Docs |
| IMP-044 | Dispatcher panics when Telegram is unreachable or the token is invalid at startup | P2 | S | Reliability |
| IMP-045 | Locale keys used in code are not checked against the `.ftl` files | P2 | S | Localization |
| IMP-046 | Timing benchmarks run as blocking tests | P2 | M | Tests / CI |
| IMP-047 | No per-user limit on paid extractions | P3 | S | Cost |
| IMP-048 | OCR leftovers kept only for tests | P3 | S | Observability |
| IMP-049 | About 40 locale keys are never used by the code | P3 | S | Localization |
| IMP-050 | Deploys use the image tag, not its digest | P3 | S | CI/CD |
| IMP-051 | Observability and HTTP settings of `.env.example` are never read | P3 | S | Config |
| IMP-052 | `update_recipe_ingredients` writes the Telegram id as `ingredients.user_id` | P3 | S | Database |

## Verification of Earlier Findings
| # | Earlier finding | Result | Item |
|---|---|---|---|
| 1 | `DATABASE_URL` logged in plaintext | Confirmed with a correction: the line exists (`src/main.rs:198`), but the tracing subscriber is installed later (`src/main.rs:230`), so today the event is discarded. It becomes a real leak as soon as tracing init moves earlier. | IMP-014 |
| 2 | Token in download URL can reach logs; no timeout | Confirmed | IMP-003 |
| 3 | `mem::forget(NamedTempFile)` leaks a file descriptor | Confirmed | IMP-015 |
| 4 | Quantity > 1000 set to `0`; confirm button ignores the flag | Confirmed, and it also makes manual input of "1500 g" fail | IMP-004 |
| 5 | `db.rs:680` binds `telegram_id` as `user_id` | Confirmed, but the function is only called from tests | IMP-029 |
| 6 | `.replace("{}", msg)` never matches `{$msg}` | Confirmed | IMP-033 |
| 7 | Edit paths use `.parse::<f64>()` | Confirmed. The main save path also loses mixed numbers (`1 1/2`) | IMP-005 |
| 8 | `ObservabilityConfig::default()`; `AppConfig.ocr` unused at runtime | Confirmed, and the DB pool and HTTP timeout settings are ignored too | IMP-016 |
| 9 | `docs/schema.sql` stale | Confirmed | IMP-037 |
| 10 | Global circuit breaker without half-open state | Confirmed | IMP-030 |
| 11 | `tests/integration/*.rs` never compiled; CI `--lib` only | Confirmed | IMP-013 |
| 12 | Migrations inline in `db.rs` | Confirmed | IMP-028 |

---

## P0: Fix Now

### IMP-001: Every message except `/recipes` is dropped as a duplicate
- [x] Done
- Priority: P0. Effort: S. Area: Bot routing.
- Location: `src/bot/message_handler.rs:586-597` (first check), `src/bot/message_handler.rs:635`
  (delegates with the same deduplicator), `src/bot/message_handler.rs:513-524` (second check),
  `src/deduplication.rs:66-103`.
- Problem: `message_handler_with_cache` calls `is_duplicate()`, which records the request and
  returns `false`. It then calls `message_handler(..., deduplicator)`, which calls
  `is_duplicate()` again for the same `(chat_id, message_id)`. The second call returns `true`, so the
  handler logs "Ignoring duplicate request" at `debug` and returns `Ok(())`. Since commit `a0d24fa`
  (2026-03-18), `/start`, `/help`, every photo, every document and every dialogue text input are
  silently ignored. Only `/recipes` and button callbacks still work. No test calls the `_with_cache`
  handlers, so nothing caught it.
  Production check: `telegram_duplicate_messages_total` on `/metrics` grows with every message.
- Proposed fix: run the dedup check once. Remove the `deduplicator` parameter from the inner
  handler, or make it a private `route_message()` without dedup, and keep the check only in the entry
  point used by `main.rs`.
- Validation:
  1. Only one `is_duplicate` call is left in the message path.
  2. Add a handler-level regression test (see IMP-031): one `/start` update gets exactly one reply.
  3. On staging, `/start` and a photo get replies, and `telegram_duplicate_messages_total` stays at 0
     for normal traffic.

### IMP-002: Users can read, rename, edit and delete other users' recipes (IDOR)
- [x] Done
- Priority: P0. Effort: M. Area: Security / DB.
- Location:
  - Callbacks: `src/bot/callbacks/recipe_callbacks.rs:216-268` (rename),
    `src/bot/callbacks/recipe_callbacks.rs:515-527` (delete), `src/bot/callbacks/recipe_callbacks.rs:726-736`
    (edit ingredients), `src/bot/callbacks/recipe_callbacks.rs:368-378` (statistics),
    `src/bot/callbacks/callback_handler.rs:703-733` (instance view).
  - Dialogue: `src/bot/dialogue_manager.rs:628` (rename write).
  - DB queries filtered by id only: `src/db.rs:94`, `src/db.rs:144-172`, `src/db.rs:505`,
    `src/db.rs:534`, `src/db.rs:585`, `src/db.rs:716`, `src/db.rs:737`.
- Problem: recipe ids come from `callback_data` (`recipe_action:delete:<id>`,
  `confirm_delete_recipe:<id>:<msg>`, `recipe_instance:<id>`). They are used without checking that
  `recipes.telegram_id` belongs to the caller. Callback data is client-controlled: a modified client
  or MTProto user-bot can send any `data` for a bot message. Ids are a sequential `BIGSERIAL`, so
  they are easy to enumerate.
  Failure scenario: user A sends `confirm_delete_recipe:42:0` and deletes user B's recipe 42 with all
  its ingredients. `edit_ingredients` loads B's ingredients into A's dialogue, and A's confirm
  then updates or deletes B's rows.
- Proposed fix: add the owner to every recipe and ingredient query, for example
  `WHERE id = $1 AND telegram_id = $2`, and join `ingredients` to `recipes` on owner for ingredient
  writes. Pass the caller's id from `q.from.id` (see IMP-024). Treat "not found or not yours" the
  same way and show `recipe-not-found`. Keep the owner check in the DB layer, not in the handlers, so
  new handlers cannot forget it.
- Validation:
  1. DB tests: create recipes for users A and B. Every scoped function called with A's id and B's
     recipe id returns `None`, `false` or 0 rows affected.
  2. Handler test: a forged `confirm_delete_recipe:<B id>:0` from A replies "not found" and B's
     recipe still exists.
  3. `rg "WHERE id = \$1\"" src/db.rs` finds no unscoped recipe or ingredient query.

### IMP-003: Bot token can leak into logs; file download has no timeout
- [x] Done
- Priority: P0. Effort: S. Area: Security / HTTP.
- Related: LLM migration (Task 0.1 and Task 2.3 cover the same code. Do it in Phase 0, not later.)
- Location: `src/bot/image_processing.rs:102-135` (`download_file`), `src/bot/image_processing.rs:111`
  (`reqwest::get`), `src/bot/image_processing.rs:157` (`log_network_error`), `src/errors.rs:121-134`.
- Problem:
  - The URL `https://api.telegram.org/file/bot<TOKEN>/<path>` is fetched with `reqwest::get`. A
    `reqwest::Error` `Display` includes `for url (...)`, and the error is logged with
    `error = %error`. Any connect or read failure writes the full bot token to Fly logs, and a leaked
    token means full bot takeover.
  - `reqwest::get` builds a new client each time (no connection reuse) with no timeout. Teloxide
    handles each chat's updates in sequence, so a stalled download blocks that user's chat
    indefinitely.
  - The HTTP status is not checked (`error_for_status()` is missing), so a 4xx/5xx body is saved as
    the image.
- Proposed fix: download through teloxide's `Download::download_file` (it reuses the bot client
  and its 30 s timeout), or through one shared `reqwest::Client` with connect and total timeouts.
  Call `.error_for_status()` and strip the URL from errors (`reqwest::Error::without_url()`) before
  logging. Check `file.size` from `get_file` against the cap before downloading.
- Validation:
  1. Unit test: make a request to `http://127.0.0.1:9/file/bot123:SECRET/x`, format the resulting
     error the way `log_network_error` does, and assert that `SECRET` is absent.
  2. Test against a local TCP listener that never answers: the download fails within the timeout.
  3. Test that a 404 response returns an error, not a file.

---

## P1: Should Fix Soon

### IMP-004: Quantities > 1000 become `0`; confirm button saves `0`; manual "1500 g" rejected
- [x] Done (triage 2026-10-28): Confirm refuses flagged quantities and lists them; one `MAX_QUANTITY`; the Gemini mapping keeps the values.
- Priority: P1. Effort: M. Area: Parsing / Bot.
- Related: LLM migration (the plan maps `needs_review` to `requires_quantity_confirmation`, so
  the button path must check that flag first, or Gemini's review signal is ignored).
- Location: `src/text_processing.rs:935-941` (flagged quantity replaced by `"0"`),
  `src/text_processing.rs:1630-1640` (`> 1000` rule), `src/bot/callbacks/review_callbacks.rs:450-616`
  (confirm button, no flag check), `src/bot/dialogue_manager.rs:954-993` (text "confirm" path, which
  does check), `src/validation.rs:213-218`, `src/validation.rs:343-350`.
- Problem:
  1. "1500 g flour" is flagged as anomalous and its quantity is replaced by `"0"`, so the original
     value is lost.
  2. The inline **Confirm** button saves without checking `requires_quantity_confirmation`, so
     `0.000` is stored. Only the typed "confirm" text goes through `AwaitingQuantityCorrection`.
  3. Manual edits call `parse_ingredient_from_text`, which runs the same detector: "1500 g farine"
     becomes `"0"`, and then `validate_quantity_range` rejects it with `edit-invalid-quantity`. Users
     cannot enter 1.5 kg as grams.
- Proposed fix: keep the original quantity and only set the flag, never overwrite with `"0"`.
  Make the threshold unit-aware (g/ml up to 10000, consistent with `validate_quantity_range`) or use
  the validation range only. Make the Confirm button go through the same
  "first flagged ingredient → `AwaitingQuantityCorrection`" logic as the text path, using one
  shared function.
- Validation:
  1. Parser test: "1500 g sugar" keeps quantity `1500`.
  2. `parse_ingredient_from_text("1500 g farine")` is `Ok`.
  3. Handler test: Confirm with a flagged ingredient enters `AwaitingQuantityCorrection`, and no row
     with quantity 0 is written.
- Progress (2026-10-27, R3): the Confirm button refuses to save flagged quantities and lists them
  (`quantities_to_check`, tested); the anomaly threshold is the validation maximum (10000), so
  "1500 g" is kept and editable; typed quantity answers are range-checked. Remaining: anomalous
  quantities (> 10000, letter mixes) are still replaced by `"0"` instead of kept and flagged.

### IMP-005: Quantity parsing is inconsistent; mixed numbers are saved as NULL
- [x] Done (2026-10-28): `parse_quantity` reads mixed numbers ("1 1/2"); saved-recipe edits store the parsed value (fractions) and can clear a quantity or unit; change detection compares the stored values (no quantity is not 1).
- Priority: P1. Effort: M. Area: Parsing / DB.
- Related: LLM migration (Task 1.1 units module and Task 1.3 mapping should own the single parser).
- Location: `src/validation.rs:239-262` (`parse_quantity`), `src/text_processing.rs:1525-1535`
  (`1½` normalized to `"1 1/2"`), `src/bot/dialogue_manager.rs:1145`, `src/bot/callbacks/editing_callbacks.rs:469`,
  `src/bot/callbacks/editing_callbacks.rs:534`, `src/db.rs:656`, `src/db.rs:676`,
  `src/ingredient_editing.rs:15`, `src/ingredient_editing.rs:65`.
- Problem:
  - `parse_quantity("1 1/2")` splits on `/` and fails to parse `"1 1"`, so it returns `None`. The
    parser itself produces `"1 1/2"` for `1½` and `2 1/4`, so the main save path stores NULL for
    every mixed number.
  - The saved-recipe edit paths use `.parse::<f64>()`: an added "1/2 cup sugar" is stored with
    quantity NULL. An updated quantity "1/2" is silently ignored because
    `update_ingredient` uses `COALESCE($2, quantity)`.
  - `ingredients_to_measurement_matches` turns a NULL quantity into `"1"`, and
    `detect_ingredient_changes` treats an unparsable edit as `1.0`. Editing any field of a
    NULL-quantity ingredient writes `1`.
- Proposed fix: one `parse_quantity` (decimal, comma decimal, `a/b`, `n a/b`, Unicode fractions)
  returning `Result<Decimal-like f64, QuantityError>`, used by every path. Keep NULL as NULL (show
  it as an empty quantity in the UI).
- Validation:
  1. Table tests: `1/2`, `1 1/2`, `2¼`, `1,5`, `½`, `0`, `abc`.
  2. DB test: save and then edit a recipe with "1 1/2 cup flour" and a NULL-quantity ingredient.
     The stored values are 1.5 and NULL.
  3. `rg "parse::<f64>\(\)" src/bot src/db.rs src/ingredient_editing.rs` finds nothing.

### IMP-006: Multi-row writes are not transactional
- [x] Done (2026-10-28): `db::save_recipe` (user upsert, recipe with its name, ingredients) and `db::apply_ingredient_changes` (updates, additions, deletions) each run in one transaction; `delete_recipe` too. DB tests: a failing ingredient leaves no recipe; another user's recipe is refused without writing. A failed save still exits the dialogue (IMP-023).
- Priority: P1. Effort: M. Area: DB.
- Related: LLM migration (Task 3.2 changes the same save path, so do this first or in the same task).
- Location: `src/bot/dialogue_manager.rs:1077-1190` (`save_ingredients_to_database`),
  `src/bot/callbacks/editing_callbacks.rs:431-600` (saved-recipe confirm), `src/db.rs:144-172`
  (`delete_recipe`), `src/db.rs:175-245` (`get_or_create_user`).
- Problem:
  - A new recipe is saved with 1 `INSERT` recipe, 1 `UPDATE` name and N `INSERT` ingredient
    statements on the pool. If ingredient k fails, the recipe exists with k-1 ingredients. The user
    sees "processing failed" (and loses the dialogue, see IMP-023), and the partial recipe shows up
    in `/recipes`.
  - The saved-recipe confirm runs N `update_ingredient`, `create_ingredient` and `delete_ingredient`
    calls one by one and returns early on the first error, leaving a half-applied edit.
  - `delete_recipe` deletes the ingredients, then the recipe, in two separate statements.
  - `get_or_create_user` does SELECT then INSERT, so two concurrent first requests can hit the
    `UNIQUE (telegram_id)` violation.
- Proposed fix: one `sqlx::Transaction` per use case: `save_recipe(tx, …)` doing
  `INSERT … RETURNING id` with the name and a multi-row ingredient insert (`UNNEST`), and
  `apply_ingredient_changes(tx, …)`. Use `INSERT … ON CONFLICT (telegram_id) DO UPDATE … RETURNING`
  for users, and `ON DELETE CASCADE` (IMP-026) or one transaction for delete.
- Validation:
  1. DB test that forces an ingredient insert failure (name longer than 255 chars) leaves no recipe
     row.
  2. A concurrent `get_or_create_user` test (two tasks) succeeds.
  3. The DB round-trips for saving 10 ingredients drop from 12 to 2-3 (count with a test
     wrapper or `sqlx` logging).

### IMP-007: Buttons that do nothing, and stale buttons that act on the wrong recipe
- [ ] Done
- Status: Partly done (2026-10-28): a review or edit button with no dialogue left answers "this button is no longer active" and loses its keyboard; the query is always answered, even when a handler fails. Left: dead buttons of U2.5 and the message-id check (a button of an older list acting on the current one).
- Priority: P1. Effort: M. Area: Bot callbacks.
- Location: `src/bot/ui_builder.rs:131-138` (`add_ingredient` on every review keyboard),
  `src/bot/callbacks/review_callbacks.rs:57-131` (review routing), `src/bot/callbacks/review_callbacks.rs:316-335`
  and `src/bot/callbacks/editing_callbacks.rs:316-320` (`add_more` and `cancel_empty` buttons),
  `src/bot/callbacks/editing_callbacks.rs:48-110` (saved-edit routing),
  `src/bot/callbacks/review_callbacks.rs:166-170` and `src/bot/callbacks/review_callbacks.rs:283-288`
  (`parse().unwrap_or(0)`), `src/bot/callbacks/callback_handler.rs:59-172` and
  `src/bot/callbacks/callback_handler.rs:206-321`.
- Problem:
  - In `ReviewIngredients` (new recipe), the "➕ Add ingredient" button sends `add_ingredient`, but
    only `add_more` is handled, so the button does nothing. `cancel_empty` (shown after the last
    ingredient is deleted) has no handler at all. `add_more` is not handled in
    `EditingSavedIngredients`.
  - Callbacks are applied to the current dialogue state without checking that `q.message.id` is the
    state's `message_id`. If the user sends photo A, then photo B, then taps "🗑️ 2" on A's message,
    ingredient 2 of recipe B is deleted.
  - Malformed `edit_x` or `delete_x` data parses to index 0 and edits or deletes the first ingredient.
  - `workflow_*` is handled twice in `ReviewIngredients`: once in `review_callbacks.rs:122-131` and
    again in the general branch.
  - If any general handler fails (`?`), `answer_callback_query` is skipped and the button keeps
    spinning. When the state is gone (restart, IMP-022), Confirm does nothing and shows no message.
- Proposed fix: a typed `enum CallbackData` with `to_string()` and `FromStr` (one place defines
  the wire format), parsed once in `callback_handler`. Route with an exhaustive `match` per
  `(state, callback)`. Reject callbacks whose message id does not match the state, with a
  "this button is no longer active" answer. Always answer the query, also on error.
- Validation:
  1. Unit test: every button produced by `ui_builder` and `ui_components` parses into a
     `CallbackData` that is handled in the state where it is shown (table test).
  2. Handler test: a callback from an older message does not change the current state.
  3. Manual check on staging: Add ingredient works in a new-recipe review.

### IMP-008: Commands are swallowed inside dialogue states; no `/cancel`
- [x] Done (2026-10-28): commands win in every dialogue state (UX Task U1.6); cancel words in French and English ("annuler", "retour", "arrêter"…), confirm words too ("valider", "oui"…); `/annuler` (`/cancel`) ends the action and shows the home message, and is in the menu.
- Priority: P1. Effort: S. Area: Dialogue.
- Location: `src/bot/message_handler.rs:59-355` (state match before commands),
  `src/bot/message_handler.rs:357-368` (commands), `src/bot/dialogue_manager.rs:725-727`
  (`is_cancellation_command`).
- Problem: commands are only checked when the state is `Start` or `None`. In
  `WaitingForRecipeNameAfterConfirm`, typing `/start` saves a recipe named "/start". In
  `EditingIngredient`, `/recipes` is parsed as an ingredient. The only escape words are the English
  "cancel", "stop" and "back", which French users do not know, and there is no `/cancel` command.
- Proposed fix: handle `/start`, `/help`, `/recipes` and a new `/cancel` before the state match
  (exit the dialogue first, then answer). Treat any text starting with `/` as a command, never as
  data. Accept localized cancel words ("annuler") from the `.ftl` files, and register commands with
  `set_my_commands`.
- Validation:
  1. Handler tests: `/cancel` in each state exits the dialogue, and `/start` in
     `WaitingForRecipeNameAfterConfirm` does not create a recipe.
  2. `rg "\"cancel\" \| \"stop\"" src/bot` shows one shared helper.

### IMP-009: Telegram limits not enforced (`callback_data` 64 bytes, message 4096 chars)
- [x] Done (2026-10-28): callback data carry ids or a name fingerprint (hotfix `1bf3ed4`, U2.2
  `item_<i>`, tests check ≤ 64 bytes); ingredient lists and the no-ingredients transcription are
  cut at a line with "…" (`ui_builder::fit_lines`, 3,500 characters), tested with 200
  ingredients. Not handled: a keyboard over 100 buttons (more than 98 ingredients).
- Priority: P1. Effort: M. Area: Bot UI.
- Location: `src/bot/ui_builder.rs:204-209` (`select_recipe:{recipe_name}`), `src/validation.rs:46`
  (names up to 255 bytes), `src/bot/image_processing.rs:237-243` (full extracted text in one message),
  `src/bot/ui_builder.rs` `format_ingredients_list`.
- Problem: Telegram rejects `callback_data` longer than 64 bytes (`BUTTON_DATA_INVALID`). A recipe
  name longer than 50 bytes (`select_recipe:` takes 14) makes the whole `/recipes` keyboard fail, so
  the user cannot list any recipe on that page. French accents use 2 bytes each. Messages longer than
  4096 characters (a long recipe text or ingredient list) make `edit_message_text` fail, and the user
  is left with "processing…".
- Proposed fix: use ids in callback data (`select_recipe:<id of the newest recipe with that name>`
  and resolve the name with an owner-scoped query), plus a `debug_assert!` or test helper that checks
  `len() <= 64` for every button. Truncate or split long messages with a shared
  `fit_message(&str) -> Vec<String>` helper.
- Validation:
  1. Test: a 255-character recipe name produces callback data of 64 bytes or less.
  2. Test: a 200-ingredient list is split into messages of 4096 characters or less.

### IMP-010: Cache is never invalidated or purged; stale recipes shown after edits
- [x] Done (2026-10-28, with UX Task U3.4): the recipe and ingredient reads of the bot no longer use a
  cache (the database answers in a few milliseconds); `get_recipes_by_name_cached` and
  `get_recipe_ingredients_cached` were deleted. The user caches, never called, stay for IMP-035.
- Priority: P1. Effort: S. Area: Cache.
- Location: `src/db.rs:838-875` (`get_recipes_by_name_cached`), `src/db.rs:878-915`
  (`get_recipe_ingredients_cached`), `src/bot/callbacks/callback_handler.rs:604-625`,
  `src/bot/callbacks/callback_handler.rs:673`, `src/bot/callbacks/callback_handler.rs:733`,
  `src/cache.rs:619` (`cleanup_all`, never called), `src/cache.rs:211` (`MemoryCache::get`).
- Problem: no write path (save, rename, edit ingredients, delete) invalidates the 5-minute caches.
  After editing a saved recipe, opening it again from `/recipes` shows the old ingredients, which
  looks like "my edit was not saved". A deleted recipe still appears among the instances for a
  duplicated name. Expired entries are never removed (`cleanup` is never called), so memory grows
  with every recipe viewed, on a 256 MB VM. Each `get` also takes a write lock on the stats.
- Proposed fix: remove these caches. The queries are single-user and indexed (IMP-026), and the
  bot's traffic does not justify caching. If caching is kept, invalidate keys in the DB write
  functions and use a bounded TTL cache (e.g. `moka`).
- Validation:
  1. Test: view, edit, then view again shows the new ingredients.
  2. Test: after deleting one of two same-named recipes, the instance list has one entry.

### IMP-011: Metrics and health server is publicly reachable; weak auth and rate limiting
- [ ] Done
- Priority: P1. Effort: S. Area: Security / Ops.
- Location: `deploy/fly.toml` (`[http_service] internal_port = 8080`,
  `METRICS_BIND_ALL_INTERFACES = 'true'`), `src/observability/metrics.rs:65-81` (auth optional,
  non-constant-time compare), `src/observability/metrics.rs:40-61` (rate limiter),
  `src/observability/metrics.rs:275`, `src/observability/metrics.rs:333`,
  `src/observability/metrics.rs:359`, `.github/workflows/deploy-production.yml` (curls the public
  `/health/ready` without a token).
- Problem: the bot uses long polling and needs no inbound HTTP, yet `/metrics`, `/health/live` and
  `/health/ready` are served on the public `just-ingredients.fly.dev`. The post-deploy step curls
  `/health/ready` without a token, so either `METRICS_AUTH_TOKEN` is unset in production (then
  `/metrics` is public) or that step always fails. `/health/ready` returns raw error text
  (`NOT READY: <db error>`). The rate limiter keys on `peer_addr`, which behind the Fly proxy is the
  proxy address, so all internet clients share one 10 req/min bucket, which the `[[http_service.checks]]` probes
  may also share. Its `HashMap` is never pruned, and it uses `.expect()`.
- Proposed fix: remove `[http_service]` (machine `[checks]` keep working on the private port) or
  require `METRICS_AUTH_TOKEN` in production for everything except `/health/live`. Return a fixed
  body on readiness failure and log the details. Key the limiter on `Fly-Client-IP` when present,
  bound and prune the map, and compare tokens in constant time. Coordinate the deploy workflow change
  with the CI/CD audit.
- Validation:
  1. `curl https://just-ingredients.fly.dev/metrics` returns 401 or is unreachable.
  2. Unit test: the limiter keeps at most N keys, and an expired key is removed.

### IMP-012: `cargo audit` reports 18 vulnerabilities; `deny.toml` unused
- [x] Done (triage 2026-10-28): dependencies updated, cargo-deny `advisories sources` in CI (`447fd9d`).
- Priority: P1. Effort: S. Area: Dependencies.
- Location: `Cargo.lock`, `.github/workflows/build-test.yml` (security job), `config/deny.toml`.
- Problem: `cargo audit` (2026-10-26) reports "18 vulnerabilities found" and 10 warnings: `rustls`
  0.23.36 (4 advisories), `rustls-webpki` (5), `aws-lc-sys` 0.37.0 (5), `h2` 0.4.13
  (RUSTSEC-2026-0258), `crossbeam-epoch` 0.9.18, `webpki-roots`, and `rsa` (already ignored).
  Most have semver-compatible fixes. The CI security job only ignores RUSTSEC-2023-0071, so it fails,
  which breaks the "CI green before the next task" rule of the migration plan. `config/deny.toml`
  is not at the repository root and no workflow runs `cargo deny`.
- Proposed fix: `cargo update`, re-run `cargo audit`, and document any remaining ignore with a reason.
  Move `deny.toml` to the root, or call `cargo deny --config config/deny.toml check` in CI with
  `EmbarkStudios/cargo-deny-action`. Install tools with prebuilt binaries instead of
  `cargo install` on every run.
- Validation: `cargo audit` reports 0 vulnerabilities apart from documented ignores, and
  `cargo deny check advisories bans` passes in CI.

### IMP-013: Integration tests never compiled; CI runs `--lib` only; DB tests destructive
- [ ] Done
- Priority: P1. Effort: M. Area: Tests / CI.
- Related: LLM migration (Task 0.2 covers CI. This item adds the details it needs.)
- Location: `tests/integration/bot_flow_tests.rs`, `tests/integration/ocr_tests.rs` (20 tests, no
  `main.rs` or `mod integration;`), `.github/workflows/build-test.yml:28` (`cargo test --lib`),
  `tests/db_tests.rs:19-47`, `tests/test_helpers.rs` (compiled as its own empty test crate).
- Problem: Cargo only discovers `tests/*.rs` and `tests/<dir>/main.rs`, so the 20 tests in
  `tests/integration/` never compile and may already be broken. CI skips all `tests/*.rs` binaries
  and doc tests. `setup_test_db` drops `users`, `recipes` and `ingredients` in whatever
  `DATABASE_URL` points to, but keeps `schema_migrations`. On the second run
  `init_database_schema` skips migration 1, so every DB test fails with "relation does not exist".
  The tests also drop the tables while other tests in the same binary run in parallel.
- Proposed fix: add `tests/integration/main.rs` with `mod bot_flow_tests;` (or move the files),
  and use `mod test_helpers;` from the tests that need it. Use `#[sqlx::test]` (a fresh database per
  test) with a `TEST_DATABASE_URL` and a Postgres service container in CI. Run
  `cargo test --all-targets` and `cargo test --doc`.
- Validation:
  1. The CI log lists the `integration` and `db_tests` binaries with non-zero test counts.
  2. Running `cargo test` twice locally gives the same result.

### IMP-014: `DATABASE_URL` logged at startup (latent); startup logs lost
- [x] Done
- Priority: P1. Effort: S. Area: Security / Logging.
- Related: LLM migration (Task 0.1).
- Location: `src/main.rs:198` (`info!(database_url = %database_url, …)`), `src/main.rs:230`
  (tracing init), `src/main.rs:77`, `src/main.rs:214`, `src/main.rs:218`.
- Problem: the URL, including the password, is formatted into a log event. Today it is discarded
  only because the subscriber is installed 30 lines later, which also means that every startup log
  before line 230 (env validation, cache and dedup init) is lost. The natural fix for the lost logs
  (init tracing first) would publish the password.
- Proposed fix: initialize tracing first, then log only the host and database name (parse with
  `sqlx::postgres::PgConnectOptions` or `url::Url` and drop the user info). Add a `Secret<String>`
  newtype with redacting `Debug`/`Display` for the DB URL, bot token and Gemini key (shared with
  migration Task 1.2).
- Validation:
  1. Unit test on the redaction helper.
  2. The staging startup log shows "Initializing database connection host=… db=…" with no password.

### IMP-015: `mem::forget(NamedTempFile)` leaks one file descriptor per image
- [x] Done
- Priority: P1. Effort: S. Area: Resources.
- Related: LLM migration (Task 2.3 removes the temp file. Fix now if the Tesseract backend ships
  in the transition release.)
- Location: `src/bot/image_processing.rs:127-134`, `src/bot/image_processing.rs:47-81`
  (`TempFileGuard`).
- Problem: forgetting a `NamedTempFile` skips closing its `File`, so every processed image leaks one
  file descriptor until the process restarts. After roughly `ulimit -n` images (often 1024), sockets
  and DB connections can no longer be opened and the bot stops working. File I/O is also done
  synchronously on the async runtime.
- Proposed fix: `temp_file.into_temp_path()` (closes the file and deletes it on drop) instead of
  `forget` plus the custom guard, or keep the bytes in memory (Task 2.3).
- Validation: process 50 images on staging and check that `ls /proc/<pid>/fd | wc -l` stays stable.

---

## P2: Quality

### IMP-016: `AppConfig` is not the source of truth; many settings ignored
- [ ] Done
- Priority: P2. Effort: M. Area: Config.
- Related: LLM migration (Task 1.2 adds `GeminiConfig` to `AppConfig`, so do this first or together).
- Location: `src/config.rs:329` (`OcrConfig::default()`), `src/config.rs:332`
  (`ObservabilityConfig::default()`), `src/observability.rs:63`, `src/main.rs:17-166` (second copy
  of the validation), `src/main.rs:201` (`PgPool::connect` with defaults),
  `src/main.rs:248-251` (hard-coded 30 s timeout), `src/main.rs:217` (hard-coded dedup TTL).
- Problem: `ENVIRONMENT`, `OTLP_ENDPOINT`, `TRACE_*` and `OBSERVABILITY_LOG_LEVEL` are ignored
  because of `ObservabilityConfig::default()`, so production always runs as "development".
  `DATABASE_MAX_CONNECTIONS`, `DATABASE_CONNECT_TIMEOUT_SECS`, `DATABASE_MIN_CONNECTIONS`,
  `HTTP_CLIENT_TIMEOUT_SECS`, `REQUEST_DEDUPLICATION_TTL_SECS` and `METRICS_PORT` are validated but
  never used. `AppConfig.ocr` is validated but the runtime uses a separate static `OCR_CONFIG`. The
  env vars are read and validated twice with different code.
- Proposed fix: load `AppConfig::from_env()` once in `main`, validate it, and pass the parts
  explicitly (pool via `PgPoolOptions` with `max_connections` around 5 and `acquire_timeout` of 5 s
  for a 256 MB VM, the bot client timeout, the dedup TTL, observability). Delete the `validate_*`
  functions in `main.rs` and the unused `METRICS_PORT`.
- Validation: tests that each env var changes the corresponding field, and the startup log shows
  `AppConfig::summary()` with `environment=production` on Fly.

### IMP-017: Error types are strings; validation errors matched by string
- [ ] Done
- Priority: P2. Effort: L. Area: Error handling.
- Location: `src/errors.rs:9-59`, `src/validation.rs:43-47` (`"empty"`, `"too_long"`),
  `src/bot/dialogue_manager.rs:280-292` and `src/bot/dialogue_manager.rs:494-511` (string matching),
  `src/bot/dialogue_manager.rs:37-39` (`error.to_string().contains("message is not modified")`),
  `src/db.rs` (every function returns `anyhow::Result`), `src/errors.rs:165` (dead
  `log_validation_error` with a byte slice `&v[..100]` that panics on a multi-byte character).
- Problem: `AppError` variants only hold `String`, `From<sqlx::Error>` and `From<anyhow::Error>`
  stringify and drop the source chain, and `source()` is not implemented. Callers cannot tell
  "not found" from "connection lost". Typos in the matched strings fail silently into the `_` arm.
  The project rule asks for domain-specific error enums.
- Proposed fix: `thiserror` enums per module: `DbError { NotFound, Conflict, Sqlx(#[from] sqlx::Error) }`,
  `ValidationError { EmptyName, NameTooLong, InvalidQuantity, … }` with a `fn l10n_key(&self)`, and
  `BotError` at the handler boundary. Keep `anyhow` only in `main`. Use teloxide's
  `ApiError::MessageNotModified` instead of matching the text.
- Validation: `rg "AppError::Validation\(\"" src` is empty, and a test checks that each
  `ValidationError` maps to an existing `.ftl` key in both languages.

### IMP-018: No-panic policy not enforced (~70 `expect`/`unreachable!` in scope)
- [ ] Done
- Priority: P2. Effort: M. Area: Error handling.
- Location: `src/bot/callbacks/editing_callbacks.rs` (29 sites), `src/bot/callbacks/review_callbacks.rs`
  (22), `src/db.rs:335`, `src/db.rs:354`, `src/db.rs:366`, `src/db.rs:385`, `src/db.rs:397`,
  `src/db.rs:866`, `src/db.rs:905`, `src/bot/callbacks/workflow_callbacks.rs:199`,
  `src/bot/callbacks/workflow_callbacks.rs:226`, `src/bot/callbacks/workflow_callbacks.rs:245`,
  `src/bot/callbacks/callback_handler.rs:556`, `src/observability/metrics.rs:47`,
  `src/bot/dialogue_manager.rs:582`, `scripts/check-unwraps.sh`, CI clippy `--allow clippy::expect_used`.
- Problem: the policy forbids `unwrap`, `expect` and `panic!`, but the check script only searches for
  `.unwrap()` and CI explicitly allows `expect_used`. Most `expect`s come from parameter structs with
  `Option` fields (`ReviewIngredientsParams`, `SavedIngredientsParams`) that are then expected, and
  from `q.message.expect(...)`, which is `None` for inline-mode messages. A panic aborts handling of
  that update without a reply to the user.
- Proposed fix: replace the `Option` fields with per-handler parameter structs that hold the required
  values. Extract the chat id once with a helper that returns `Result`. Add
  `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable))]`
  in `lib.rs` and `main.rs`. Allow-list only the compile-time constant regexes, or move them to
  `LazyLock<Result<…>>`.
- Validation: `cargo clippy --all-targets -- -D warnings` passes with the deny attribute, and CI stops
  using `--allow clippy::expect_used`.

### IMP-019: `span.enter()` held across `.await` (9 sites)
- [ ] Done
- Priority: P2. Effort: S. Area: Async / Tracing.
- Location: `src/bot/message_handler.rs:511`, `src/bot/message_handler.rs:584`,
  `src/bot/callbacks/callback_handler.rs:49`, `src/bot/callbacks/callback_handler.rs:196`,
  `src/db.rs:61`, `src/db.rs:419`, `src/db.rs:625`, `src/db.rs:800`, `src/db.rs:923`.
- Problem: an `Entered` guard kept across `.await` stays active on the worker thread while the task
  is suspended. Events from other tasks on that thread are then recorded in the wrong span, for
  example log lines of user B tagged with user A's `user_id`. The tracing documentation calls this
  incorrect.
- Proposed fix: `#[tracing::instrument(skip_all, fields(user_id = …))]` on the async functions, or
  `async { … }.instrument(span).await`.
- Validation: `rg "span.enter\(\)" src` is empty, and a test with `tracing-subscriber`'s test writer
  shows two concurrent handlers logging their own `user_id`.

### IMP-020: Misleading metrics and OpenTelemetry that exports nothing
- [ ] Done
- Priority: P2. Effort: M. Area: Observability.
- Location: `src/observability/system_monitoring.rs:14-19` (first `statm` field is virtual size, not
  RSS), `src/observability/system_monitoring.rs:33` and `src/observability/system_monitoring.rs:42`
  (placeholders), `src/observability/metrics.rs:333` (`uptime_seconds` set to 1.0),
  `src/observability/metrics.rs:790` (`daily_active_users` counts every action),
  `src/bot/message_handler.rs:550`, `src/bot/message_handler.rs:638`,
  `src/bot/callbacks/callback_handler.rs:175`, `src/bot/callbacks/callback_handler.rs:324` (status
  always 200), `src/observability/tracing_mod.rs:67-95` (tracer provider without a
  `tracing-opentelemetry` layer).
- Problem: `process_memory_mb` reports virtual memory, usually far above the 256 MB limit, so it
  cannot warn before an OOM. CPU, active threads and uptime are constants. Errors are counted as 200.
  An OTLP provider is installed, but no layer bridges `tracing` spans to it, so nothing is ever
  exported (and the config is ignored anyway, see IMP-016).
- Proposed fix: report RSS (second `statm` field multiplied by the page size, or the
  `metrics-process` crate). Remove the placeholder gauges. Record `status = ok | error` from the
  handler result. Either add `tracing-opentelemetry` or drop the three OpenTelemetry crates (see
  IMP-034).
- Validation: on staging, `process_resident_memory_bytes` matches `ps -o rss`, and a failing handler
  increments the error series.

### IMP-021: Localization re-parses the `.ftl` file on every message
- [ ] Done
- Priority: P2. Effort: S. Area: Localization.
- Location: `src/localization.rs:21-47` (`create_bundle`), `src/localization.rs:56-92`,
  `src/localization.rs:80` (formatting errors dropped into `&mut vec![]`).
- Problem: every `t_lang` call copies and parses the full 11-12 KB `.ftl` file and builds a new
  bundle. `/help` alone parses it 15 times, and a review message does it dozens of times, on a
  shared CPU. Missing arguments (IMP-033) are hidden because formatting errors are dropped.
  `use_isolating` is left on, so every variable is wrapped in invisible U+2068/U+2069 characters.
- Proposed fix: build `fluent_bundle::concurrent::FluentBundle`s once in
  `LocalizationManager::new()` and keep them in a `HashMap<&'static str, Bundle>`. Call
  `set_use_isolating(false)`. Log formatting errors at `warn` with the key.
- Validation: a test that every key in `en` exists in `fr` and formats without errors, and a
  test or counter showing each `.ftl` resource is parsed once per process.

### IMP-022: Dialogue state is in memory only; lost on every deploy
- [ ] Done
- Status: Partly done (2026-10-28, short term): after a restart, review and edit buttons say they are no longer active instead of doing nothing. Left: dialogues are still lost on every deploy (persistent storage).
- Priority: P2. Effort: M. Area: Dialogue.
- Location: `src/main.rs:258` (`InMemStorage`), `src/dialogue.rs:85`.
- Problem: each deploy or restart drops every in-progress review. The user's next tap on Confirm
  does nothing (IMP-007), and the extracted ingredients (a paid Gemini call after the migration) are
  lost. Finished conversations are never evicted, and each state keeps the full extracted text.
- Proposed fix: short term, answer callbacks with no matching state with "session expired, please
  resend the photo". Medium term, implement teloxide's `Storage` trait on a Postgres `dialogues` table
  (JSON state, `updated_at`) with a TTL cleanup, since `RecipeDialogueState` is already
  `Serialize`.
- Validation: restart the bot during a review on staging. The user gets a clear message, or the
  review continues once storage is persistent.

### IMP-023: Small dialogue handler bugs (rename, cancel text, lost state)
- [ ] Done
- Priority: P2. Effort: S. Area: Dialogue.
- Location: `src/bot/dialogue_manager.rs:680-720`, `src/bot/dialogue_manager.rs:615`,
  `src/bot/dialogue_manager.rs:985-991`, `src/bot/dialogue_manager.rs:1590-1596`,
  `src/bot/dialogue_manager.rs:968`, `src/bot/dialogue_manager.rs:489`,
  `src/bot/dialogue_manager.rs:1038`, `src/bot/dialogue_manager.rs:1640`.
- Problem:
  - The rename validation-error branches say "Keep dialogue active", but `dialogue.exit()` at line 720
    always runs, so the user's second attempt is answered with "Received: …".
  - Cancelling a rename shows the `delete-cancelled` text.
  - `if let Some(…{ message_id: ref mut … }) = dialogue.get().await?` changes a local copy and is
    never saved.
  - The typed "confirm" path drops `recipe_name_from_caption`.
  - When the save fails, the dialogue exits and the user's reviewed ingredients are lost.
- Proposed fix: return early instead of falling through to `exit()`. Use a `rename-cancelled` key.
  Build the state with the message id before calling `dialogue.update`. Keep the state on DB errors
  so the user can retry.
- Validation: handler tests for each case (a too-long name followed by a valid one renames the
  recipe, and a DB failure keeps `ReviewIngredients`).

### IMP-024: User identity mixes `chat.id` and `from.id`; group chats share data
- [ ] Done
- Priority: P2. Effort: S. Area: Security / Bot.
- Location: `src/bot/command_handlers.rs:108`, `src/bot/callbacks/workflow_callbacks.rs:78`,
  `src/bot/callbacks/workflow_callbacks.rs:135`, `src/bot/dialogue_manager.rs:389`,
  `src/bot/dialogue_manager.rs:999`, `src/bot/dialogue_manager.rs:1601` (use `chat.id`),
  `src/bot/callbacks/review_callbacks.rs:483`, `src/bot/callbacks/editing_callbacks.rs:503` (use
  `from.id`), `src/main.rs:274`, `src/main.rs:300-309` (dialogue keyed by chat).
- Problem: in private chats the two ids are equal. In a group, `/recipes` lists the group's recipes,
  the caption path saves under the user id while the name path saves under the group id, and every
  member's text feeds the same dialogue. Any member can edit or delete what another member saved.
- Proposed fix: restrict the dispatcher to private chats (`dptree::filter(|m: Message| m.chat.is_private())`
  and the same check for callbacks), and add one `fn owner_id(&Update) -> TelegramUserId` newtype
  used everywhere.
- Validation: a group message gets a "private chat only" reply, and `rg "chat.id.0" src/bot` only
  appears in send calls, not in DB calls.

### IMP-025: `**bold**` and code fences shown literally (no `parse_mode`)
- [ ] Done
- Priority: P2. Effort: M. Area: Bot UI.
- Location: `src/bot/command_handlers.rs:42`, `src/bot/image_processing.rs:238-252`,
  `src/bot/callbacks/callback_handler.rs:368`, `src/bot/callbacks/callback_handler.rs:627-628`,
  `src/bot/ui_builder.rs:50-54` and others (no `parse_mode` in `src/`).
- Problem: about 30 message templates contain `**…**` or triple backticks, but no message sets a parse
  mode, so users see the literal asterisks. Dates are always English and UTC
  (`"%B %d, %Y at %H:%M"`), also for French users.
- Proposed fix: send with `ParseMode::Html` and escape every user-provided value (recipe and
  ingredient names, extracted text) with a single `html_escape` helper, or drop the markers. Format
  dates through Fluent (`DATETIME`) or a per-locale pattern.
- Validation: a snapshot test of the review message with a recipe named `<b>&"x"`, and a manual check
  on staging in both languages.

### IMP-026: Schema: missing index, no cascade, raw text duplicated per ingredient
- [ ] Done
- Priority: P2. Effort: S. Area: DB.
- Related: LLM migration (Task 3.2 decides what goes into `recipes.content` and `ingredients.raw_text`).
- Location: `src/db.rs:1315-1322` (`recipes`, no index on `telegram_id`), `src/db.rs:1328`
  (`recipe_id` FK without `ON DELETE CASCADE`), `src/db.rs:1321` and `src/db.rs:771`
  (`to_tsvector('english', …)`), `src/bot/dialogue_manager.rs:1158-1166` (full extracted text
  passed as `raw_text` of every ingredient).
- Problem: every user-facing query filters `recipes.telegram_id`, which means a sequential scan
  as the table grows. `recipes.telegram_id` has no FK to `users`. Delete needs two statements
  (IMP-006). A 20-ingredient recipe stores its OCR text 21 times. Full-text search uses the English
  configuration for French recipes.
- Proposed fix: a new migration (IMP-028) with
  `CREATE INDEX recipes_telegram_id_name_idx ON recipes (telegram_id, recipe_name)` and
  `ON DELETE CASCADE` on `ingredients.recipe_id`. Store the per-line source text (migration Task 3.2),
  and use the `simple` configuration or one tsvector per language.
- Validation: `EXPLAIN` of the `/recipes` query uses the index, and deleting a recipe in one
  statement removes its ingredients.

### IMP-027: Positional `row.get(i)` (panics) and duplicated row mapping
- [ ] Done
- Priority: P2. Effort: M. Area: DB.
- Location: `src/db.rs:104-108`, `src/db.rs:480-491`, `src/db.rs:565-575`, `src/db.rs:596-606`,
  `src/db.rs:777-784`, `src/db.rs:813-820` (same `Recipe`/`Ingredient` mapping repeated). Only
  `src/db.rs:66` uses a compile-time checked `sqlx::query!`.
- Problem: `Row::get` panics on a type mismatch or a wrong index, so reordering a `SELECT` column
  crashes at runtime instead of failing to compile. `Arc<String>` fields in `User` and `Recipe` add
  no value.
- Proposed fix: `#[derive(sqlx::FromRow)]` on `User`, `Recipe` and `Ingredient` with
  `query_as!` (offline data already in `.sqlx/`, the Dockerfile copies it), and plain `String` fields.
- Validation: `rg "row.get\(" src` is empty, and `cargo sqlx prepare --check` runs in CI.

### IMP-028: Inline migrations with a hand-written SQL splitter
- [ ] Done
- Priority: P2. Effort: M. Area: DB.
- Location: `src/db.rs:1288-1592` (`migrations` module, `split_sql_statements` at
  `src/db.rs:1353`).
- Problem: schema changes need Rust edits. The splitter does not handle `$$` blocks or `/* */`
  comments. There is no checksum and no lock, and migration 1 uses `CREATE TABLE IF NOT EXISTS`, so
  a database created by older code (see the extra columns in `docs/schema.sql`) silently keeps its
  old shape.
- Proposed fix: `migrations/0001_initial.sql` (same idempotent SQL) plus `sqlx::migrate!()` (the
  `migrate` feature is on by default), run at startup, and new changes as new files. Drop the splitter
  and the `rollback_to_version` helper.
- Validation: start against an empty DB and against a copy of production, and check
  `_sqlx_migrations` in both.

### IMP-029: `update_recipe_ingredients` binds `telegram_id` as `user_id`
- [x] Done (2026-10-28): `update_recipe_ingredients` deleted (never called by the bot).
- Priority: P2. Effort: S. Area: DB.
- Location: `src/db.rs:619-714`, in particular `src/db.rs:680` (`.bind(recipe.telegram_id)` for
  `ingredients.user_id REFERENCES users(id)`), `src/db.rs:639` and `src/db.rs:669-671` (reads outside
  the transaction).
- Problem: any added ingredient violates the FK (or, by coincidence, attaches to another user), so
  the whole transaction rolls back. The function is only called from tests today. The production
  edit path re-implements it without a transaction (IMP-006).
- Proposed fix: fix the bind (resolve `users.id` in the same transaction) and make this the only
  "apply ingredient changes" function used by `editing_callbacks`, or delete it.
- Validation: a DB test that adds an ingredient to a saved recipe and checks `user_id = users.id`.

### IMP-030: One global circuit breaker without a half-open state
- [x] Done (triage 2026-10-28): the OCR breaker is gone; `extraction::breaker` has a half-open state.
- Priority: P2. Effort: M. Area: Resilience.
- Related: LLM migration (Task 2.2 reuses this breaker for Gemini, so fix it there).
- Location: `src/circuit_breaker.rs:86-90`, `src/circuit_breaker.rs:158-186`,
  `src/circuit_breaker.rs:200-222`, `src/bot/image_processing.rs:99-100` (process-wide static).
- Problem: one bad input stream (any failure is counted) can open the circuit for every user. After
  the reset delay the breaker closes completely instead of allowing one trial request. The count and
  the timestamp sit behind two mutexes and are updated non-atomically.
- Proposed fix: one `Mutex<State { Closed { failures }, Open { until }, HalfOpen }>`, count only
  upstream failures (timeouts, 429, 5xx, not "no ingredients found"), and inject the breaker
  instead of a static.
- Validation: unit tests with a fake clock: threshold opens it, after the cooldown one trial is
  allowed, a success closes it and a failure reopens it.

### IMP-031: Tests do not exercise handlers; several tests are tautological
- [ ] Done
- Priority: P2. Effort: L. Area: Tests.
- Location: `tests/bot_tests.rs:399-470` (calls `Vec::remove` itself instead of the handler),
  `tests/bot_tests.rs:1002-1030` (checks button text, not `callback_data` length), no test references
  `message_handler_with_cache` or `callback_handler_with_cache`.
- Problem: the suite builds structs and keyboards but never sends an update through the dispatcher,
  which is why IMP-001, IMP-004, IMP-007 and IMP-023 went unnoticed.
- Proposed fix: add a seam: handlers depend on a small `trait Messenger` (send, edit, answer) and on a
  repository trait, or use the `teloxide_tests` crate. Write scenario tests: photo → review → confirm
  → saved, edit, delete, cancel, stale button, forged callback (IMP-002).
- Validation: at least one test per `RecipeDialogueState` variant goes through the real routing
  function, and these tests run in CI (IMP-013).

### IMP-032: Logging hygiene: user content at `info`, `error!` on normal paths
- [ ] Done
- Priority: P2. Effort: S. Area: Logging.
- Location: `src/bot/dialogue_manager.rs:1087-1175` (about 10 `info!` per save, including ingredient
  names and "CRITICAL" messages), `src/bot/callbacks/editing_callbacks.rs:536-544` (`error!`
  "Attempting to add new ingredient"), `src/db.rs:189-245`, `src/db.rs:487`
  (`info!("Ingredient found: {:?}")`), `src/db.rs:769` (search query), `src/observability/metrics.rs:736-743`
  (recipe name), `src/text_processing.rs:447` (`info!` on every detector creation).
- Problem: the logs carry users' recipe content at `info`, are noisy, and false `error!` lines hide
  real errors in Fly logs.
- Proposed fix: `debug!` for per-step traces, one `info!` per completed use case with counts and ids
  only, and `error!` only for failures.
- Validation: a staging save of a 10-ingredient recipe produces at most 2 `info` lines and no `error`
  line.

### IMP-033: Locale placeholder bug: `.replace("{}", …)` vs `{$msg}`
- [x] Obsolete (triage 2026-10-28): the `.replace("{}")` code and the OCR keys were removed.
- Priority: P2. Effort: S. Area: Localization.
- Related: LLM migration (Task 3.3 already plans this fix. Tick both together.)
- Location: `src/bot/image_processing.rs:321`, `src/bot/image_processing.rs:337`,
  `locales/en/main.ftl:43`, `locales/en/main.ftl:46` (and the `fr` equivalents).
- Problem: the message is formatted without arguments, so Fluent renders the literal `{$msg}` and
  the `replace("{}", …)` never matches. Users see "OCR processing timed out: {$msg}".
- Proposed fix: `t_args_lang(…, &[("msg", msg)], …)`, and never `replace` on localized strings.
- Validation: a localization test that formats both keys with an argument and contains no `{$`.

---

## P3: Nice to Have

### IMP-034: Dependency hygiene: unused crates, duplicates, two TLS stacks, no MSRV
- [ ] Done
- Priority: P3. Effort: M. Area: Dependencies.
- Related: LLM migration (Task 5.1 removes `leptess`, `image` and `rand`, and Task 5.3 pins the Docker
  images).
- Location: `Cargo.toml` (`tokio-util` unused in `src/`; `lazy_static` used in
  `src/text_processing.rs:418` and `src/validation.rs:17`; `thiserror = "1.0"` while the lock also
  has 2.0.18; `rand` only used in `src/ocr/retry.rs`; no `rust-version`), `Cargo.lock` (duplicate
  `rand` 0.8/0.9, `thiserror` 1/2, `webpki-roots` 0.26/1.0, `getrandom`, `hashbrown`),
  `deploy/Dockerfile` (`rust:latest`, `ubuntu:latest`), CI `dtolnay/rust-toolchain@stable`.
- Problem: both `native-tls`/OpenSSL (reqwest default features, used by teloxide) and `rustls` with
  both `aws-lc-rs` and `ring` (sqlx, `metrics-exporter-prometheus`) are compiled and shipped. The
  toolchain and base images float, so builds are not reproducible. `std::sync::LazyLock` is already
  used, which needs Rust 1.80 or newer, but this is not declared.
- Proposed fix: remove `tokio-util`. Replace `lazy_static` with `LazyLock`. Bump `thiserror` to 2.
  Use rustls only (`reqwest` with `default-features = false, features = ["rustls-tls", "json"]`, and
  the teloxide `rustls` feature), and drop `libssl3` from the image. Set `rust-version` and add an MSRV
  CI job. Pin the toolchain in `rust-toolchain.toml`. Plan the edition 2024 migration
  (`cargo fix --edition`) after Phase 5 to avoid churn on code that is about to be deleted.
- Validation: `cargo tree -d` shows fewer duplicates, `cargo tree -i openssl-sys` is empty, and the
  MSRV job is green.

### IMP-035: Dead code (unused caches, DB functions, handlers, helpers)
- [ ] Done
- Priority: P3. Effort: M. Area: Code organization.
- Related: LLM migration (Task 3.4 plans to reuse `OcrResultCache`. It is unbounded and never purged
  (IMP-010), so use a bounded cache instead.)
- Location: `src/cache.rs:331` (`OcrResultCache`), `src/cache.rs:413` (`DbQueryCache`),
  `CacheManager.recipe_cache`, `src/db.rs:308-405` (`*_cached` user functions), `src/db.rs`
  `read_recipe`, `update_recipe`, `read_ingredient`, `list_ingredients_by_user`, `search_recipes`,
  `has_duplicate_recipes`, `update_recipe_ingredients` (tests only),
  `src/bot/callbacks/callback_handler.rs:41-178` and `src/bot/message_handler.rs:499-565`
  (non-cached handlers, only re-exported), `src/observability/tracing_mod.rs:97-161`,
  `src/observability/metrics.rs:115-122`, `src/errors.rs:153-168`, `src/bot/callbacks/editing_callbacks.rs.bak`
  (untracked file in `src/`).
- Problem: about 1,000 lines that compile, show up in docs and must be kept consistent, and that
  contain latent bugs (IMP-029, the slicing panic in IMP-017).
- Proposed fix: delete them, or move the few that are still wanted to tests. Remove
  `#[allow(dead_code)]` attributes so the compiler reports the rest.
- Validation: `rg "allow\(dead_code\)" src` only shows justified cases, and the build, clippy and
  tests stay green.

### IMP-036: Oversized modules and duplicated handlers
- [ ] Done
- Priority: P3. Effort: L. Area: Code organization.
- Location: `src/bot/dialogue_manager.rs` (1,658 lines), `src/db.rs` (1,592 lines),
  `src/bot/message_handler.rs:42` (306-line function), `src/bot/callbacks/editing_callbacks.rs:431`
  (351-line function), `src/bot/callbacks/callback_handler.rs:41-178` vs
  `src/bot/callbacks/callback_handler.rs:184-327` (the same routing twice),
  `src/bot/callbacks/callback_handler.rs:580-762` vs `src/bot/callbacks/recipe_callbacks.rs:32-196`
  (the same recipe views twice).
- Problem: every routing change must be made in two places, and the
  "build review message + keyboard" code is copy-pasted about 10 times.
- Proposed fix: split by use case (`bot/flows/{new_recipe,saved_recipe,rename}.rs`, `db/{users,recipes,ingredients}.rs`).
  Add one `render_review(&state) -> (String, InlineKeyboardMarkup)` and one routing function. Do this
  after IMP-001 and IMP-007, and after migration Phase 3, to avoid conflicts.
- Validation: no module over about 500 lines (the migration plan's target), and the clippy
  `too_many_lines` warnings in `src/bot` go away.

### IMP-037: `docs/schema.sql` does not match the live migration
- [ ] Done
- Priority: P3. Effort: S. Area: Docs.
- Location: `docs/schema.sql` (`ocr_entries` table, `ingredients.ocr_entry_id`,
  `ingredients.recipe_name`, duplicated `*_fkey1` constraints) vs `src/db.rs:1300-1349`.
- Problem: readers and reviewers use a schema that no longer exists. The duplicated FKs suggest that
  the production database may differ from migration 1 (see IMP-028).
- Proposed fix: regenerate from a fresh database (`pg_dump --schema-only`) after IMP-028, or delete
  the file and point to `migrations/`. Check the production schema for leftover columns and
  constraints.
- Validation: running `docs/schema.sql` on an empty database and then the app's schema validation
  succeeds.

### IMP-038: `u64 as i64` casts for Telegram ids; curated clippy pedantic set
- [ ] Done
- Priority: P3. Effort: S. Area: Lints.
- Location: about 45 `user.id.0 as i64` / `q.from.id.0 as i64` sites, e.g.
  `src/bot/callbacks/editing_callbacks.rs:158`, `src/bot/command_handlers.rs:34`,
  `src/bot/message_handler.rs:509`. Clippy pedantic+nursery reports 1,463 warnings in total, mostly
  noise.
- Problem: the casts are scattered and unchecked. A few pedantic lints point to real issues in
  scope: `significant_drop_tightening` (`src/cache.rs:504`, `src/cache.rs:609`,
  `src/deduplication.rs:107`), `unused_async` (`src/observability/health_checks.rs:47`,
  `src/observability/health_checks.rs:60`, `src/observability/health_checks.rs:77`),
  `ref_option` (7 handlers take `&Option<String>`), and `redundant_clone` (`src/cache.rs:756`,
  `src/observability/metrics.rs:622`).
- Proposed fix: a `TelegramUserId(i64)` newtype with a checked `From<UserId>` (shared with IMP-024),
  and a curated `[lints.clippy]` table in `Cargo.toml` (the lints above plus `cast_possible_wrap`)
  instead of pedantic wholesale.
- Validation: `rg "\.0 as i64" src` is empty, and the chosen lints are at `deny` in CI.

### IMP-039: Health checks load Tesseract every minute; bot-token check is a no-op
- [x] Done (triage 2026-10-28): no Tesseract; health uses the Telegram heartbeat; readiness needs a Telegram success.
- Priority: P3. Effort: S. Area: Observability.
- Related: LLM migration (Task 3.5 replaces the OCR check. Readiness must not call the paid API.)
- Location: `src/observability/health_checks.rs:47-57` (`LepTess::new` on every readiness call and
  every 60 s, blocking in async), `src/observability/health_checks.rs:60-74` (format check only),
  `src/observability/health_checks.rs:77-122`.
- Problem: each check loads the Tesseract model synchronously on a runtime thread, which uses memory
  and CPU on a 256 MB shared-CPU VM. The "telegram_bot" health metric is always healthy, even with a
  revoked token.
- Proposed fix: after the migration, readiness checks the DB and the configuration. Track Telegram
  health from the result of the polling loop (last successful `getUpdates`), not from the token format.
- Validation: `/health/ready` latency under 50 ms, and revoking the token on staging turns the
  Telegram health metric red.

### IMP-040: Pre-push hook untracked, slow, and missing `cargo fmt --check`
- [x] Done (triage 2026-10-28): versioned `scripts/hooks/pre-push` with fmt, clippy and `cargo test --lib` (`ea8b5f6`).
- Priority: P2. Effort: S. Area: Tooling.
- Related: LLM migration (Task 5.4 versions the hook in `scripts/hooks/`.)
- Location: `.git/hooks/pre-push` (untracked, documented in README).
- Problem: the hook runs the full `cargo test` (needs Tesseract locally, ~3 minutes) plus a
  redundant `cargo test --doc`, but not `cargo fmt --all -- --check`. Commit `6537576` had to fix a
  formatting error that the hook let through and CI would have rejected. New clones get no hook.
- Proposed fix: `scripts/hooks/pre-push` running fmt check, clippy `-D warnings` and
  `cargo test --lib`; install with `git config core.hooksPath scripts/hooks`; the full suite runs in CI.
- Validation: an unformatted change is rejected by `git push` in under a minute.

### IMP-041: Docker image built twice, pushed on PRs, no build cache, unused CI artifact
- [x] Done (triage 2026-10-28): one image per push to main, layer cache, no duplicate workflow (Task 5.4).
- Priority: P2. Effort: M. Area: CI/CD.
- Related: LLM migration (Task 5.4, target pipeline.)
- Location: `.github/workflows/build-and-push-docker.yml`, `.github/workflows/deploy-staging.yml`,
  `.github/workflows/deploy-production.yml`, `.github/workflows/build-test.yml` (upload-artifact step).
- Problem: every push builds the image for ghcr (never used by Fly) and Fly rebuilds it remotely for
  staging, and again for production: three release builds of ~4 minutes each, none cached, and
  production does not run the image that staging tested. `push: true` also runs on pull requests
  (fails for forks). Build and Test uploads a debug binary nobody downloads. No `concurrency` groups
  or `timeout-minutes`.
- Proposed fix: build once with buildx and `cache-from/to: type=gha`, push by commit SHA (only
  outside PRs), deploy that digest to staging then production; drop the artifact upload; add
  concurrency and timeouts.
- Validation: one push produces one image digest, visible in both staging and production releases.

### IMP-042: Deprecated Node 20 actions; smoke tests on Node 18 check health only
- [ ] Done
- Priority: P3. Effort: S. Area: CI/CD.
- Location: all workflows (`actions/checkout@v4`, `actions/setup-node@v4`, `docker/*` actions),
  `.github/workflows/deploy-staging.yml` (`node-version: '18'`), `smoke-tests.js`.
- Problem: every job shows the Node 20 deprecation warning; Node 18 is end of life. The smoke tests
  only call `/health/live` and `/health/ready`, so a bot that cannot reach Telegram or the extraction
  API passes.
- Proposed fix: move to action versions running on Node 24 and Node 22 for the smoke tests; add a
  Telegram `getMe` check and one extraction call on a fixture image.
- Validation: no deprecation annotations; smoke tests fail with a revoked bot token.

### IMP-043: README describes a pipeline that does not exist
- [x] Done (2026-10-27, with migration Task 5.5: README rewritten for the Gemini bot)
- Priority: P3. Effort: S. Area: Docs.
- Location: `README.md:89`, `README.md:122-127`.
- Problem: the README claims automatic production deploys, deployments gated on tests and an
  auto-installed hook. None is true (production is manual and ungated, the hook is local).
- Proposed fix: rewrite the section after Phase 0 of the migration, then again after Task 5.4.
- Validation: README matches `.github/workflows/`.

### IMP-044: Dispatcher panics when Telegram is unreachable or the token is invalid at startup
- [ ] Done
- Priority: P2. Effort: S. Area: Reliability.
- Location: `src/main.rs` (dispatcher start), teloxide 0.17 `dispatching/dispatcher.rs:385`.
- Problem: observed while validating the Docker image (Task 0.4): with an invalid token the polling
  listener logs `Failed to get webhook info: Api(InvalidToken)` and the dispatcher panics
  (`thread 'main' panicked at .../teloxide-0.17.0/src/dispatching/dispatcher.rs:385`). The same
  happens if the Telegram API is unreachable at startup. The process dies with a panic instead of
  a clear error, and with the `always` restart policy (Task 0.5) a revoked token restarts forever.
- Proposed fix: call `bot.get_me()` before building the dispatcher, retry network errors with
  backoff, and exit with a clear error (and an alert) on `InvalidToken`.
- Validation: starting with an invalid token exits with a one-line error and no panic; starting with
  the network down retries and then connects.

### IMP-045: Locale keys used in code are not checked against the `.ftl` files
- [x] Done (2026-10-27, UX Task U1.7): `every_key_used_in_code_exists_in_both_locales` in
  `tests/localization_tests.rs` finds the literal keys of `t_lang`, `t_args_lang` and the
  localized button helpers in `src/` and checks both `.ftl` files.
- Priority: P2. Effort: S. Area: Localization.
- Location: `src/localization.rs:91` (fallback "Missing translation: …"), every `t_lang`/`t_args_lang`
  call site.
- Problem: found by review gate R0: `src/bot/callbacks/editing_callbacks.rs` used the key
  `error-recipe-not-found` (3 call sites, fixed) which exists in neither locale, so users saw
  "Missing translation: error-recipe-not-found". Nothing prevents the next missing key.
- Proposed fix: a test that collects the string literals passed to `t_lang`/`t_args_lang` in `src/`
  and asserts each key exists in both `locales/en/main.ftl` and `locales/fr/main.ftl`.
- Validation: the test fails when a call site uses an unknown key.

### IMP-046: Timing benchmarks run as blocking tests
- [x] Done (2026-10-27): every test of `tests/performance_tests.rs` and the adaptive
  preprocessing timing test are `#[ignore]`d (run with `--ignored`); the OCR timing tests go with
  Phase 5. Moving the text-parser benchmarks to `benches/` (criterion) stays optional.
- Priority: P2. Effort: M. Area: Tests / CI.
- Related: LLM migration (most benchmarks measure OCR code that Phase 5 deletes).
- Location: `tests/performance_tests.rs` (23 tests with ops/sec and overhead thresholds),
  `tests/integration_tests.rs`.
- Problem: since Task 0.8 CI runs all test targets, so wall-clock thresholds gate merges on shared,
  noisy runners. `test_multi_line_vs_single_line_performance` (relative degradation < 5%) failed in
  CI run for `54a06df` at 39.5% and once in the local pre-push hook; it is now `#[ignore]`d.
  `tests/integration_tests.rs` has the same kind of test: `test_adaptive_preprocessing_performance_comparison`
  (high-quality preprocessing < 300 ms) failed the CI runs for `80a6941` and `dcaa6b0` at 307 ms;
  it is now `#[ignore]`d too.
- Proposed fix: move the benchmarks to `benches/` (criterion) or a non-blocking scheduled CI job,
  keep only functional assertions in `tests/`, and delete the OCR benchmarks in Phase 5.
- Validation: `cargo test --all-targets` contains no wall-clock assertions; the benchmark job
  reports numbers without failing the build.

### IMP-047: No per-user limit on paid extractions
- [ ] Done
- Priority: P3. Effort: S. Area: Cost.
- Related: LLM migration (Task 3.4 left it optional).
- Location: `src/extraction/guard.rs`.
- Problem: every photo costs about 0.0017 USD. The concurrency limit and the cache bound the load and
  repeated images, but one user (or a bot) sending thousands of different photos is only limited by
  Telegram's rate limits.
- Proposed fix: a per-user daily counter (in memory, reset at midnight UTC, or in Postgres) checked
  before extraction, with a localized "daily limit reached" message; limit configurable.
- Validation: unit test with a fake extractor: the N+1th photo of the day is refused without a call.

### IMP-048: OCR leftovers kept only for tests
- [ ] Done
- Priority: P3. Effort: S. Area: Observability.
- Related: LLM migration Task 5.1 (Tesseract removed on 2026-10-27).
- Location: `src/observability/metrics.rs` (`record_ocr_metrics`, `record_ocr_performance_metrics`,
  `OcrPerformanceMetricsParams`, `update_circuit_breaker_state` and its `circuit_breaker_state`
  gauge, which the Gemini breaker never moves), `src/observability/tracing_mod.rs` (`ocr_span`),
  `src/cache.rs` (`OcrResultCache`, `OcrCacheKey`, `OcrCacheValue` and the `ocr_cache` field and
  TTL of `CacheManager`); about 60 calls in `tests/observability_tests.rs`,
  `tests/observability_integration_tests.rs` and `tests/performance_tests.rs`.
- Problem: nothing in the bot records `ocr_*` metrics or uses the OCR cache any more; these
  survive only because tests use them as samples. R5 review, 2026-10-27.
- Proposed fix: switch the tests to live metrics and spans (`record_db_metrics`,
  `telegram_span`), delete the helpers and the OCR cache; publish the Gemini breaker state as a
  gauge if a dashboard needs it.
- Also left from OCR (follow-up review of Task 5.2, 2026-10-27): the quantity regex still accepts
  `l` and `O` (`[lO\d]+/\d+` in `text_processing.rs`), so a typed "l/2 cup flour" is refused with
  `edit-invalid-quantity` instead of being read as a name; `is_measurement_line` is only used by
  its doc test.
- Validation: `git grep ocr_ -- src tests` is empty; tests green.

### IMP-049: About 40 locale keys are never used by the code
- [ ] Done
- Priority: P3. Effort: S. Area: Localization.
- Location: `locales/en/main.ftl`, `locales/fr/main.ftl` (e.g. `help-step5`, `help-tip5`,
  `success-extraction`, `caption-used`, `document-image`, `recipe-deleted`, `ingredients-updated`,
  `delete-cancelled`).
- Problem: 47 of 184 keys are never named in `src/` (the 5 `error-ocr-*` keys were removed with
  Task 5.1); some are leftovers of removed flows, some may be read by tests. Translators maintain
  text nobody sees.
- Proposed fix: check each key against `src/` and `tests/`, delete the unused ones, and add the
  reverse check (every key is used) to `tests/localization_tests.rs`.
- Validation: the reverse locale test passes.

### IMP-050: Deploys use the image tag, not its digest
- [ ] Done
- Priority: P3. Effort: S. Area: CI/CD.
- Location: `.github/workflows/deploy-staging.yml`, `deploy-production.yml` (`--image …:sha-<commit>`).
- Problem: re-running the `image` job of "Build and Test" pushes a new digest under the same
  `sha-<commit>` tag, so production could run an image staging never ran. Follow-up review of
  Task 5.4, 2026-10-27.
- Proposed fix: output the digest from the `image` job, record it in the staging run (e.g. in the
  run name or an artifact) and deploy `…@sha256:<digest>` in both workflows.
- Validation: the production run deploys the digest the staging run deployed.

### IMP-051: Observability and HTTP settings of `.env.example` are never read
- [x] Closed as a duplicate of IMP-016 (triage 2026-10-28).
- Priority: P3. Effort: S. Area: Config.
- Location: `src/config.rs` (`config.observability = ObservabilityConfig::default()`),
  `src/main.rs` (HTTP client timeout 30 s, deduplicator 300 s / 10,000 entries),
  `src/observability/tracing_mod.rs` (pretty logs in "development").
- Problem: `ENVIRONMENT`, `OTLP_ENDPOINT`, `ENABLE_TRACE_SAMPLING`, `TRACE_SAMPLING_RATIO`,
  `ENABLE_METRICS_EXPORT`, `SERVICE_VERSION`, `HTTP_CLIENT_TIMEOUT_SECS` and
  `REQUEST_DEDUPLICATION_TTL_SECS` are documented in `.env.example` but have no effect;
  `LOG_FORMAT=json` cannot switch logs to JSON. Found by review gate RU0, 2026-10-27.
- Proposed fix: load `ObservabilityConfig::from_env()` and use the parsed HTTP and deduplication
  settings, or delete the variables from `.env.example`.
- Validation: unit tests of the config loading; `LOG_FORMAT=json` gives JSON logs.

### IMP-052: `update_recipe_ingredients` writes the Telegram id as `ingredients.user_id`
- [x] Done (2026-10-28, same as IMP-029): the function was deleted.
- Priority: P3. Effort: S. Area: Database.
- Location: `src/db.rs` (`update_recipe_ingredients` binds `recipe.telegram_id` as `user_id`).
- Problem: `ingredients.user_id` is the internal `users.id`; the function is not called by the bot
  today, but using it would mix the two ids. Found by review gate RU0, 2026-10-27.
- Proposed fix: delete the function (saved-recipe edits use `update_ingredient`,
  `create_ingredient` and `delete_ingredient`) or bind the user's `users.id`.
- Validation: DB test, or `git grep update_recipe_ingredients` empty.

## Triage of 2026-10-28
Read-only check of every open item against `main` at `1c10336`, after UX phase U1, part of U2 and
migration Phase 5. Ticked above: 004, 005, 006, 009, 012, 029, 030, 033, 039, 040, 041, 051, 052.
- Scheduled in the UX plan: 007 (U2.5; the message-id check and answering the query after a
  failing handler are not planned yet), 010 (U3.4), 024 (U3.3), the search part of 026 (U4.1).
- Partly done: 008 (no French cancel word nor `/annuler`), 011 (public health server, staging
  without metrics token, limiter per proxy peer), 013 (`tests/integration/bot_flow_tests.rs`
  never compiled), 016 (observability and HTTP settings ignored), 023 (a failed save loses the
  review), 025 (English, UTC dates on cards), 034, 042.
- Still open as described: 017–022, 026–028, 031, 032, 035–038, 044, 047–050.
