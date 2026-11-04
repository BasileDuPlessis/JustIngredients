-- Every recipe belongs to a user row: add the missing rows (recipes saved before users were
-- always created), then the foreign key on the owner. Safe to run again, like the others.
INSERT INTO users (telegram_id)
SELECT DISTINCT telegram_id FROM recipes
ON CONFLICT (telegram_id) DO NOTHING;

ALTER TABLE recipes DROP CONSTRAINT IF EXISTS recipes_telegram_id_fkey;
ALTER TABLE recipes
    ADD CONSTRAINT recipes_telegram_id_fkey
    FOREIGN KEY (telegram_id) REFERENCES users (telegram_id);
