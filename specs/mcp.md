# MCP server: your recipes in an AI assistant

The bot serves a [Model Context Protocol](https://modelcontextprotocol.io) server, so that an AI
assistant can read your recipes: "what can I cook with leeks?", "show the tart in grams", "what
does 2 cups of flour weigh?". Read only: nothing can be changed or deleted through it. Design:
[tasks/engine-api.md](tasks/engine-api.md); code: `src/doors/mcp.rs` (tools), `src/doors/guard.rs`
(token and limits), `src/engine/` (what the tools call).

## Addresses

| | Address | Token prefix |
|---|---|---|
| Production | `https://just-ingredients.fly.dev/mcp` | `ji_` |
| Staging (test bot) | `https://just-ingredients-staging.fly.dev/mcp` | `ji_stg_` |

A token works only where it was made. Staging runs only while a change is tested: it is stopped at
night and after a release, and a request does not start it.

## 1. Get a token

In Telegram, send `/cle` (or `/key`) to the bot (during the beta, only the Telegram ids of
`API_BETA_USERS` can). Tap "➕ Nouvelle clé": the token is shown once, under a spoiler, then masked
10 minutes later. Copy it at once. `/cle` lists your tokens (by their beginning) and revokes them.

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

The web connectors of claude.ai and ChatGPT need OAuth, which the server does not offer yet
(tranche 4 of the design).

## 3. Tools

| Tool | Arguments | Answer |
|---|---|---|
| `list_recipes` | none | every recipe (id, name, memo), sorted by name, at most 500 |
| `search_recipes` | `query` | recipes found by words of their language, best first |
| `get_recipe` | `id`, `units`?, `lang`? | name, memo, ingredients (measure as saved and as shown), linked recipes |
| `convert_quantity` | `quantity`, `unit`, `ingredient`, `to`, `lang`? | the measure as a card would show it |

`units` / `to`: `recipe` (as read), `weight` (grams), `french` (g, cl, spoons), `us` (cups,
ounces); default the view chosen in the bot (`/unites`). `lang`: `fr` or `en`, the language of unit
names ("cuil. à café" or "tsp"); default yours in Telegram. Names are in the recipes' language,
mostly French, and search does not translate: an assistant lists the recipes or searches in French.
The text read from a photo is never returned.

## Limits and errors

60 requests a minute per token, 4 at once, 10 seconds each, 64 KB per request. HTTP `401`: no
token, a wrong or revoked one (10 failures in a minute from an address block it for the minute:
`429`); `403`: a request for another host; `429` / `503`: too many, or busy, retry later. A tool
that cannot answer (unknown id, bad argument) says why in its result, which the assistant reads.

## Operations

- One log line per request (`Door request`: door, status, duration, token id; never the request's
  content nor the token); counters `ji_api_requests_total{door, status}`,
  `ji_api_duration_seconds{door}`, `ji_mcp_tool_calls_total{tool, result}`.
- Tests: `tests/mcp_tests.rs` (JSON-RPC over a real socket, two users), `tests/doors_tests.rs`
  (the guard), `tests/engine_tests.rs`.
