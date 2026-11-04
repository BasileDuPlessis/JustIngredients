ALTER TABLE ingredients DROP CONSTRAINT IF EXISTS ingredients_recipe_id_fkey;
ALTER TABLE ingredients ADD CONSTRAINT ingredients_recipe_id_fkey
    FOREIGN KEY (recipe_id) REFERENCES recipes(id);
DROP INDEX IF EXISTS recipes_telegram_id_created_at_idx;
