-- OAuth sign-ins (migration 13), after review: Telegram's answer is claimed once before its
-- code is exchanged (a reload or a double load must not end the sign-in), and the name of the
-- Telegram account is kept until consent, which shows it (a shared computer may be signed in
-- as someone else).
ALTER TABLE oauth_requests ADD COLUMN IF NOT EXISTS claimed_at TIMESTAMPTZ;
ALTER TABLE oauth_requests ADD COLUMN IF NOT EXISTS telegram_name TEXT;
