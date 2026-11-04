-- Ids of the bot's and the user's messages that a later action deletes or edits
-- (UX plan U5): never their content
CREATE TABLE IF NOT EXISTS chat_messages (
    chat_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    kind TEXT NOT NULL,
    recipe_id BIGINT,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (chat_id, message_id)
);
CREATE INDEX IF NOT EXISTS chat_messages_chat_kind_idx
    ON chat_messages (chat_id, kind);
