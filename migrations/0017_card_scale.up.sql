-- A recipe card shown with its quantities scaled (specs/tasks/recipe-scaling.md): the factor it
-- shows ("4/3"), so that updating the card (a rename, a memo) keeps it. The recipe itself never
-- changes.
ALTER TABLE chat_messages ADD COLUMN IF NOT EXISTS scale TEXT;
