# Database Design Feature Specification

## Overview
The database design provides persistent storage for user data, recipes, and extracted ingredients. It uses PostgreSQL; recipes are searched in Rust over their names and ingredient names (see [Search](#search)); an older full-text column and index on recipe content remain unread. The schema is **recipe-centric**: users own recipes, and recipes contain ingredients. There is no intermediate OCR entry table.

> Verified against `src/db.rs` at `ec2e7d3` (2026-10-27). Planned changes (one list entry per
> recipe, one owner identity, French-aware search) are in [UX Improvements](tasks/ux-improvements.md)
> Phases U3 and U4.

## Requirements

### Functional Requirements
- **User Management**: Store users with the Telegram language code seen at creation (replies use the language of each message, not this column) and their unit view (`unit_view`, UX plan U8)
- **Recipe Storage**: Persist recipes with the extracted text (the Gemini transcription of the ingredient section; recipes saved before Phase 5 may hold OCR text) and an optional name
- **Ingredient Storage**: Structured storage of parsed ingredients linked to recipes
- **Search**: `searchable_recipes` reads a user's named recipes with their ingredient names; `crate::search` matches the words
- **Recipe Organization**: List, paginate, and query recipes per user
- **Ownership**: Every recipe and ingredient query takes the owner's Telegram id and filters on it (ingredients through their recipe), so a user can never read or change another user's data
- **Data Integrity**: Maintain referential integrity
- **Performance**: Efficient queries with caching for common operations

### Non-Functional Requirements
- **Scalability**: Support growing user base and data volume
- **Performance**: Sub-second query response times
- **Reliability**: ACID compliance for data consistency
- **Security**: User data isolation and access control
- **Maintainability**: Clear schema design and documentation

### Success Criteria
- [x] All user data properly stored and retrieved
- [x] Recipe and ingredient data accurately stored
- [ ] Search used by the bot (planned: UX Phase U4; the existing query covers content only)
- [x] Performance meets sub-second requirements
- [x] Data integrity maintained through constraints
- [x] Versioned migration system with rollback support

## Code Layout
`src/db/`, one module per table group, all re-exported by `db/mod.rs` (callers write
`crate::db::…`): `users.rs`, `recipes.rs` (recipes, links, lists, names), `ingredients.rs`,
`chat_messages.rs`, `usage.rs` (`usage_days`, `app_state`), `schema.rs` (startup setup and
check), `migrations.rs`. The dialogue states (`dialogues`) are read and written by
`src/dialogue_storage.rs`.

## Architecture

### Schema Design
```
Database Schema
├── users table          # User profiles and language preferences
├── recipes table        # Recipe records owned by a telegram user, with an optional short note (migration 10)
├── ingredients table    # Parsed ingredient data linked to recipes
├── chat_messages        # Ids of messages a later action deletes or edits (migration 3)
├── recipe_links         # Pairs of linked recipes (migration 6)
├── usage_days, app_state # Usage history and values kept across restarts (migration 7)
├── dialogues            # Each chat's running action (migration 8)
├── _sqlx_migrations     # Applied migrations, by sqlx (schema_migrations: the former table, kept)
└── FTS index           # GIN index on the generated content_tsv column (content only)
```

### Table Relationships
```
users (1) ──── (N) recipes (1) ──── (N) ingredients
  telegram_id              recipe_id (FK)
  id ─────────────────────────────── user_id (FK)
```

Note: `recipes.telegram_id` references `users.telegram_id` (foreign key `recipes_telegram_id_fkey`, migration 9; no cascade: users are never deleted, and `save_recipe()` creates the user first); no join table is needed. `ingredients.user_id` is the internal `users.id`.

### Data Flow
1. **User Registration**: `get_or_create_user()` when a recipe is saved or an ingredient is added (not on `/start`)
2. **Recipe Creation**: `create_recipe()` stores the extracted text under `telegram_id`
3. **Ingredient Storage**: a new recipe is saved by `save_recipe()` in one transaction (user upsert with `ON CONFLICT (telegram_id)`, recipe with its name, one insert per ingredient; nothing is written if one fails). The edit of a saved recipe is applied by `apply_ingredient_changes()` in one transaction (updates write the exact values, so a quantity or unit can be cleared; additions; deletions).
4. **Recipe Naming**: (names are limited to 255 characters, counted as characters since 2026-11-05; they were counted as bytes) a user's recipe names are unique: `save_recipe()` and `rename_recipe()` store "Tarte (2)", "Tarte (3)"… when the name is taken (`unique_recipe_name`, first free number from 2, within 255 bytes) and return the stored name; `update_recipe_name()` writes a name as it is (tests)
5. **Lookups**: the recipe list and the home message read the user's named recipes in alphabetical order with their total (`get_user_recipes_alphabetical`: all the user's named recipes are read with `recipes_telegram_id_created_at_idx` and sorted in Rust by `search::compare_names`, case and accents ignored, numbers by value, then oldest first; returns one page of `RecipeSummary { id, name, created_at }`; since 2026-10-30, newest first before); a recipe is opened by id (`read_recipe_with_name`); no full-text query is used

## API Design

### Core Operations
```rust
// User management
pub async fn get_or_create_user(pool: &PgPool, telegram_id: i64, language_code: Option<&str>) -> Result<User>
pub async fn get_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> Result<Option<User>>

// Recipe CRUD (owner-scoped: rows of another telegram_id behave as missing)
pub async fn create_recipe(pool: &PgPool, telegram_id: i64, content: &str) -> Result<i64>
pub async fn delete_recipe(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<bool> // also deletes its ingredients, in one transaction
pub async fn update_recipe_name(pool: &PgPool, telegram_id: i64, recipe_id: i64, recipe_name: &str) -> Result<bool>
pub async fn read_recipe_with_name(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Option<Recipe>>

// Linked recipes (UX plan U9)
pub async fn save_recipe_parts(pool: &PgPool, telegram_id: i64, language_code: Option<&str>, content: &str, parts: &[RecipePart<'_>]) -> Result<Vec<SavedRecipe>> // one transaction: each part saved with a unique name, all linked together; save_recipe is one part
pub async fn linked_recipes(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Vec<RecipeSummary>> // both ways, alphabetical, the owner's only

// Unit views and units (UX plan U8)
pub async fn unit_view(pool: &PgPool, telegram_id: i64) -> Result<UnitView> // French units when never chosen
pub async fn set_unit_view(pool: &PgPool, telegram_id: i64, language_code: Option<&str>, view: UnitView) -> Result<()> // creates the user when needed
pub async fn normalize_saved_units(pool: &PgPool) -> Result<u64> // startup pass: units saved before normalization rewritten with units::normalize_measurement

// Recipe search & listing
pub async fn searchable_recipes(pool: &PgPool, telegram_id: i64) -> Result<Vec<SearchableRecipe>> // named recipes with their ingredient names, newest first (search)
pub async fn get_user_recipes_alphabetical(pool: &PgPool, telegram_id: i64, limit: i64, offset: i64) -> Result<(Vec<RecipeSummary>, i64)> // named recipes, alphabetical (sorted in Rust), one page, total (recipe list, home message)
pub async fn has_duplicate_recipes(pool: &PgPool, telegram_id: i64, recipe_name: &str) -> Result<bool> // not called by the bot

// Transactional writes of the bot
pub async fn save_recipe(pool: &PgPool, telegram_id: i64, language_code: Option<&str>, content: &str, name: &str, ingredients: &[NewIngredient<'_>]) -> Result<SavedRecipe> // recipe_id, user_id
pub async fn apply_ingredient_changes(pool: &PgPool, telegram_id: i64, recipe_id: i64, language_code: Option<&str>, updates: &[IngredientUpdate<'_>], additions: &[NewIngredient<'_>], deletions: &[i64]) -> Result<()> // fails without writing for another user's recipe

// Ingredient CRUD (owner-scoped through the ingredient's recipe, except where noted)
pub async fn create_ingredient(pool: &PgPool, user_id: i64, recipe_id: Option<i64>, name: &str, quantity: Option<f64>, unit: Option<&str>, raw_text: &str) -> Result<i64> // user_id = users.id; callers check recipe ownership
pub async fn get_recipe_ingredients(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Vec<Ingredient>> // ORDER BY created_at, id: a recipe saved in one transaction gives all its ingredients the same created_at

// No cache: the bot reads users, recipes and ingredients from the database each time (the
// recipe caches went on 2026-10-28, IMP-010; the unused user caches on 2026-11-03, IMP-035)

// Schema management
pub async fn init_database_schema(pool: &PgPool) -> Result<()>
pub async fn validate_database_schema(pool: &PgPool) -> Result<()>
```

### Data Models
Rows are read with `sqlx::query_as` into these `#[derive(sqlx::FromRow)]` structs (also
`RecipeSummary` and `ChatMessage`): columns map by name (`quantity::float8 AS quantity`,
`recipe_name AS name`), and a type mismatch is an error, never a panic. No positional
`row.get(i)` is left.
```rust
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: String,  // COALESCE(language_code, 'en'): the column is nullable
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Recipe {
    pub id: i64,
    pub telegram_id: i64,       // Owner
    pub content: String,        // Extracted text (Gemini transcription; older rows: OCR text)
    pub recipe_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct Ingredient {
    pub id: i64,
    pub user_id: i64,
    pub recipe_id: Option<i64>,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## Migration System

Versioned schema migrations with rollback support, run by sqlx (backlog IMP-028,
`src/db/migrations.rs`): one pair of SQL files per version in `migrations/`
(`NNNN_name.up.sql` and `.down.sql`), embedded at build time by `sqlx::migrate!` (`build.rs`
rebuilds when they change; the Dockerfile copies the folder).

```rust
pub static MIGRATOR: Migrator                                   // the embedded migrations
pub async fn run(pool: &PgPool) -> Result<()>                   // adopt, then apply the pending ones
pub async fn current_version(pool: &PgPool) -> Result<i64>      // 0 without migrations
pub async fn undo_to(pool: &PgPool, target: i64) -> Result<()>  // down scripts above `target`
```

Migrations are applied at startup via `init_database_schema()`, each in a transaction, under
sqlx's advisory lock, recorded with its checksum in `_sqlx_migrations` (a changed file is an
error, never applied silently). A database migrated by the former in-house system has no
`_sqlx_migrations` but a `schema_migrations` table: the first start records its versions in
`_sqlx_migrations` as applied, with the files' checksums, without running them again
(`adopt_legacy_versions`), then runs what comes after. `schema_migrations` is left as it is, so
that a release from before the switch (an automatic rollback) still reads its version. Versions applied but unknown to the binary are ignored (`ignore_missing`), so a rollback to an older release starts on a database that is ahead of it (a changed checksum of a known version is still an error). Each
migration is idempotent. There are ten: version 1 `create_initial_tables`, version 2 `recipes_owner_index_and_ingredient_cascade` (owner index on `recipes`, `ON DELETE CASCADE` on `ingredients.recipe_id`), version 3 `chat_messages` (see below) and version 4 `number_duplicate_recipe_names` (a placeholder since RU6: the numbering runs at every startup in `number_duplicate_recipe_names()`, after the schema validation, with the rule of new names — the oldest of a user's same-name recipes keeps the name, the others take the first free "(n)" — and does nothing once names are unique) version 5 `users_unit_view` (`users.unit_view TEXT NOT NULL DEFAULT 'french'`) and version 6 `recipe_links` (`recipe_links (recipe_a, recipe_b)`, one row per pair of linked recipes, the smaller id first, both foreign keys `ON DELETE CASCADE`, index on `recipe_b`: UX plan U9), version 7 `usage_history` (`users.last_seen_at` and its index, `usage_days`, `app_state`: plan O, see [deployment.md](deployment.md)) version 8 `dialogues` (below: backlog IMP-022) and version 9 `recipes_owner_fk` (adds a `users` row for any recipe owner that has none, then the foreign key `recipes.telegram_id → users.telegram_id`: backlog IMP-026) and version 10 `recipe_note` (`recipes.note`, a short free text, `CHECK (char_length(note) <= 300)`; the bot accepts 200, see [recipe-notes.md](tasks/recipe-notes.md)); each has a down script. Then `validate_database_schema()` checks the tables and columns and stops the startup on a mismatch. After it come the duplicate numbering and `normalize_saved_units()`, which rewrites the units saved before normalization ("G", "litres", "cl" → g, l, ml with the quantity converted…) one ingredient at a time, skips what `units::renormalized` finds unsafe (an unstable spelling, a quantity that does not fit), logs a failed row and goes on, and does nothing once every unit is normalized; its own failure never stops the startup. Just before the dispatcher starts, `PgDialogueStorage::start` deletes the dialogue states that are not resumed; its failure is only logged.

### Constraints
- **Data Types**: Appropriate PostgreSQL types for performance
- **Indexing Strategy**: Optimized for common query patterns
- **Foreign Keys**: Referential integrity enforcement
- **Text Encoding**: UTF-8 support for international content

### Dependencies
- **sqlx**: only `create_recipe` uses `query!`, checked at compile time against `.sqlx/`; every
  other query is checked when it runs
- **tokio**: Async database operations
- **chrono**: Date/time handling
- **serde**: Data serialization

## Schema Details

### Users Table
```sql
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    telegram_id BIGINT UNIQUE NOT NULL,
    language_code VARCHAR(10) DEFAULT 'en',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    unit_view TEXT NOT NULL DEFAULT 'french' -- recipe, weight, french or us (migration 5)
);
```

### Recipes Table
```sql
CREATE TABLE recipes (
    id BIGSERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL,
    content TEXT NOT NULL,          -- Extracted text (transcription; older rows: OCR text)
    recipe_name VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,
    note TEXT CONSTRAINT recipes_note_length CHECK (char_length(note) <= 300)  -- migration 10; NULL = no note
);
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
-- Migration 2: every user-facing query filters by owner, newest first
CREATE INDEX recipes_telegram_id_created_at_idx ON recipes (telegram_id, created_at DESC, id DESC);
```

### Ingredients Table
```sql
CREATE TABLE ingredients (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id),
    recipe_id BIGINT REFERENCES recipes(id) ON DELETE CASCADE,  -- cascade since migration 2
    name VARCHAR(255) NOT NULL,
    quantity DECIMAL(10,3),
    unit VARCHAR(50),
    raw_text TEXT,                  -- NULL since 2026-10-28; older rows repeat recipes.content
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX ingredients_recipe_id_idx ON ingredients (recipe_id);
CREATE INDEX ingredients_user_id_idx   ON ingredients (user_id);
```

### Chat Messages Table (migration 3)
```sql
-- Ids of messages a later action deletes or edits (UX plan U5), never their content
CREATE TABLE chat_messages (
    chat_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    kind TEXT NOT NULL,              -- transient (the running action's messages) or home
    recipe_id BIGINT,                -- for recipe cards (U5.4)
    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (chat_id, message_id)
);
CREATE INDEX chat_messages_chat_kind_idx ON chat_messages (chat_id, kind);
```
`record_chat_message` (upsert), `forget_chat_message`, `take_chat_messages_except` (delete and return,
with whether the message is under 48 hours old, the limit for Telegram deletions).

### Dialogues Table (migration 8)
```sql
-- Each chat's running action (review, edit, rename), so a deploy or a restart does not lose it
CREATE TABLE dialogues (
    chat_id BIGINT PRIMARY KEY,
    state JSON NOT NULL,             -- RecipeDialogueState; json, not jsonb: keeps \u0000
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```
`dialogue_storage::PgDialogueStorage` implements teloxide's `Storage`: an upsert per state
change, a delete when the action ends (also when there is none). A state unchanged for 48 hours
is not read; a state that no longer reads is deleted. `start` (at startup) deletes the states
unchanged for 48 hours, or all of them when the `app_state` row `dialogues_alive` (noted by
`mark_alive` every minute and at shutdown) is missing or over 10 minutes old; the schema check
includes the table's columns.
Timings in `ji_db_duration_seconds{operation="dialogue_get|dialogue_update|dialogue_remove|dialogue_purge"}`.

### Migrations Tables
`_sqlx_migrations` is sqlx's (version, description, installed_on, success, checksum,
execution_time; adopted versions have `execution_time` 0). The former table, kept for a
rollback to a release from before the switch and no longer written:
```sql
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Search

The bot's search (UX plan U4) reads the user's named recipes with their ingredient names in one
query (`searchable_recipes()`: `array_agg` over a left join, newest first) and matches the words in
Rust (`crate::search`: whole words, case, French accents and ligatures, plurals and small words
ignored). A user
has at most a few hundred recipes, so no Postgres extension (`unaccent` needs rights the app user
may not have) and no index are needed. The former English full-text query `search_recipes` was
removed; its generated column and GIN index below are no longer read.

### FTS Configuration
```sql
-- Generated column and GIN index on recipe content
content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
```

### Queries
```sql
-- get_user_recipes_alphabetical: recipe list and home message (sorted and paged in Rust)
SELECT id, recipe_name, created_at FROM recipes
WHERE telegram_id = $1 AND recipe_name IS NOT NULL;
```

## Testing Strategy

### Unit Tests
- Data model validation
- Query construction testing
- Error handling verification

### Integration Tests
- Full database operations
- Migration testing
- FTS functionality validation

### Performance Tests
- Query response time validation
- Concurrent access testing
- Index effectiveness measurement

## Monitoring & Observability

### Metrics
- `db_operations_total{operation}`, `db_operation_duration_seconds`, `db_rows_affected{operation}`,
  `db_query_complexity{operation}`,
  `db_performance_class_total{operation,class}` (fast < 10 ms, medium < 100 ms, slow), recorded
  by `create_recipe`, `create_ingredient`, `save_recipe` and `apply_ingredient_changes`
- No connection pool or database size metrics
- Database health: `/health/ready` and the 60 s health recorder run `SELECT 1`

### Logging
- Connection retries at startup (`db_connection::connect_with_retry`)
- Connection issues and query errors
- Migration execution

## Future Enhancements

### Potential Improvements
- **Advanced Search**: Fuzzy matching, synonym support
- **Analytics**: Usage patterns and ingredient insights
- **Backup Strategy**: Automated backup and recovery
- **Data Export**: Recipe export in various formats