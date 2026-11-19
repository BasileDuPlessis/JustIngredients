# Recipe photos through an assistant (tranche 3 of the engine): study

Status: validated by the owner (2026-11-19: the four recommendations of §6), under review; nothing implemented. The owner asked for it after Claude read a photo
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
