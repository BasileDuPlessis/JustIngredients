-- Plan O3: each user's last visit (active users), and one line of totals per
-- day, Paris time, kept beyond the 15 days of Fly's metrics; plan O4 and O5: a
-- few values kept across restarts (release running, last weekly report)
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS users_last_seen_at_idx ON users (last_seen_at);
CREATE TABLE IF NOT EXISTS usage_days (
    day DATE PRIMARY KEY,
    active_users BIGINT NOT NULL DEFAULT 0,
    photos_read BIGINT NOT NULL DEFAULT 0,
    photos_failed BIGINT NOT NULL DEFAULT 0,
    recipes_created BIGINT NOT NULL DEFAULT 0,
    gemini_calls BIGINT NOT NULL DEFAULT 0,
    gemini_failures BIGINT NOT NULL DEFAULT 0,
    gemini_cost_micro_usd BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS app_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
