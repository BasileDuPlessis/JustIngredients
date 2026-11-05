-- A short free text kept with a recipe (oven temperature, cooking time, a source link…).
-- NULL means no note. The bot cuts nothing: it refuses over 200 characters; the constraint is a
-- second wall with some room.
ALTER TABLE recipes ADD COLUMN IF NOT EXISTS note TEXT;

ALTER TABLE recipes DROP CONSTRAINT IF EXISTS recipes_note_length;
ALTER TABLE recipes
    ADD CONSTRAINT recipes_note_length CHECK (char_length(note) <= 300);
