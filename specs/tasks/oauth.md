# OAuth for AI assistants (tranche 4 of the engine): study

Status: study (2026-11-11), nothing implemented. The owner chose this tranche after tranche 2.
Goal: the user adds JustIngredients to **claude.ai** (web, desktop, mobile apps) or **ChatGPT** by
its URL, signs in **with Telegram**, approves what the assistant may do, and is done: no
terminal, no key to copy. The personal keys of `/cle` keep working for Claude Code, Cursor, VS
Code. Builds on [engine-api.md](engine-api.md) (§11) and [../mcp.md](../mcp.md).

## 1. Facts this design rests on (checked 2026-11-11)

- **MCP authorization, spec 2026-07-28** (modelcontextprotocol.io/specification/2026-07-28,
  basic/authorization): the MCP server MUST publish protected resource metadata (RFC 9728) with
  `authorization_servers`, MUST answer 401 for a missing or invalid token (with
  `WWW-Authenticate: Bearer resource_metadata="…", scope="…"`), MUST check the token's audience
  (RFC 8707) and MUST NOT pass a token on. The authorization server (AS) MUST implement OAuth 2.1
  with PKCE S256 and publish RFC 8414 metadata whose `issuer` equals the URL it is found at;
  redirect URIs are matched exactly; refresh tokens of public clients MUST be rotated. Client
  registration: Client ID Metadata Documents (**CIMD**) SHOULD, Dynamic Client Registration
  **deprecated**. The `iss` parameter on authorization responses (RFC 9207) SHOULD (MUST in the
  CIMD draft). Consent for a client MUST show who asks and the redirect host, with CSRF
  protection and no framing.
- **Claude** (claude.com/docs/connectors/building/authentication): uses CIMD when the AS metadata
  has `client_id_metadata_document_supported: true` and `"none"` among
  `token_endpoint_auth_methods_supported`; its client is
  `https://claude.ai/oauth/mcp-oauth-client-metadata` (fetched: public client, redirect
  `https://claude.ai/api/mcp/auth_callback`, grants authorization_code and refresh_token), for
  claude.ai, Desktop and the mobile apps; Claude Code's is
  `https://claude.ai/oauth/claude-code-client-metadata` (localhost redirects, any port). Claude
  starts sign-in only on an HTTP 401, uses the first authorization server listed, requires the
  metadata's `resource` to equal the URL the user typed, refreshes on 401 and up to 5 minutes
  before expiry, connects over IPv4 from `160.79.104.0/21`, allows 10 s per AS request.
- **ChatGPT** (developers.openai.com/apps-sdk/build/auth): prefers CIMD; with RFC 9207 support
  (`authorization_response_iss_parameter_supported`, the same exact issuer everywhere, `iss` in
  every authorization response) it uses the stable client `https://chatgpt.com/oauth/client.json`
  (fetched: redirect `https://chatgpt.com/connector_platform_oauth_redirect`, methods `none` or
  `private_key_jwt`); PKCE S256 mandatory; sends the metadata's `resource` as is; compares
  strings exactly.
- **Telegram login** (core.telegram.org/widgets/login, "Log In With Telegram", 2026-03-01): a
  standard OpenID Connect provider, `https://oauth.telegram.org` (discovery fetched: `/auth`,
  `/token`, JWKS; authorization code only, no refresh; client authentication
  `client_secret_basic` or `client_secret_post`; PKCE S256; scopes `openid`, `profile`…; ID tokens
  RS256, ES256, EdDSA or ES256K). The bot's owner registers the allowed redirect URLs in
  BotFather (mini app → bot → Login Widget), which shows a client id and a client secret. The
  server is reachable from the owner's Mac (unlike `api.telegram.org`).
- **rmcp 3.9.0** serves protocol versions 2024-11-05 to 2026-07-28; it has no server-side OAuth:
  the AS and the token checks are ours, in the axum router and `doors::guard`.

## 2. Design

### Roles and addresses

One origin, `https://just-ingredients.fly.dev` (staging: `…-staging.fly.dev`), which is both:
- the **protected resource**: `https://just-ingredients.fly.dev/mcp` (the URL the user enters);
- the **authorization server**, issuer `https://just-ingredients.fly.dev` (no path: no
  insert-versus-append ambiguity of the well-known URLs). We are not an OpenID provider: no
  `openid-configuration` (ChatGPT would then ask for `openid email profile`).

