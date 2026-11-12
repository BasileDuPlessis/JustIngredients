-- OAuth for AI assistants (specs/tasks/oauth.md): claude.ai or ChatGPT act on a user's recipes
-- once the user signed in with Telegram and approved. Secrets (codes, tokens, the browser's
-- binding, the consent form's token) are stored by SHA-256 only.

-- A user let one client (by its client id, a URL) act on their recipes with these scopes. One
-- active grant per user and client: a new approval replaces the previous one.
CREATE TABLE IF NOT EXISTS oauth_grants (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    scopes TEXT[] NOT NULL,
    resource TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX IF NOT EXISTS oauth_grants_active_idx
    ON oauth_grants (user_id, client_id) WHERE revoked_at IS NULL;

-- The tokens of a grant: access tokens (1 hour) and refresh tokens (30 days, rotated at each
-- use: `rotated_at`, and the new one's `parent_id`). A rotated refresh token keeps its row until
-- it expires: presented again within 60 s, a retry, it gives a fresh pair; later, it revokes the
-- grant (it was copied).
CREATE TABLE IF NOT EXISTS oauth_tokens (
    id BIGSERIAL PRIMARY KEY,
    grant_id BIGINT NOT NULL REFERENCES oauth_grants (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    token_hash BYTEA NOT NULL UNIQUE,
    parent_id BIGINT REFERENCES oauth_tokens (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    rotated_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS oauth_tokens_grant_idx ON oauth_tokens (grant_id);
CREATE INDEX IF NOT EXISTS oauth_tokens_expires_idx ON oauth_tokens (expires_at);

-- A sign-in under way (10 minutes): the client's checked request, our PKCE and nonce with
-- Telegram (`state_hash` finds it on Telegram's answer, `binding_hash` is the browser's cookie),
-- then who signed in and the consent form's token. Consumed once, by the user's decision.
CREATE TABLE IF NOT EXISTS oauth_requests (
    id BIGSERIAL PRIMARY KEY,
    state_hash BYTEA NOT NULL UNIQUE,
    binding_hash BYTEA NOT NULL UNIQUE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    client_state TEXT,
    code_challenge TEXT NOT NULL,
    scopes TEXT[] NOT NULL,
    resource TEXT NOT NULL,
    telegram_verifier TEXT NOT NULL,
    telegram_nonce TEXT NOT NULL,
    telegram_id BIGINT,
    consent_hash BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS oauth_requests_expires_idx ON oauth_requests (expires_at);

-- Our authorization codes (60 s, used once). The grant is made when the code is redeemed; a code
-- presented again revokes it.
CREATE TABLE IF NOT EXISTS oauth_codes (
    code_hash BYTEA PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    scopes TEXT[] NOT NULL,
    resource TEXT NOT NULL,
    grant_id BIGINT REFERENCES oauth_grants (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS oauth_codes_expires_idx ON oauth_codes (expires_at);

-- An event made through an OAuth grant names it, as one made with a personal key names the key
-- (no foreign key: events outlive both)
ALTER TABLE events ADD COLUMN IF NOT EXISTS grant_id BIGINT;
