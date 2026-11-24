# Recipe photos through an assistant (tranche 3 of the engine): study

Status: **in production (v45, `f28f884`, 2026-11-24)**: tasks 1 to 4 and 6 done and reviewed (§8); task 5 (ChatGPT's file input) waits for a trial with ChatGPT. Tested on staging end to end (2026-11-24) by `~/.claude/ji-scripts/photo_e2e.sh`: `protocol` (the tools, the real test photo posted, staging's Gemini, the draft saved once) and `claude` (a headless Claude Code session asked to import a photo gave the link, waited, then read the draft and saved it), both with a read-write staging key in `~/.claude/ji-scripts/staging-key`. Validated by the owner (2026-11-19, §6 and §7).
itself and created the recipe with `create_recipe` ("la transcription est bonne mais nous devons
garder la main avec Gemini").

Goal: the user gives a recipe photo to their assistant (claude.ai, ChatGPT, Claude Code…) and
JustIngredients reads it **with its own Gemini pipeline** (the bot's prompt, units, spoon check,
recipes in several parts, cache and limits), not with the assistant's own reading; the result is
a draft the user confirms; the recipes are then saved, linked, and appear in Telegram. Builds on
[engine-api.md](engine-api.md) (§10) and [oauth.md](oauth.md).

## 1. Facts this design rests on (checked 2026-11-19)

- **An assistant cannot hand a photo's bytes to a tool.** A tool's arguments are JSON the model
  writes; a model cannot reproduce the bytes of an image it was shown. So a photo reaches a server
  only through something the client does: a file it passes, or a page the user opens.
- **ChatGPT** (developers.openai.com/apps-sdk/reference): a tool lists its file inputs in
  `_meta["openai/fileParams"]`; at run time ChatGPT passes `{download_url, file_id, mime_type,
  file_name}` ("always includes `download_url` and `file_id`"), a temporary URL the server
  downloads. Not documented: size limits, the URL's lifetime, timeouts.
- **claude.ai** (claude.com/docs/connectors/building): tools, prompts, resources, text and image
  results; **no file inputs** to tools; not supported: sampling, resource subscriptions, "advanced
  or draft capabilities" (URL elicitation is not listed); **a tool call may last 240 s**; results up
  to ~150,000 characters. **MCP Apps** (an HTML widget in the conversation) are supported, for
  custom connectors too (app tools need a remote server).
- **MCP URL-mode elicitation** (spec 2025-11-25 and 2026-07-28): a server may ask the client to
  open a URL for the user; the server must check that whoever opens it is the user who asked. Not
  listed by claude.ai; ChatGPT unknown.
- **Today**: Claude reads a photo itself and calls `create_recipe` with typed lines (the owner,
  2026-11-19: right). It bypasses our reading, costs us nothing, but loses parts, the spoon check
  against the printed text, and our rules.
- **The bot's pipeline** (`src/extraction/guard.rs`): `GuardedExtractor` around Gemini: a
  concurrency limit, a cache by SHA-256 of the image (the same photo is never paid twice), a
  per-user daily limit (`EXTRACTION_DAILY_LIMIT_PER_USER`, 50) **kept in memory** (a deploy resets
  it). The bot then shows a review the user edits before saving.

## 2. Design

### The engine: read a photo into a draft, then save it

- `engine::photos::read(owner, image) -> Draft`: the same `GuardedExtractor` the bot uses, then
  the same mapping (units, spoons, parts). The draft is stored (migration 15, `photo_drafts`: a
  random id, the user, the parts and their ingredients as read, the recipe's language, created,
  expires after 24 h, saved). The photo itself is never kept, as in the bot.
- `engine::photos::save(owner, draft_id)` creates the recipes through `writes::create` (one per
  part, linked, each with its `recipe.created` event: the bot refreshes nothing, the list shows
  them); a draft is saved once. `discard` drops it.
- **The daily limit moves to the database** (`user_photo_days (user_id, day, calls)`, taken
  atomically, a call refused by Gemini given back), **shared by Telegram and the assistants**:
  the same 50 a day, no reset at deploys. One definition of the day (Paris).

### Three ways for the photo to arrive

1. **An upload page, for every assistant (first)**. The tool `import_photo` answers with a link,
   `https://just-ingredients.fly.dev/photo/<random>`, valid 15 minutes, for one photo, tied to the
   key or connection that asked. The user opens it (on a phone: camera or library), picks the
   photo; the page sends it, Gemini reads it (5 to 20 s, the page waits and says so), and the page
   shows "Lu : Tarte aux poireaux, 8 ingrédients. Retournez dans votre assistant." The assistant
   then calls `get_photo_draft` and shows the ingredients; the user says OK; it calls
   `save_photo_draft`.
   - The link lets its holder add a photo to the asker's account, nothing else (not read, not
     change): 15 minutes, one photo, 10 MB, JPEG, PNG, WebP or HEIC (checked by their bytes),
     within the daily limit. The page: no script, a plain form, never framed nor cached, no
     referrer.