| Path | What |
|---|---|
| `GET /.well-known/oauth-protected-resource/mcp` (and `/.well-known/oauth-protected-resource`) | `{resource, authorization_servers: [issuer], scopes_supported: ["recipes:read", "recipes:write"], bearer_methods_supported: ["header"], resource_name}` |
| `GET /.well-known/oauth-authorization-server` | issuer, endpoints below, `response_types_supported: ["code"]`, `grant_types_supported: ["authorization_code", "refresh_token"]`, `code_challenge_methods_supported: ["S256"]`, `token_endpoint_auth_methods_supported: ["none"]`, `client_id_metadata_document_supported: true`, `authorization_response_iss_parameter_supported: true`, `scopes_supported` |
| `GET /oauth/authorize` | starts a sign-in |
| `GET /oauth/telegram/callback` | Telegram's answer |
| `POST /oauth/consent` | the user's decision |
| `POST /oauth/token` | codes and refresh tokens for tokens (form-urlencoded) |

A request to `/mcp` without a valid token answers **401** with
`WWW-Authenticate: Bearer resource_metadata="https://…/.well-known/oauth-protected-resource/mcp", scope="recipes:read"`
(today: `Bearer` only). These paths go through limits like the doors' (per address).

### Clients: CIMD, allowlisted

Only known clients are accepted: Claude (`https://claude.ai/oauth/mcp-oauth-client-metadata`),
Claude Code (`https://claude.ai/oauth/claude-code-client-metadata`), ChatGPT
(`https://chatgpt.com/oauth/client.json`); the list is a setting. Their documents are fetched over
HTTPS (no redirects, 5 KB at most, 5 s), cached an hour, and checked: `client_id` equals the URL,
the request's `redirect_uri` is one of `redirect_uris` exactly (Claude Code's localhost ones with
any port). An unknown client or redirect gets an error page, never a redirect. No Dynamic Client
Registration (deprecated, and it would let anyone register). The allowlist also removes most of
the request-forgery risk of fetching documents by URL.

### The sign-in, step by step

```
 Claude                  JustIngredients (AS)                 Telegram (oauth.telegram.org)
   │ GET /mcp                  │                                   │
   │──────────────────────────►│ 401 + resource_metadata           │
   │◄──────────────────────────│                                   │
   │ metadata (resource, AS)   │                                   │
   │ GET /oauth/authorize      │ client, redirect, PKCE, resource,  │
   │  ?client_id&redirect_uri  │ scope checked; request stored      │
   │  &code_challenge&state    │ (10 min)                           │
   │  &scope&resource          │                                   │
   │──────────────────────────►│ 302 → /auth?client_id=<bot>        │
   │       (user's browser)    │   &redirect_uri=…/callback         │
   │                           │   &scope=openid profile            │
   │                           │   &state&nonce&code_challenge      │
   │                           │──────────────────────────────────►│ the user logs in
   │                           │ GET /oauth/telegram/callback      │ with Telegram
   │                           │◄──────────────────────────────────│ ?code&state
   │                           │ POST /token (bot's secret) ──────►│
   │                           │◄─────────────── ID token ─────────│
   │                           │ ID token checked (JWKS, iss, aud,  │
   │                           │ exp, nonce): Telegram user id;     │
   │                           │ a beta user? → consent page        │
   │                           │                                   │
   │                 ┌─────────┴─────────┐                         │
   │                 │ 🔑 claude.ai wants │ user approves,          │
   │                 │ to read your       │ write access optional   │
   │                 │ recipes  [Allow]   │                         │
   │                 └─────────┬─────────┘                         │
   │ 302 redirect_uri?code&state&iss                               │
   │◄──────────────────────────│                                   │
   │ POST /oauth/token (code + code_verifier)                      │
   │──────────────────────────►│ access token (1 h) + refresh token │
   │◄──────────────────────────│                                   │
   │ GET /mcp, Bearer jio_…    │ guard: hash, expiry, audience,     │
   │──────────────────────────►│ scopes → the engine                │
```

