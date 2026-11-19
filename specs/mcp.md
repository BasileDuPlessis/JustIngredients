# MCP server: your recipes in an AI assistant

The bot serves a [Model Context Protocol](https://modelcontextprotocol.io) server, so that an AI
assistant can read your recipes: "what can I cook with leeks?", "show the tart in grams", "what
does 2 cups of flour weigh?". With a read-write key it can also rename a recipe, write its memo
and create a recipe from text; nothing can be deleted, nor an ingredient changed. claude.ai and
ChatGPT connect without a key: you sign in with Telegram and approve. Design:
[tasks/engine-api.md](tasks/engine-api.md) and [tasks/oauth.md](tasks/oauth.md); code:
`src/doors/mcp.rs` (tools), `src/doors/guard.rs` (tokens and limits), `src/doors/oauth/` (the
sign-in), `src/engine/` (what the tools call).

## Addresses

| | Address | Key prefix | OAuth tokens |
|---|---|---|---|
| Production | `https://just-ingredients.fly.dev/mcp` | `ji_` | `jio_`, `jir_` |
| Staging (test bot) | `https://just-ingredients-staging.fly.dev/mcp` | `ji_stg_` | `jio_stg_`, `jir_stg_` |

A key or token works only where it was made. Staging runs only while a change is tested: it is stopped at
night and after a release, and a request does not start it.

## claude.ai and ChatGPT: sign in with Telegram

No key to copy. Add the address above as a custom connector, then sign in:

- **claude.ai** (web, desktop, phone): Settings → Connectors → *Add custom connector*, the address;
  *Authentication*: "Se connecter maintenant" (sign in now); *OAuth client*: "Utiliser l'identité
  publiée de Claude" (Claude's published identity), not your own client.
- **ChatGPT**: chatgpt.com/plugins → *Add* → *Add custom MCP server*, the address, OAuth (it may
  ask for its desktop app).

The assistant opens Telegram's sign-in, then our approval page: it names your Telegram account
and the assistant, reading always, changes (rename, memos, new recipes, never delete) only if you
tick them. "Autoriser" takes you back to the assistant, and the bot tells you "🔗 claude.ai est
maintenant connecté…" with a "🔌 Déconnecter" button. `/cle` lists the connected assistants and
disconnects them. The assistant renews its access by itself (an hour at a time, for 30 days
without use); during the beta, only the Telegram ids of `API_BETA_USERS` get through.

Claude Code, Cursor and VS Code use a personal key instead (below).

## 1. Get a key (Claude Code, Cursor, VS Code)

In Telegram, send `/cle` (or `/key`) to the bot (during the beta, only the Telegram ids of
`API_BETA_USERS` can). Tap "➕ Clé lecture seule" or "➕ Clé lecture et écriture": the token is shown once, under a
spoiler (tap it to reveal it, tap it again to copy it), then masked 10 minutes later. Copy it at
once, best in Telegram on the computer the assistant runs on. `/cle` lists your tokens (by their
beginning, with their access) and revokes them.

Keep it in an environment variable rather than in a file or a command line that is kept:

```sh
read -s JI_TOKEN && export JI_TOKEN   # paste the token, then Enter (nothing is shown)
```

## 2. Connect an assistant

**Claude Code**, in a project's `.mcp.json` (Claude Code reads `${JI_TOKEN}` from the
environment when it starts):

```json
{
  "mcpServers": {
    "justingredients": {
      "type": "http",
      "url": "https://just-ingredients.fly.dev/mcp",
      "headers": { "Authorization": "Bearer ${JI_TOKEN}" }
    }
  }
}
```

