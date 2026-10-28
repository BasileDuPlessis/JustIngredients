# Database Design Feature Specification

## Overview
The database design provides persistent storage for user data, recipes, and extracted ingredients. It uses PostgreSQL; a full-text column and index exist on recipe content, but the bot does not search yet (see [Full-Text Search](#full-text-search-implementation)). The schema is **recipe-centric**: users own recipes, and recipes contain ingredients. There is no intermediate OCR entry table.

> Verified against `src/db.rs` at `ec2e7d3` (2026-10-27). Planned changes (one list entry per
> recipe, one owner identity, French-aware search) are in [UX Improvements](tasks/ux-improvements.md)
> Phases U3 and U4.

## Requirements

### Functional Requirements
- **User Management**: Store users with the Telegram language code seen at creation (replies use the language of each message, not this column)
- **Recipe Storage**: Persist recipes with the extracted text (the Gemini transcription of the ingredient section; recipes saved before Phase 5 may hold OCR text) and an optional name
- **Ingredient Storage**: Structured storage of parsed ingredients linked to recipes
- **Full-Text Search**: `search_recipes` searches recipe content (not names) with the English configuration; no handler calls it
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
- [x] In-memory caching reduces repeated DB lookups
- [x] Versioned migration system with rollback support

## Architecture

### Schema Design
```
Database Schema
├── users table          # User profiles and language preferences
├── recipes table        # Recipe records owned by a telegram user
├── ingredients table    # Parsed ingredient data linked to recipes
├── schema_migrations    # Versioned migration tracking
└── FTS index           # GIN index on the generated content_tsv column (content only)
```

### Table Relationships
```
users (1) ──── (N) recipes (1) ──── (N) ingredients
  telegram_id              recipe_id (FK)
  id ─────────────────────────────── user_id (FK)
```

Note: `recipes.telegram_id` holds the Telegram user id by value (no foreign key to `users`); no join table is needed. `ingredients.user_id` is the internal `users.id`.

### Data Flow
1. **User Registration**: `get_or_create_user()` when a recipe is saved or an ingredient is added (not on `/start`)
2. **Recipe Creation**: `create_recipe()` stores the extracted text under `telegram_id`
3. **Ingredient Storage**: a new recipe is saved by `save_recipe()` in one transaction (user upsert with `ON CONFLICT (telegram_id)`, recipe with its name, one insert per ingredient; nothing is written if one fails). The edit of a saved recipe is applied by `apply_ingredient_changes()` in one transaction (updates write the exact values, so a quantity or unit can be cleared; additions; deletions).
4. **Recipe Naming**: `update_recipe_name()` sets human-readable name
5. **Lookups**: the recipe list reads distinct names (`get_user_recipes_paginated`), a selected name is matched exactly (`recipe_name = $2`); the home message reads the newest named recipes with their total (`get_user_recipes_newest_first`, `ORDER BY created_at DESC, id DESC`, returns `RecipeSummary { id, name, created_at }`); no full-text query is used

## API Design

### Core Operations
```rust
// User management
pub async fn get_or_create_user(pool: &PgPool, telegram_id: i64, language_code: Option<&str>) -> Result<User>
pub async fn get_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> Result<Option<User>>
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> // not called by the bot

// Cached user lookups (5-minute TTL), none called by the bot
pub async fn get_or_create_user_cached(pool, telegram_id, language_code, cache) -> Result<User>
pub async fn get_user_by_telegram_id_cached(pool, telegram_id, cache) -> Result<Option<User>>
pub async fn get_user_by_id_cached(pool, user_id, cache) -> Result<Option<User>>

// Recipe CRUD (owner-scoped: rows of another telegram_id behave as missing)
pub async fn create_recipe(pool: &PgPool, telegram_id: i64, content: &str) -> Result<i64>
pub async fn read_recipe(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Option<Recipe>> // not called by the bot
pub async fn update_recipe(pool: &PgPool, telegram_id: i64, recipe_id: i64, new_content: &str) -> Result<bool> // not called by the bot
pub async fn delete_recipe(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<bool> // also deletes its ingredients, in one transaction
pub async fn update_recipe_name(pool: &PgPool, telegram_id: i64, recipe_id: i64, recipe_name: &str) -> Result<bool>
pub async fn read_recipe_with_name(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Option<Recipe>>

// Recipe search & listing
pub async fn search_recipes(pool: &PgPool, telegram_id: i64, query: &str) -> Result<Vec<Recipe>> // not called by the bot
pub async fn get_recipes_by_name(pool: &PgPool, telegram_id: i64, recipe_name: &str) -> Result<Vec<Recipe>> // exact name
pub async fn get_user_recipe_names(pool: &PgPool, telegram_id: i64) -> Result<Vec<String>>
pub async fn get_user_recipes_paginated(pool: &PgPool, telegram_id: i64, limit: i64, offset: i64) -> Result<(Vec<String>, i64)> // distinct names A-Z, total
pub async fn get_user_recipes_newest_first(pool: &PgPool, telegram_id: i64, limit: i64, offset: i64) -> Result<(Vec<RecipeSummary>, i64)> // named recipes, newest first, total (home message)
pub async fn has_duplicate_recipes(pool: &PgPool, telegram_id: i64, recipe_name: &str) -> Result<bool> // not called by the bot
pub async fn get_user_recipe_statistics(pool: &PgPool, telegram_id: i64) -> Result<RecipeStatistics>

// Transactional writes of the bot
pub async fn save_recipe(pool: &PgPool, telegram_id: i64, language_code: Option<&str>, content: &str, name: &str, ingredients: &[NewIngredient<'_>]) -> Result<SavedRecipe> // recipe_id, user_id
pub async fn apply_ingredient_changes(pool: &PgPool, telegram_id: i64, recipe_id: i64, language_code: Option<&str>, updates: &[IngredientUpdate<'_>], additions: &[NewIngredient<'_>], deletions: &[i64]) -> Result<()> // fails without writing for another user's recipe

// Ingredient CRUD (owner-scoped through the ingredient's recipe, except where noted)
pub async fn create_ingredient(pool: &PgPool, user_id: i64, recipe_id: Option<i64>, name: &str, quantity: Option<f64>, unit: Option<&str>, raw_text: &str) -> Result<i64> // user_id = users.id; callers check recipe ownership
pub async fn read_ingredient(pool: &PgPool, telegram_id: i64, ingredient_id: i64) -> Result<Option<Ingredient>> // not called by the bot
pub async fn update_ingredient(pool: &PgPool, telegram_id: i64, ingredient_id: i64, name: Option<&str>, quantity: Option<f64>, unit: Option<&str>) -> Result<bool>
pub async fn delete_ingredient(pool: &PgPool, telegram_id: i64, ingredient_id: i64) -> Result<bool>
pub async fn list_ingredients_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Ingredient>> // by users.id, not called by the bot
pub async fn get_recipe_ingredients(pool: &PgPool, telegram_id: i64, recipe_id: i64) -> Result<Vec<Ingredient>>

// The user *_cached functions keep results 5 minutes; the bot reads recipes and ingredients
// without a cache (the recipe caches were removed on 2026-10-28, IMP-010)

// Schema management
pub async fn init_database_schema(pool: &PgPool) -> Result<()>
pub async fn validate_database_schema(pool: &PgPool) -> Result<()>
```

### Data Models
```rust
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: Arc<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Recipe {
    pub id: i64,
    pub telegram_id: i64,       // Owner
    pub content: Arc<String>,   // Extracted text (Gemini transcription; older rows: OCR text)
    pub recipe_name: Option<Arc<String>>,
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

pub struct RecipeStatistics {
    pub total_recipes: i64,
    pub total_ingredients: i64,
    // ... per-user aggregate stats
}
```

## Migration System

Versioned schema migrations with rollback support (`src/db.rs` `migrations` submodule):

```rust
// Apply all pending migrations in order
pub async fn apply_pending_migrations(pool: &PgPool) -> Result<()>

// Get current schema version
pub async fn get_current_version(pool: &PgPool) -> Result<i32>

// Rollback to a specific version
pub async fn rollback_to_version(pool: &PgPool, target_version: i32) -> Result<()>
```

Migrations are applied at startup via `init_database_schema()`. Each migration is idempotent and recorded in `schema_migrations`. There is one migration today (version 1, `create_initial_tables`). Then `validate_database_schema()` checks the tables and columns and stops the startup on a mismatch.

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
    updated_at TIMESTAMPTZ DEFAULT NOW()
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
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
);
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
```

### Ingredients Table
```sql
CREATE TABLE ingredients (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id),
    recipe_id BIGINT REFERENCES recipes(id),
    name VARCHAR(255) NOT NULL,
    quantity DECIMAL(10,3),
    unit VARCHAR(50),
    raw_text TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX ingredients_recipe_id_idx ON ingredients (recipe_id);
CREATE INDEX ingredients_user_id_idx   ON ingredients (user_id);
```

### Migrations Table
```sql
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Full-Text Search Implementation

The column, index and query exist and are covered by a database test, but no bot handler calls
`search_recipes`: the search button answers "coming soon". The English configuration does not
stem French words or ignore accents, and recipe names and ingredient names are not searched.
A French-aware search is planned in [UX Improvements](tasks/ux-improvements.md) Phase U4.

### FTS Configuration
```sql
-- Generated column and GIN index on recipe content
content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
```

### Queries
```sql
-- search_recipes (user-scoped, unused by the bot)
SELECT id, telegram_id, content, recipe_name, created_at FROM recipes
WHERE telegram_id = $1 AND content_tsv @@ plainto_tsquery('english', $2)
ORDER BY created_at DESC;

-- get_recipes_by_name: exact name, used when a name is picked in the recipe list
SELECT id, telegram_id, content, recipe_name, created_at FROM recipes
WHERE telegram_id = $1 AND recipe_name = $2
ORDER BY created_at DESC;
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
  by `create_recipe`, `create_ingredient`, `save_recipe`, `apply_ingredient_changes` and
  `get_recipes_by_name`
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