# JustIngredients Engine: one engine, several doors (study)

Status: tranche 1 in production (v42, 2026-11-06). Tranche 2 (writes and events) started: §15b. Done: task 1 (safety net), task 2 (engine read side), task 3 (tokens and `/cle`), all in production since v41 (2026-11-06, with a spoon hotfix); task 4 (axum and the door guard, 2026-11-06), task 5 (`/mcp`, 2026-11-06). How to connect an assistant: [../mcp.md](../mcp.md). Study revised after an independent review. Asked by the
owner: "something ambitious, a big architecture change, API, events… the engine should not be
only on Telegram but reachable from other tools". The owner chose **AI assistants through MCP
first** and validated the four decisions of the first version: tranches in order, tokens created
with `/cle` and limited to a beta list, the token message neutralised after 10 minutes, the
`fly.dev` address. Section 16 lists what the review changed.

## 1. Goal

The recipe engine (photo reading, ingredients, units, conversions, search, linked recipes, memo)
becomes a service with several entry points: Telegram stays one of them, next to an MCP server
(the Model Context Protocol, spoken by AI assistants such as Claude Code, Cursor, VS Code) and
later a public HTTP API. A change made through one door shows everywhere, through events.

Example, once tranche 1 is out: in an assistant, "what can I cook with leeks?" makes it call
`list_recipes` (all names, in the user's language: "Tarte aux poireaux") or `search_recipes`
with "poireaux", then `get_recipe` with `units = weight`. Search is lexical, in the recipe's
language: "leeks" does not find "poireaux", and the tool descriptions say so.

## 2. Where we start (measured 2026-11-05)

- About 14,000 of the 24,600 lines of `src/` do not depend on Telegram: extraction behind
  `IngredientExtractor` with its guard (concurrency, per-user daily limit, cache, breaker), units,
  conversion, unit views, recipe parts, search, and the database layer, whose user-facing reads and
  writes are scoped by owner (`read_recipe_with_name`, `searchable_recipes`, `rename_recipe`,
  `set_recipe_note`, `apply_ingredient_changes`, `save_recipe_parts`…). Exceptions, used by tests
  only: `create_ingredient` (no owner check), `update_recipe_name` (skips the unique-name rule),
  `create_recipe`.
- The knot is `src/bot/` (about 10,600 lines): use cases are mixed with Telegram rendering. A
  review in progress is a `RecipeDialogueState` holding Telegram message ids; the card is built in
  `bot/cards.rs` and `ui_builder::render_recipe_card` straight from database rows.
- `unit_view::show_measure` returns a localized text and a `converted` flag, not a number;
  `users.language_code` is written at first contact and never updated.
- Identity is the Telegram user id: `recipes.telegram_id → users.telegram_id` (IMP-026).
- HTTP: a hand-written `hyper` 1 server on port 8080 serves `/health/live` and `/health/ready`
  (public through Fly's `http_service`, HTTPS). Its rate limiter allows **10 requests per minute
  per client** on every path but `/health/live`, before routing; `check_request_size` reads only
  `Content-Length`; `/health/live` follows Telegram's reachability and is the `http_service`
  check. Metrics are on the internal port 9091. One machine (`--ha=false`); the bot's database
  pool has 10 connections. The owner watches the Fly bill.
- Safety net, and its limits: 28 handler scenarios check database and dialogue state, **not the
  card's text**; card tests in `ui_builder` build `db::Ingredient` directly; the only HTTP test
  checks 200 and 404.
- Known weakness that matters later: the photo daily limit lives in memory (`GuardedExtractor`,
  UTC days), so a deploy resets it.

## 3. Target architecture

```
                ┌────────────────────────── engine ───────────────────────────┐
  Telegram  ──▶ │ recipes: list · search · read (in a unit view) · convert     │
  MCP /mcp  ──▶ │ writes: rename · memo · delete · edit · create from text     │ ──▶ Postgres
  HTTP /v1  ──▶ │ photos: read → draft → confirm                               │ ──▶ Gemini
                └──────────────────────────────┬───────────────────────────────┘
                                               │ events (outbox, same transaction)
                                               ▼
                               Telegram cards (later: SSE, webhooks)
```

- `src/engine/` holds the use cases. Every call takes an `Owner { user_id, telegram_id }` (so
  that moving recipes to `users.id` later stays inside `db`) and returns domain values or a typed
  `EngineError` (`NotFound`, `Invalid`, `Conflict`, `Forbidden`, `Internal`): "not yours" and "the
  database failed" are no longer the same `anyhow` error.
- Wire types (what MCP and the API return) are defined once, with `serde` and a JSON schema,
  and shared by every door.
- Adapters: `bot` (Telegram), `mcp` (rmcp), later `api` (axum `/v1`). The Telegram card is
  rendered from the engine's `RecipeView`, so the bot and the assistants see the same recipe.
- Rule, checked by a test that reads the sources: `src/engine/` may only use an allowlist of
  modules (`db`, `extraction`, `unit_view`, `units`, `conversion`, `search`, `validation`,
  `recipe_parts`, `errors`) and no `teloxide`, `axum`, `rmcp`, `crate::bot` or
  `crate::observability` (which imports teloxide). A Cargo workspace split is deferred until the
  engine's shape has settled (after tranche 2).
- Same process, same machine, same port 8080: the `hyper` server becomes an `axum` router that
  keeps the health routes exactly as they are and adds `/mcp`. No new Fly machine, no new cost.

## 4. Identity and tokens (tranche 1)

Telegram stays the way in: a user creates a personal token in the bot.

- `/cle` (French) / `/key`: allowed to the Telegram ids of a new setting **`API_BETA_USERS`**
  (not `OWNER_CHAT_ID`: staging has none, and setting it there would start alerts and the weekly
  report from the test bot). The owner sets it on both apps.
- The bot creates `ji_` (`ji_stg_` on staging) + 32 random bytes (base64url), shows it once as
  `<tg-spoiler><code>…</code></tg-spoiler>` (not `protect_content`, which also forbids copying the text: dropped on 2026-11-06 when the owner could not copy a token), with how to
  use it, and lists the active tokens with a 🗑 button each to revoke. At most 5 active tokens per
  user.
- Stored: `api_tokens (id, user_id → users.id ON DELETE CASCADE, token_hash BYTEA UNIQUE (SHA-256),
  hint TEXT (the prefix and 6 characters), scopes TEXT[], created_at, last_used_at, revoked_at)`;
  no label (the bot has no way to type one; the hint and the dates tell tokens apart). The token itself
  is never stored nor logged. Verification checks the format first (prefix, length, alphabet)
  without touching the database, then looks the SHA-256 up (a 256-bit random secret needs no slow
  hash). `last_used_at` is written at most every 5 minutes.
- The message showing a token is recorded in `chat_messages` with a new kind `secret`. A sweeper
  (at startup and every minute) **edits** it, 10 minutes after it was sent, down to "🔑 Clé
  ji_ab12… créée": an edit works at any age, a delete only within 48 hours, and a recorded message
  survives a restart or a deploy. It is not a `transient` message: the end of the next action
  must not delete it before the user copied it.
- How-to given with the token: keep it in an environment variable, e.g. `.mcp.json` with
  `"Authorization": "Bearer ${JI_TOKEN}"` (Claude Code expands variables there; Cursor uses
  `${env:…}`, VS Code an input with `password: true`), rather than on a command line that ends up
  in the shell history.
- Scopes: `recipes:read` in tranche 1; `recipes:write`, `photos:write` later.

## 5. MCP server `/mcp` (tranche 1: read only)

- Transport: `rmcp` (pinned `=3.9.0`: the crate changes fast) Streamable HTTP service, mounted in
  the axum router with `nest_service`. Configuration: `allowed_hosts` from the settings
  (`${FLY_APP_NAME}.fly.dev`; the default accepts only loopback hosts), **stateless**
  (`legacy_session_mode(false)`, `json_response(true)`: no sessions to lose at each deploy, no
  SSE), `max_request_body_bytes` 64 KB.
- Authentication: an axum middleware reads `Authorization: Bearer`, verifies the token, puts the
  `Owner` in the request's extensions and removes the header; the tools read the owner from the
  request parts rmcp passes to them. No token, a bad one or a revoked one: `401` with
  `WWW-Authenticate: Bearer`.
- `list_tools` and `call_tool` are written by hand, not with the `#[tool]` macro (its expansion
  contains a run-time `panic!`, which the project forbids; four tools do not need it).
- Tools, all annotated read-only:

| Tool | Input | Output |
|---|---|---|
| `list_recipes` | none | every named recipe (at most 500): id, name, memo |
| `search_recipes` | `query` | ids and names, best first, as the bot's search ranks them |
| `get_recipe` | `id`, `units?` (`recipe`, `weight`, `french`, `us`; default the user's view), `lang?` | name, memo, ingredients `{stored: {quantity, unit, name}, shown: {text, converted}}`, unit view, linked recipes |
| `convert_quantity` | `quantity`, `unit`, `ingredient`, `to`, `lang?` | `{shown: {text, converted}}`, rounded as on the cards |

- Descriptions say that names are in the user's language (mostly French) and that search is by
  words of that language, so an assistant lists or searches in French. There is no numeric
  converted value yet: `shown.text` is what the card would show. `recipes.content` (the text read
  from the photo, possibly someone else's page) is never returned.
- Each answer has structured content plus a short text an assistant can quote. A recipe of
  another user answers "not found", never "forbidden".
- `lang` defaults to the user's language, which `record_user_seen` now updates from Telegram's
  `language_code` (today it is frozen at first contact).

## 6. HTTP API `/v1` (tranche 1b, after MCP)

The same engine and tokens, for tools that are not assistants: `GET /v1/me`,
`GET /v1/recipes` (all names, or `?q=` for search), `GET /v1/recipes/{id}?units=&lang=`,
`GET /v1/convert`. JSON, errors as `application/problem+json` (RFC 9457), OpenAPI at
`/v1/openapi.json` (`utoipa`). Moved out of tranche 1 by the review: the owner's goal is the
assistants, and the REST surface plus its OpenAPI document is a week on its own. If paging is ever
needed, it is a keyset cursor on (folded name, created_at, id), never an offset.

## 7. HTTP front (tranche 1)

- Limits, separated: the existing 10/min per IP stays on `/health/*` only; `/mcp` gets 60/min
  per token, a per-IP limit counted only on `401` answers (token guessing), and a global
  concurrency cap (4 requests at a time) so assistants can never take the bot's 10 database
  connections. Real body limits (`RequestBodyLimitLayer`), a request timeout (10 s).
- The health routes keep their exact behaviour, written down first as tests against today's
  `hyper` server, then kept green through the swap: `/health/live` not counted, `429` after 10,
  `413` on a large body, `/health/ready` `503` when the database is down, unknown paths
  "Not Found" (404, also for a wrong method, where axum would say 405), the client key from
  `Fly-Client-IP` then the TCP peer (`into_make_service_with_connect_info`). The server still
  starts before the Telegram connection.
- `/health/live` follows Telegram: if Telegram is unreachable for long, Fly may stop routing
  `/mcp` too. Accepted for now (one process whose main job is the bot), to revisit if the API grows.
- Logs: one line per request with door, route **template** (never the raw URI: `q=` is search
  text), status, duration, token prefix. Request parts are never printed. `RUST_LOG` gets
  `rmcp=warn`.

## 8. Events (tranche 2)

- `events (id, user_id, kind, recipe_id, origin, token_id, version, payload JSONB, created_at)`,
  written **in the transaction of the change** (transactional outbox). Events are thin: ids, the
  origin (`telegram`, `mcp`, `api`), what a consumer cannot read again afterwards
  (`recipe.deleted` carries the name and the linked ids); consumers read the current state.
- Ordering: ids are taken at insert but transactions commit in another order, so a reader of
  `id > last_id` could skip an event committed late. Each event insert takes
  `pg_advisory_xact_lock(<constant>)` first, so ids commit in order (nothing at this scale).
- `pg_notify('ji_events', id)` in the same transaction is delivered on commit; a dispatcher
  listens (`sqlx::PgListener`, one dedicated connection) and also polls every 30 s. Each consumer
  keeps its cursor; delivery is at least once, consumers are idempotent.
- Prerequisites in the code: database writes take a transaction (`&mut Transaction`) instead of
  opening their own or none (`set_recipe_note` runs on the pool); every write of the bot (six call
  sites) goes through the engine, or its changes would emit nothing; test-only db functions move to
  the tests.
- First consumer: the Telegram cards. The bot keeps refreshing its own changes synchronously (the
  handler tests check right after a dispatch, and Telegram refuses an edit that changes nothing);
  the consumer handles events whose origin is not `telegram`, e.g. a memo written by an assistant.
- SSE and webhooks: left out until a tool needs them. When one does: webhooks over HTTPS only,
  signed HMAC-SHA256, no redirects, timeouts and a response size cap, and server-side request
  forgery blocked with a DNS resolver that filters the resolved addresses (the address checked is
  the one connected to): 0/8, 10/8, 100.64/10, 127/8, 169.254/16, 172.16/12, 192.168/16,
  fc00::/7 (including Fly's private network `fdaa::/16`, where the metrics port and Postgres
  live), fe80::/10, IPv4-mapped IPv6. SSE keep-alives every 15 s, resumable after a deploy.

## 9. Writes (tranche 2)

MCP (then the API) gets rename, memo, ingredient edits and "create a recipe from text" (the
typed-line parser, no Gemini call), with the bot's rules (unique names, `validate_recipe_note`,
quantity ranges). Delete: MCP clients must treat tool annotations as untrusted, and text read from
a photo can carry instructions, so a delete through MCP is a **soft delete** (`deleted_at`,
restorable for 30 days from the bot) or not offered at all; to decide then.

## 10. Photos outside Telegram (tranche 3)

- A photo sent to `/v1/photos` or an MCP tool answers at once with a job id; `jobs` table, a worker
  takes them with `FOR UPDATE SKIP LOCKED`, calls the same `GuardedExtractor`, stores a **draft**
  (`drafts (id UUID, user_id, ingredients JSONB, expires_at)`), emits `draft.ready`; a draft is
  edited then confirmed.
- The per-user daily limit moves to the database in a new table `user_photo_days (user_id, day,
  calls)` (`usage_days` holds global totals per Paris day, not per user), taken atomically
  (`INSERT … ON CONFLICT DO UPDATE SET calls = calls + 1 WHERE calls < $limit RETURNING calls`),
  with the give-back of calls refused before Gemini kept, and one definition of the day.

## 11. OAuth and identity (tranche 4, maybe earlier)

Bearer tokens work in clients where the user sets a header (Claude Code, Cursor, VS Code). Web
connectors (claude.ai, ChatGPT) and Claude Desktop's remote servers go through the authorization
of the MCP specification: protected resource metadata (RFC 9728), an authorization server with
discovery (RFC 8414 or OpenID), PKCE, refresh, audience-checked tokens (RFC 8707), client
metadata documents fetched by URL (another request-forgery surface), a consent page, and a login,
e.g. Telegram Login (which needs the bot's domain set with BotFather). Until it exists, only
beta users who can edit a client's configuration use the engine; so whether OAuth comes before
events and photos is decided after tranche 1. A domain of our own should be chosen before
anyone outside the beta is invited: it goes into every client's configuration and the OAuth
issuer.

## 12. Security

| Risk | Answer |
|---|---|
| Stolen token | Shown once as a protected spoiler, neutralised after 10 min (persisted), hashed at rest, prefix to recognise it, revocable in one tap, 5 per user, `ji_stg_` on staging. |
| Gemini bill | Tranches 1 and 2 cannot call Gemini. Tranche 3: the persisted per-user limit shared by every door, plus a per-token limit. |
| Another user's data | The owner comes from the token only; engine calls are owner scoped; "not found" for others' ids; tests with two users for every tool. |
| Token guessing, flood | Format checked before the database; per-IP limit on `401`; 60/min per token; concurrency cap; body and time limits. |
| DNS rebinding on `/mcp` | rmcp's `Host` allowlist set to our hostname. |
| Prompt injection (a memo or a name carrying instructions) | Tranche 1 is read only; `recipes.content` never returned; memos ≤ 200 and names ≤ 100 characters; writes later with soft delete; text returned as data. |
| Leaks in logs | Tokens never logged (prefix only), route templates not URIs, request parts never printed, recipe text never at `info`+. |
| Browsers | No CORS (no browser client); added with a web app. |

## 13. Operations

- Same Fly app and machine; `https://just-ingredients.fly.dev/mcp` (staging:
  `just-ingredients-staging.fly.dev`, which must be running: it is stopped at night and after a
  release, and a request does not start it).
- Settings: `API_BETA_USERS` (both apps, set by the owner), the allowed host from `FLY_APP_NAME`.
- Metrics: `ji_api_requests_total{door, route, status}`, `ji_api_duration_seconds`, tokens created
  and revoked; dashboard panels in `monitoring/fly-grafana/`.

## 14. Tests

- Before any refactor: golden tests of the card text for the four unit views in French and
  English (with a memo, linked recipes, a converted and an unconverted line), and the health
  behaviour tests of section 7 against today's server.
- Engine: unit and database tests (throwaway databases), two users everywhere.
- MCP: raw JSON-RPC requests sent to the router in-process (`tower::ServiceExt::oneshot`), as
  rmcp's own tests do (rmcp's client needs reqwest 0.13; the project uses 0.12): initialize, list
  tools, call each; no token, bad, revoked, other user's recipe, `429`, wrong `Host`.
- Tokens: creation by a beta user only, five at most, revocation, the sweeper's edit after
  10 minutes and after a restart.
- By hand on staging: the owner connects Claude Code to the staging `/mcp` and asks about recipes.

## 15. Tranche 1 tasks (about one week)

1. ✅ Safety net first: card golden tests; health behaviour tests. (S) Done: `tests/health_tests.rs`
   (5 tests against today's server), `tests/card_golden_tests.rs` (16 cards in
   `tests/golden/cards/`, read through `cards::card_html`; `JI_UPDATE_GOLDEN=1` writes them).
2. ✅ `src/engine/` read side: `Owner`, `EngineError`, wire types, `list`, `search`, `recipe`,
   `convert`; the card rendered from `RecipeView` (golden tests unchanged); `record_user_seen`
   updates the language; the import allowlist test. (M) Done: `src/engine/{mod,recipes}.rs`;
   `Owner` is opaque (a Telegram id inside); `Lang` from Telegram's code, as `detect_language`;
   `EngineError` has `NotFound`, `Invalid`, `Internal` (`Conflict` and `Forbidden` come with the
   writes that need them); `RecipeView`, `RecipeEntry`, `ShownIngredient`, `Shown` serialize with
   the memo as `memo` and the unit view by name; the 16 golden cards unchanged after the card
   moved to the engine; `tests/architecture_tests.rs` (allowlist, mutation-checked);
   `tests/engine_tests.rs` (two users: list, search, recipe in a view, not found, language).
3. ✅ Migration 11 `api_tokens`; `engine::tokens`; `API_BETA_USERS`; `/cle` with the list and
   revoke buttons; the `secret` message kind and its sweeper. (M) Done: `src/db/api_tokens.rs`
   (the creation locks the user's row, so two at once cannot pass the limit of 5),
   `src/engine/tokens.rs` (`Realm` from `FLY_APP_NAME`: `ji_` or `ji_stg_`, each refused by the
   other; shape checked before the database; `last_used_at` at most every 5 minutes;
   `EngineError::LimitReached`), `config::ApiConfig`, `bot::api_keys` (`ApiAccess` given to the
   message and button handlers, the keys message, the token message and its sweeper).
   Tests: engine (owner only, revoked, realms, limit, secret not stored, use noted), handler
   (beta user creates then revokes, protected spoiler message recorded as `secret`; outside the
   beta: unknown command, forged button creates nothing), database (messages due for masking).
   An independent review found 10 issues; 9 fixed (a production token whose random part starts
   with `stg_` was refused; "➕ Nouvelle clé" during another action deleted its prompt without
   ending it; three ways a token could stay shown for good: send or record failures now revoke
   it, the sweep forgets only on permanent Telegram errors; a partial index for the sweep; the
   secret out of `Debug`; one shared user upsert; a failed `last_used_at` write no longer refuses
   a token; no MCP address until `/mcp` exists). The tenth was wrong (a user row created by a
   token gets the column's default language `'en'`, not NULL).
4. ✅ axum router on 8080 with the health routes unchanged; the limits of section 7. (M) Done:
   `observability::server` serves both servers with axum (the `hyper` code and dependencies are
   gone); every public request but a door's reaches one health handler that keeps the order of
   before (limit per client, size, route, "Not Found"): `tests/health_tests.rs`, written before
   the swap, passes unchanged, 20 runs out of 20. `start_with_doors` mounts the doors next to the
   health routes, outside their limit. `doors::guard` (`Guard`, `Limits`, `guarded`): an address
   with 10 failed authentications in a minute is refused (`429`), a body announced over 64 KB
   (`413`), the token checked (`401` with `WWW-Authenticate: Bearer`, counted as a failure), 60
   requests a minute per token (`429`), 4 at once for all doors with a 2 s wait (`503`), 10 s at
   most (`504`); the token is handed to the door in the request's extensions and its header
   removed; one log line and `ji_api_requests_total{door, status}`,
   `ji_api_duration_seconds{door}` per request, never the path, the body nor the token.
   `tests/doors_tests.rs` (real socket, throwaway database; new CI step; two steps
   mutation-checked). No door is mounted yet: task 5.
5. ✅ `/mcp`: rmcp stateless, auth middleware, four tools, their tests. (M) Done:
   `src/doors/mcp.rs`: rmcp `=3.9.0` without its macros, `StreamableHttpService` with
   `NeverSessionManager`, `legacy_session_mode(false)`, JSON answers, no SSE keep-alive, the
   app's public name as the only allowed host (`allowed_hosts`), 64 KB; behind `doors::guard`,
   mounted by `main` on the public server. `list_tools` / `call_tool` by hand, four read-only
   tools (annotations read-only, not destructive, idempotent, closed world; closed input
   schemas): `list_recipes`, `search_recipes`, `get_recipe` (`units`, `lang`, default the user's
   view and Telegram language: `engine::language_of`), `convert_quantity`. Answers: the engine's
   JSON as structured content and as text; a refusal (unknown id, another user's, bad argument) is
   a tool result with `isError`, a failure of ours an internal error without detail (logged);
   `ji_mcp_tool_calls_total{tool, result}`. The `/cle` token message gives the address.
   `tests/mcp_tests.rs`: handshake, list, each tool, two users, bad calls, no token, another
   host (mutation-checked); new CI step. Client set-up: `specs/mcp.md`.
6. ✅ Metrics, dashboards, specs (`telegram-bot.md`, `deployment.md`, a new `mcp.md` with the client
   set-up). Staging test with Claude Code, then production. (S) Done: the usage dashboard has
   "Assistants IA : requêtes MCP par jour" (by answer) and "outils appelés" panels, and names the
   key actions. Staging test (2026-11-06, the owner): Claude Code connected with a `ji_stg_`
   token, answered "which recipes have leeks?" (6 calls: searches in French and English, then
   the list) and "with flour?" (11 calls); the logs held one `Door request` line per call
   (door, status, 8–41 ms, token id) and nothing of the questions, recipes or token. Two problems
   found on the way, fixed: the token message could not be copied (`protect_content`), and the
   set-up steps must run in a terminal of their own (a `!` command in Claude Code keeps no
   variable). Production: with the next release.

## 15b. Tranche 2 tasks: writes and events (started 2026-11-06)

The owner's choices (2026-11-06): an assistant may **rename** a recipe, **set or clear its memo**
and **create a recipe from text**; no delete and no ingredient edit through MCP in this
tranche (a mistake, or instructions hidden in a text, can destroy nothing). Write access is
**chosen when a key is created** in `/cle` ("lecture seule" or "lecture et écriture"); the keys
made before stay read only.

1. ✅ Outbox. Migration 12: `events (id, user_id → users.id ON DELETE CASCADE, kind, recipe_id,
   origin, token_id, version, payload JSONB, created_at)` and `event_cursors (consumer,
   last_id)`. `db::events`: an event is inserted in the transaction of its change, after
   `pg_advisory_xact_lock` (ids commit in order, §8), with `pg_notify('ji_events', id)`. (M)
   Done (`src/db/events.rs`, sqlx `json` feature); `engine::events` reads them for consumers
   (`after`, `cursor`: a new consumer starts at the last event, not the past; `advance`).
2. Transactional writes. The five recipe writes of the bot (create, rename, memo, ingredient
   edit, delete) get a variant that runs in a given transaction; `engine::writes` runs each
   with its event (`Actor { owner, origin, token_id }`; kinds `recipe.created`,
   `recipe.renamed`, `recipe.memo_changed`, `recipe.ingredients_changed`, `recipe.deleted`,
   thin: ids, and for a delete its name and linked ids). (M) Done: `rename_recipe_in`,
   `set_recipe_note_in`, `apply_ingredient_changes_in` (`IngredientChanges`), `delete_recipe_in`,
   `save_recipe_parts_in` (the pool versions open their own transaction around them);
   `src/engine/writes.rs` (`Actor`, `create`, `rename`, `set_memo`, `edit_ingredients`,
   `delete`; names and memos checked and cleaned as in the bot).
3. ✅ The bot writes through the engine (five call sites), `origin = telegram`; handler and golden
   tests unchanged. (M) Done: the 31 handler scenarios, the 16 golden cards and the
   architecture test pass unchanged; `tests/engine_tests.rs` checks every write's event, in
   order and with its door, none for a refused write, and the consumer's cursor.
4. Events reach Telegram: a consumer (cursor `telegram_cards`) listens and polls every 30 s;
   for an event that does not come from Telegram it refreshes the recipe's recorded cards (and
   the linked ones after a rename). Events older than 30 days are purged. (M)
5. MCP writes. Scope `recipes:write`; `/cle` creates a read-only or a read-write key and shows
   which; write tools are listed and callable only with a read-write key: `rename_recipe`
   (unique names, as in the bot), `set_memo` (cleaned as in the bot; `null` clears),
   `create_recipe` (a name and one ingredient per line, read by the typed-line parser, no
   Gemini call; a refused line is told with its reason). (M)
6. Tests (engine with events, consumer, MCP writes with both scopes), review, docs, staging
   test, production. (S)

## 16. What the independent review changed (2026-11-05)

19 findings, checked against the code and rmcp 3.9.0's source before being applied:

- Blocking, now in the plan: the 10/min limiter would have broken `/mcp` (§7); rmcp's defaults
  refuse non-loopback hosts and keep in-memory sessions lost at each deploy (§5); `/cle` limited
  to `OWNER_CHAT_ID` could not be tested on staging (§4, `API_BETA_USERS`); the token message
  needed a persisted neutralisation, by edit (§4); the card refactor had no test on the card's
  text, and `/v1` with OpenAPI made tranche 1 two weeks (§6, §14); conversions are text, not
  numbers, the language was frozen, and search does not translate (§1, §5); the health swap needs
  its behaviour written as tests first (§7).
- Decided for later tranches: outbox ordering, transactional writes and the bot's writes through
  the engine (§8); thin events with an origin (§8); soft delete for MCP (§9); a new per-user photo
  table (§10); SSRF with a filtering resolver, SSE and webhooks cut until needed (§8); OAuth's real
  size and its place in the order (§11).
- Minor, applied: logging of route templates, token format check and throttled `last_used_at`,
  hand-written tool listing instead of the panicking macro, raw JSON-RPC tests, Fly liveness and
  staging notes.