- **Telegram side**: our own PKCE, `state` (the stored request's id) and `nonce` with Telegram;
  the code is exchanged with the bot's login secret; the ID token is checked against Telegram's
  keys (cached), `iss`, `aud` (the bot's id), `exp` and our nonce. The user is Telegram's `id`
  claim (`profile` scope), i.e. `users.telegram_id`. During the beta only `API_BETA_USERS` get
  further; anyone else sees "pas encore ouvert".
- **Consent page**: server-rendered HTML, no script, in the user's language. It names the
  client by the **host of its client_id** ("claude.ai", not the self-declared name) and the
  redirect host, the Telegram account, and what is asked: reading always; **writing only if the
  user ticks it** (the same choice as `/cle`). CSRF: a random value in the form and in a
  `__Host-` cookie bound to the request; `frame-ancestors 'none'`. Consent is asked every time
  (nothing remembered: simpler and safer).
- **Our authorization code**: random, single use, 60 seconds, bound to the client, redirect URI,
  PKCE challenge, resource, granted scopes and user. The redirect carries `code`, the client's
  `state` and `iss`.

### Tokens

- `/oauth/token`, `authorization_code`: the code, client, redirect URI and `code_verifier` (S256)
  must match, and `resource` if sent; answers an **access token** `jio_…` (1 hour) and a
  **refresh token** `jir_…` (30 days). `refresh_token`: answers a new pair and revokes the old
  refresh token; a revoked refresh token presented again revokes the whole grant (a sign it was
  stolen). Errors as OAuth says (`invalid_grant`, `invalid_client`, `invalid_request`,
  `unsupported_grant_type`).
- Tokens are opaque random values stored by SHA-256, as the `/cle` keys; each belongs to a
  **grant** (user, client, scopes, resource). The guard accepts `ji_` keys as today and `jio_`
  access tokens: unexpired, audience equal to our resource, scopes as granted; write tools still
  need `recipes:write`.
- `/cle` lists the connected apps ("🔗 claude.ai, lecture et écriture, connecté le 12 nov.") with
  a revoke button (the grant and its tokens).

### Storage (migration 13)

`oauth_grants (id, user_id, client_id, scopes, resource, created_at, last_used_at, revoked_at)`,
`oauth_tokens (id, grant_id, kind, token_hash UNIQUE, expires_at, revoked_at)`,
`oauth_requests (id, client_id, redirect_uri, code_challenge, state, scope, resource,
telegram_nonce, telegram_verifier, telegram_id, csrf, expires_at)`,
`oauth_codes (code_hash, request…, expires_at, used_at)`. Expired rows purged daily.

### Settings and the owner's part

- `TELEGRAM_LOGIN_SECRET` (each bot's client secret from BotFather), set by the owner with
  `fly secrets set` on staging and production. The client id is the bot's id (the part of the bot
  token before `:`).
- In BotFather, for the test bot and the real one: Login Widget → allowed URL
  `https://<app>.fly.dev/oauth/telegram/callback`.
- `OAUTH_CLIENTS` (the allowlist), with the three clients above by default.

## 3. Security

| Risk | Answer |
|---|---|
| A stranger's client | Allowlisted client ids only; exact redirect URIs from the client's own document. |
| Confused deputy (Telegram remembers the bot and logs in silently) | Our consent page every time, naming the client host and the redirect host. |
| CSRF, clickjacking on consent | Random token in form and `__Host-` cookie, `frame-ancestors 'none'`, `SameSite=Lax`. |
| Code interception | PKCE S256 required, codes single use and 60 s. |
| Stolen tokens | Access 1 h; refresh rotated, reuse revokes the grant; revocable in `/cle`; hashed at rest; never logged. |
| Token for another server | Audience checked (`resource` bound at issue). |
| Forged Telegram identity | ID token signature (Telegram's JWKS), `iss`, `aud`, `exp`, `nonce`; code exchanged server side with the secret. |
| Fetching client documents | Allowlisted URLs only, no redirects, size and time limits. |
| Open redirect | Errors before the client is validated are shown, never redirected. |

## 4. Not known yet (to learn on staging)

How Telegram's login behaves on a phone inside Claude's sign-in browser (handoff to the Telegram
app?); ChatGPT's plan requirements for custom connectors; whether claude.ai needs anything not
documented. The staging test covers the first on the owner's phone.

## 5. Tasks (about 1.5 weeks)

1. Metadata documents and the 401 header; the paths and their limits. (S)
2. Migration 13; `engine::oauth` (requests, codes, grants, tokens, rotation, purge). (M)
3. Client registry: allowlist, document fetch and checks, cache. (S)
4. `/oauth/authorize` and the Telegram login (OIDC client: code exchange, ID token check with a
   JWT library such as `jsonwebtoken`, JWKS cache). (M)
5. Consent page and `/oauth/consent`; our codes. (M)
6. `/oauth/token` (code, refresh with rotation and reuse detection). (M)
7. The guard accepts `jio_` tokens; `/cle` lists and revokes connected apps. (S)
8. Tests (the whole flow with a fake Telegram and a fake client; each refusal), review, docs
   (`mcp.md`), the owner's BotFather and secret set-up, staging test on computer and phone,
   production. (M)

## 6. Decisions for the owner

1. Sign in with Telegram's standard login ("Log In With Telegram", recommended), rather than a
   link that makes the bot ask "is it you?".
2. Accepted assistants: claude.ai, Claude Code and ChatGPT (recommended), others on request.
3. Consent: reading always, writing only when ticked (recommended), as in `/cle`.
4. Stay on `just-ingredients.fly.dev` (recommended for the beta: a domain change later means
   reconnecting the assistants).
5. Tokens: 1 hour, renewed by the assistant for 30 days without asking; revocable in `/cle`.
6. Your part: BotFather set-up for both bots and one secret per app (`fly secrets set`), with
   step-by-step instructions when we get there.