2. **ChatGPT's file input**. `import_photo` declares its `photo` argument a file input; when
   ChatGPT passes a photo attached to the conversation, the server downloads it (HTTPS only, a
   public address only: no request to our own network; 10 MB, 15 s; image types by their bytes),
   reads it during the call and answers with the draft. Without a file, the same tool answers with
   the upload link (way 1).
3. **An MCP App (later, if wanted)**: a small widget in the conversation with a "Choisir une
   photo" button, for claude.ai and ChatGPT: the photo never leaves the chat. More work (an HTML
   and JavaScript widget served by us), the best experience; not in this tranche.

The server's instructions and the tools' descriptions tell assistants **not to read recipe photos
themselves**: give them to `import_photo`. `create_recipe` stays for recipes the user types or
dictates.

### Tools (read-write keys and connections only: a reading is paid and leads to a save)

| Tool | Arguments | Answer |
|---|---|---|
| `import_photo` | `photo`? (ChatGPT's file) | the draft (way 2), or the upload link and its id (way 1) |
| `get_photo_draft` | `id` | waiting for the photo, being read, or the draft: names, ingredients as the card would show them, warnings (⚠️ lines the bot would flag) |
| `save_photo_draft` | `id` | the recipes created (ids, names, linked) |
| `discard_photo_draft` | `id` | done |

Corrections: the assistant saves what was read, then fixes a name or memo with the existing tools;
ingredient edits stay in the bot (the owner's choice of tranche 2). A badly read photo is
discarded and taken again.

### Limits

- The ordinary tools keep the guard's 10 s; the photo tools get 60 s (claude.ai allows 240). The
  upload page has its own route, 10 MB bodies (the MCP door keeps 64 KB).
- Gemini's concurrency is the bot's (`GuardedExtractor`): a photo from an assistant waits like
  one from Telegram (30 s at most, then "busy, try again").
- No jobs table nor worker (engine-api.md §10 planned one): the reading happens in the page's
  request (way 1) or in the tool call (way 2), both short enough; a restart during a reading loses
  that one reading, which the user retries.

## 3. Security and cost

| Risk | Answer |
|---|---|
| The Gemini bill | Paid readings only for write access, the shared daily limit in the database, the cache (the same photo is free), 10 MB, one photo per link. |
| A forwarded upload link | It only adds a photo to the asker's account; 15 minutes, used once. |
| Request forgery through ChatGPT's file URL | HTTPS, public addresses only (resolved and checked before connecting), no redirects, size and time limits. |
| A photo carrying instructions ("ignore…") | Read by Gemini into ingredients only; the draft holds names and lines, which the existing validation checks; nothing in a photo is ever executed. |
| Someone else's photo | Never stored; the draft expires in 24 h. |

## 4. Not known yet (to learn on staging)

The lifetime of ChatGPT's download URL and its host; whether ChatGPT passes a photo attached in
the conversation to a file input; how the upload page feels on the owner's phone from claude.ai.

## 5. Tasks (about a week)

1. The daily limit in the database, shared with the bot (migration 15 `user_photo_days`). (S)
2. `engine::photos`: read into a draft, save, discard (migration 15 `photo_drafts`,
   `photo_uploads`). (M)
3. The upload page (`/photo/<id>`, multipart, phone-friendly, the reading's result). (M)
4. The four MCP tools, the server's instructions ("don't read photos yourself"). (S)
5. ChatGPT's file input: the safe download. (S)
6. Tests (a fake extractor, every refusal), review, docs, staging test from claude.ai on the
   phone, production. (M)

## 6. Decisions for the owner

1. The upload page first (every assistant, claude.ai included), ChatGPT's file input in the same
   tranche, the in-chat widget later (recommended).
2. A draft confirmed before saving, as the bot's review (recommended), rather than saving at once.
3. One daily limit for Telegram and the assistants together, kept in the database: 50 photos a
   day per user, as today (recommended).
4. Photo tools only for keys and connections that may change recipes (recommended).
5. The photo is never kept (as in the bot).

## 7. What the review changed (2026-11-19), and two more decisions

An independent review checked the study against the code. The premise, the cache and limit, the
shared extractor and the events hold. Changes:

- **A draft keeps what the bot's review keeps**: the lines as `arrange(to_measurement_matches(..))`
  gives them (with their sections, as `MeasurementMatch` JSON, the dialogue's format), the
  transcription (`writes::create`'s `content`: it decides "tbsp" or "cuil. à soupe" on the card,
  and the search), the title. Saving runs the bot's own steps (`split_parts`, `part_names`,
  `to_new_ingredient`, one `create` with every part, which links them), moved into the engine and
  shared with the bot.
- **The bot's review rules hold**: no save while a line is flagged ⚠️
  (`requires_quantity_confirmation`, IMP-004) and no save without a name. **The owner's decision**:
  the assistant asks the user and sends the answer; `save_photo_draft(id, name?, lines?)`, where
  `lines` replaces **flagged lines only**, each given as a typed line ("2 cuil. à soupe d'huile",
  read by the typed-line parser). Without them, the save is refused with the flagged lines and
  their numbers. Other ingredient edits stay in the bot.
- **The reading runs on its own, in the background**: claimed from the page's form, it is spawned
  under a photo limit (2 at once, beyond: "busy, try again", the link still usable), whatever
  becomes of the page's request (a phone switching back to the assistant). The import goes
  `waiting` → `reading` → `ready` or `failed(reason)`; `reading_started_at` turns a reading
  interrupted by a restart into "send it again". The form answers 303 to `/photo/{id}`, which
  refreshes itself while reading (`<meta http-equiv="refresh">`, no script). `get_photo_draft` waits
  up to 20 s for a change, then answers where it is. No per-tool timeout is needed: the door's 10 s
  hold for every tool.
- **ChatGPT's file input only for ChatGPT, after a staging trial** (**the owner's decision**: the
  upload page first). A `download_url` is input any write key could set: the file parameter is
  listed only when the connection's client is ChatGPT's, its host pinned once learned. Before that
  (task 5), a trial connects ChatGPT on staging and logs the file object's shape and host.
- **The link**: its secret hashed, apart from the import's id; claimed atomically (a double tap);
  used up only by a reading that gives a result (too large, busy or unavailable keep it); a GET
  never uses it (link previews, an assistant fetching it); at the post, the key or connection must
  still be active and allowed to write (the page is outside the guard). No CSRF issue: the secret is
  in the path, there is no cookie.
- **The page**: `form-action 'self'` (the sign-in pages say `'none'`); axum's `multipart` feature,
  the body limit raised on this route only (a little over 10 MB), the link checked before the
  body is read, the body read in chunks with a cap; its own per-address limit and in-flight cap;
  the route written `/photo/{id}` (axum 0.8).
- **HEIC is not read** (`ImageMime`: JPEG, PNG, WebP; other formats are converted, HEIC refused):
  the page asks for `accept="image/*"`, for which iOS sends JPEG. A phone's original photo is far
  bigger than Telegram's: 10 MB at most, 2 readings at once.
- **The daily limit** (task 1, done): kept inside `GuardedExtractor`, after both cache checks (a
  cache hit is free), a reading from a door always names its user; one statement; a call refused
  before Gemini given back; a count that cannot be read refuses the call (the bill first). **The
  day becomes Paris's** (it was UTC): the docs and the limit's message say "today".
- **Save atomically**: the import is claimed (its owner, not saved, not expired) inside the
  transaction that creates the recipes, after the events lock; the saved ids are kept, so a retried
  save answers the same recipes.
- **Tools**: drafts looked up by (id, owner); the write gate derived from the write tools' list
  (tested: every write tool refused to a read-only key); `get_photo_draft` never returns the
  transcription nor a line's source text, says what is data (text read from a photo may carry
  instructions), and every failure (not a recipe, no ingredients, daily limit, unreadable, busy,
  expired). `import_photo` says: give the link, then wait for the user to say it is sent (no
  polling); "do not read recipe photos yourself" goes in `import_photo`'s and `create_recipe`'s
  descriptions (whether claude.ai shows server instructions is not documented). Counted per door.
- **Purge** always runs (the OAuth purge only runs with OAuth): imports, drafts and past days.
- The consent page and `/cle` say that writing now includes photo readings (paid).
- engine-api.md planned a `photos:write` scope: write access covers it (the owner's decision 4).

Tasks, revised: 1 done; 2 `engine::photos` imports and drafts (migration 16), the save shared with
the bot; 3 the upload page; 4 the tools; 5 ChatGPT, after its trial; 6 tests, review, docs,
staging, production.

## 8. What the code review changed (2026-11-19)

An independent review of tasks 1 to 4 found no confirmed security or data bug (checked: claims
and the single save, the atomic limit, the link's secret, the page's headers, no text read
returned to the assistant, the write gate, parity with the bot). Fixed, with tests:

- **One reading at once from the pages** (it was two): a reading holds ~37 MB (the photo, its
  base64, Gemini's request) on the 256 MB machine.
- **The upload has 2 minutes** (a stalled phone held a place); the link is claimed, then a
  reading slot taken; without one the link is given back ("another photo is being read").
- **Readings cut by a restart** are sent again at once: at startup, `reading` imports go back to
  waiting, told "interrupted" (one machine); the stale time becomes 15 minutes, far above a
  reading's longest, so a live one is never read twice.
- **A draft whose lines cannot be read back** is never saved as an empty recipe (unavailable).
- **The pages shown** are limited per address (60 a minute), and "busy" reloads itself while a
  reading's page waits.
- **The text read leaves the import** once saved (the recipes hold it), and at expiry; the purge's
  steps run whatever the others did.
- **A 429 from Gemini** (never billed) is given back to the day's count.
- Small: `lines: null` accepted; a range ("2-3") said to stay to confirm; a browser's first
  language read without its weight.

Left for later: `import_photo` could refuse at once when the day's photos are used up (today the
link takes the photo and says so); a page answered before its body is read may show "connection
reset" in some browsers (413, busy).