or for every project: `claude mcp add --transport http --scope user justingredients
https://just-ingredients.fly.dev/mcp --header "Authorization: Bearer $JI_TOKEN"` (the shell puts
the token in Claude Code's own settings file). Check with `/mcp` inside Claude Code.

**Cursor** (`.cursor/mcp.json`) and **VS Code** (`.vscode/mcp.json`) take the same `url` and
`headers` (Cursor reads variables as `${env:JI_TOKEN}`; VS Code as an input with
`"password": true`).


## 3. Tools

| Tool | Arguments | Answer |
|---|---|---|
| `list_recipes` | none | every recipe (id, name, memo), sorted by name, at most 500 |
| `search_recipes` | `query` | recipes found by words of their language, best first |
| `get_recipe` | `id`, `units`?, `lang`? | name, memo, ingredients (measure as saved and as shown), linked recipes |
| `convert_quantity` | `quantity`, `unit`, `ingredient`, `to`, `lang`? | the measure as a card would show it |
| `rename_recipe` (read-write key) | `id`, `name` | the name stored, made unique ("Tarte (2)") |
| `set_memo` (read-write key) | `id`, `memo` (`null` clears) | the memo stored, cleaned as in the bot |
| `create_recipe` (read-write key) | `name`, `ingredients` (one line each) | the new recipe's id and name |
| `import_photo` (read-write) | none | an upload link for the user (15 minutes) and the import's id |
| `get_photo_draft` (read-write) | `id` | waiting, being read, the draft (title, parts, numbered lines, those to confirm), failed, saved, expired |
| `save_photo_draft` (read-write) | `id`, `name`?, `lines`? (`[{line, text}]`, flagged lines only) | the recipes created (linked, one per part) |
| `discard_photo_draft` (read-write) | `id` | done |

A read-only key or connection is shown the first four only and cannot call the others.

**Recipe photos** are read by JustIngredients itself, with the bot's rules (parts, units, the
spoon check, the cache, the daily limit shared with Telegram), never by the assistant: say "import
this recipe photo" and the assistant calls `import_photo`. It gives a link: open it on the phone
(camera or library) or the computer, send the photo (JPEG, PNG, WebP, 10 MB at most; it is not
kept). The page reads it (up to a minute), then says "Photo lue: retournez dans votre
assistant". The assistant shows the ingredients; a line marked to confirm (an unsure quantity,
"2-3 c. à soupe") needs your answer, and a photo without a title needs a name. Once you agree,
the recipes are saved, linked, and appear in the bot's list. Each reading counts in your photos of
the day (50). ChatGPT's own file input is not used yet (to try first, specs/tasks/mcp-photos.md). A change made through
an assistant updates the recipe's cards in Telegram within seconds (the bot follows the engine's
events).

`units` / `to`: `recipe` (as read), `weight` (grams), `french` (g, cl, spoons), `us` (cups,
ounces); default the view chosen in the bot (`/unites`). `lang`: `fr` or `en`, the language of unit
names ("cuil. à café" or "tsp"); default yours in Telegram. Names are in the recipes' language,
mostly French, and search does not translate: an assistant lists the recipes or searches in French.
The text read from a photo is never returned.

## Limits and errors

60 requests a minute per key or connection, 4 at once, 10 seconds each, 64 KB per request. HTTP
`401`: no token, a wrong, expired or revoked one (10 wrong keys in a minute from an address block
it for the minute: `429`; OAuth tokens never do, an assistant's users sharing its addresses);
`403`: a request for another host; `429` / `503`: too many, or busy, retry later. A tool
that cannot answer (unknown id, bad argument) says why in its result, which the assistant reads.

## Operations

- Every change is an event (`events`, origin `mcp`, the key's or the grant's id), kept 30 days;
  a new connection too (`oauth.connected`).
- Photo imports (`photo_imports`: the link's secret by SHA-256, the draft, the saved ids) are
  deleted a day after they expire; the daily counts (`user_photo_days`, Paris days) after a week.
  At most 2 photos read at once from the pages (Gemini's slots are shared with Telegram), 20
  posts a minute per address. Logs: `Photo import started`, `received`, `read`, never the link.
- One log line per request (`Door request`: door, status, duration, `key:N` or `grant:N`; never
  the request's content nor the token); the sign-in logs its steps (`OAuth sign-in started`,
  `Telegram answered`, `approved`, `grant made`), never a code, state or token; counters `ji_api_requests_total{door, status}`,
  `ji_api_duration_seconds{door}`, `ji_mcp_tool_calls_total{tool, result}`.
- OAuth needs `TELEGRAM_LOGIN_CLIENT_ID` and `TELEGRAM_LOGIN_SECRET` (BotFather's mini app → the
  bot → "OAuth2 login": Redirect URL `https://<app>.fly.dev/oauth/telegram/callback`, Trusted
  origin `https://<app>.fly.dev`); without them the door takes keys only.
- Tests: `tests/mcp_tests.rs` (JSON-RPC over a real socket, two users), `tests/doors_tests.rs`
  (the guard), `tests/oauth_tests.rs` (a whole sign-in with a fake Telegram, every refusal),
  `tests/photo_tests.rs` (the upload page with a fake Gemini), `tests/engine_tests.rs`.
