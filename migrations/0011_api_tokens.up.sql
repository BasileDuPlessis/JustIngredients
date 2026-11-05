-- Personal tokens for the doors other than Telegram (MCP, later the HTTP API): see
-- specs/tasks/engine-api.md. The token itself is never stored, only its SHA-256; `hint` is its
-- beginning, to recognise it in the bot's list. A revoked token keeps its row (revoked_at).
CREATE TABLE IF NOT EXISTS api_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    hint TEXT NOT NULL,
    scopes TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS api_tokens_active_user_idx
    ON api_tokens (user_id) WHERE revoked_at IS NULL;

-- The messages showing a token, which the bot masks 10 minutes after they were sent: found every
-- minute without reading the whole table
CREATE INDEX IF NOT EXISTS chat_messages_secret_sent_idx
    ON chat_messages (sent_at) WHERE kind = 'secret';
