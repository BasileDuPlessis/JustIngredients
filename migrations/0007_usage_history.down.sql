DROP TABLE IF EXISTS app_state;
DROP TABLE IF EXISTS usage_days;
DROP INDEX IF EXISTS users_last_seen_at_idx;
ALTER TABLE users DROP COLUMN IF EXISTS last_seen_at;
