-- Backlog IMP-022: each chat's running action (review, edit, rename) as JSON, so
-- a deploy or a restart does not lose it (`dialogue_storage`)
CREATE TABLE IF NOT EXISTS dialogues (
    chat_id BIGINT PRIMARY KEY,
    state JSON NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
