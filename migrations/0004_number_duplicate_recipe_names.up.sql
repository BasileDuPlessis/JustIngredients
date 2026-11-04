-- The numbering itself runs at every startup in Rust
-- (`number_duplicate_recipe_names`, the rule of new names, idempotent): the first
-- SQL version of this migration (applied on staging) could leave duplicates
SELECT 1;
