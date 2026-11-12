ALTER TABLE events DROP COLUMN IF EXISTS grant_id;
DROP TABLE IF EXISTS oauth_codes;
DROP TABLE IF EXISTS oauth_requests;
DROP TABLE IF EXISTS oauth_tokens;
DROP TABLE IF EXISTS oauth_grants;
