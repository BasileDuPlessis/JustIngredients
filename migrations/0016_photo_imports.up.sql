-- Recipe photos through an assistant (specs/tasks/mcp-photos.md): an import is an upload link
-- (15 minutes, its secret stored by SHA-256), then the draft Gemini read (24 hours), then the
-- recipes it was saved as. Tied to the key or connection that asked: the link only works while
-- it still may write. The photo itself is never stored.
CREATE TABLE IF NOT EXISTS photo_imports (
    id BIGSERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL,
    upload_hash BYTEA NOT NULL UNIQUE,
    token_id BIGINT,
    grant_id BIGINT,
    status TEXT NOT NULL
        CHECK (status IN ('waiting', 'reading', 'ready', 'failed', 'saved', 'discarded')),
    -- Why the last reading gave no draft (a short code), shown with the link again
    failure TEXT,
    -- The draft: the lines as read and arranged (MeasurementMatch JSON), the text read, the title
    ingredients JSONB,
    transcription TEXT,
    title TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    link_expires_at TIMESTAMPTZ NOT NULL,
    reading_started_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    saved_recipe_ids BIGINT[]
);
CREATE INDEX IF NOT EXISTS photo_imports_owner_idx ON photo_imports (telegram_id);
CREATE INDEX IF NOT EXISTS photo_imports_expires_idx ON photo_imports (expires_at);
