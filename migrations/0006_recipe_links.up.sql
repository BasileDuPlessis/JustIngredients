-- Recipes linked together, both ways (UX plan U9): one row per pair, the smaller
-- id first, gone with either recipe
CREATE TABLE IF NOT EXISTS recipe_links (
    recipe_a BIGINT NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    recipe_b BIGINT NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    PRIMARY KEY (recipe_a, recipe_b),
    CHECK (recipe_a < recipe_b)
);
CREATE INDEX IF NOT EXISTS recipe_links_recipe_b_idx ON recipe_links (recipe_b);
