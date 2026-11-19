-- Each user's paid photo readings per day (specs/tasks/mcp-photos.md), shared by Telegram and the
-- assistants and kept across deploys (the count used to live in memory). Paris days, as
-- usage_days. A row is taken atomically: calls + 1 only while under the limit.
CREATE TABLE IF NOT EXISTS user_photo_days (
    telegram_id BIGINT NOT NULL,
    day DATE NOT NULL,
    calls INTEGER NOT NULL CHECK (calls >= 0),
    PRIMARY KEY (telegram_id, day)
);
