-- The outbox of the engine's changes (specs/tasks/engine-api.md, §8 and §15b): one row per
-- change, written in the transaction of the change, so a change never goes without its event.
-- Thin: what changed and where it came from; consumers read the current state. A deleted
-- recipe's event keeps its id (no foreign key on recipe_id).
CREATE TABLE IF NOT EXISTS events (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    recipe_id BIGINT,
    origin TEXT NOT NULL,
    token_id BIGINT,
    version SMALLINT NOT NULL DEFAULT 1,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS events_created_at_idx ON events (created_at);

-- Where each consumer of the events is: the last event it handled
CREATE TABLE IF NOT EXISTS event_cursors (
    consumer TEXT PRIMARY KEY,
    last_id BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
