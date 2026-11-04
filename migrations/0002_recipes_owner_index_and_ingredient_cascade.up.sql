-- Every user-facing query filters recipes by owner, newest first; deleting a recipe
-- deletes its ingredients.
CREATE INDEX IF NOT EXISTS recipes_telegram_id_created_at_idx
    ON recipes (telegram_id, created_at DESC, id DESC);
ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
    FOREIGN KEY (recipe_id) REFERENCES recipes(id) ON DELETE CASCADE;
