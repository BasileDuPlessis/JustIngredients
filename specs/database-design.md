# Database Design Feature Specification

## Overview
The database design provides persistent storage for user data, recipes, and extracted ingredients. It uses PostgreSQL with full-text search capabilities to enable efficient querying and retrieval. The schema is **recipe-centric**: users own recipes, and recipes contain ingredients. There is no intermediate OCR entry table.

## Requirements

### Functional Requirements
- **User Management**: Store user profiles and language preferences
- **Recipe Storage**: Persist recipes with OCR text and optional name
- **Ingredient Storage**: Structured storage of parsed ingredients linked to recipes
- **Full-Text Search**: Search recipes by content and name
- **Recipe Organization**: List, paginate, and query recipes per user
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
- [x] Full-text search working across recipe names
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
└── FTS indexes         # Full-text search on recipe content/name
```

### Table Relationships
```
users (1) ──── (N) recipes (1) ──── (N) ingredients
  telegram_id              recipe_id (FK)
```

Note: `recipes.telegram_id` directly references the Telegram user; no join table is needed.

### Data Flow
1. **User Registration**: `get_or_create_user()` on first Telegram interaction
2. **Recipe Creation**: `create_recipe()` stores OCR text under `telegram_id`
3. **Ingredient Parsing**: `create_ingredient()` / `update_recipe_ingredients()` persist parsed data
4. **Recipe Naming**: `update_recipe_name()` sets human-readable name
5. **Search Queries**: FTS on `content`, name lookup by ILIKE

## API Design

### Core Operations
```rust
// User management
pub async fn get_or_create_user(pool: &PgPool, telegram_id: i64, language_code: Option<&str>) -> Result<User>
pub async fn get_user_by_telegram_id(pool: &PgPool, telegram_id: i64) -> Result<Option<User>>
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>>

// Cached user lookups (5-minute TTL)
pub async fn get_or_create_user_cached(pool, telegram_id, language_code, cache) -> Result<User>
pub async fn get_user_by_telegram_id_cached(pool, telegram_id, cache) -> Result<Option<User>>

// Recipe CRUD
pub async fn create_recipe(pool: &PgPool, telegram_id: i64, content: &str) -> Result<i64>
pub async fn read_recipe(pool: &PgPool, recipe_id: i64) -> Result<Option<Recipe>>
pub async fn update_recipe(pool: &PgPool, recipe_id: i64, new_content: &str) -> Result<bool>
pub async fn delete_recipe(pool: &PgPool, recipe_id: i64) -> Result<bool>
pub async fn update_recipe_name(pool: &PgPool, recipe_id: i64, recipe_name: &str) -> Result<bool>
pub async fn read_recipe_with_name(pool: &PgPool, recipe_id: i64) -> Result<Option<Recipe>>

// Recipe search & listing
pub async fn search_recipes(pool: &PgPool, telegram_id: i64, query: &str) -> Result<Vec<Recipe>>
pub async fn get_recipes_by_name(pool: &PgPool, telegram_id: i64, name: &str) -> Result<Vec<Recipe>>
pub async fn get_recipes_by_name_cached(pool, telegram_id, name, cache) -> Result<Vec<Recipe>>
pub async fn get_user_recipes_paginated(pool: &PgPool, telegram_id: i64, page: i64, page_size: i64) -> Result<Vec<Recipe>>
pub async fn has_duplicate_recipes(pool: &PgPool, telegram_id: i64, name: &str) -> Result<bool>
pub async fn get_user_recipe_statistics(pool: &PgPool, telegram_id: i64) -> Result<RecipeStatistics>

// Ingredient CRUD
pub async fn create_ingredient(pool: &PgPool, params: CreateIngredientParams) -> Result<Ingredient>
pub async fn read_ingredient(pool: &PgPool, ingredient_id: i64) -> Result<Option<Ingredient>>
pub async fn update_ingredient(pool: &PgPool, ingredient_id: i64, updates: IngredientUpdate) -> Result<bool>
pub async fn delete_ingredient(pool: &PgPool, ingredient_id: i64) -> Result<bool>
pub async fn list_ingredients_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Ingredient>>
pub async fn get_recipe_ingredients(pool: &PgPool, recipe_id: i64) -> Result<Vec<Ingredient>>
pub async fn get_recipe_ingredients_cached(pool, recipe_id, cache) -> Result<Vec<Ingredient>>
pub async fn update_recipe_ingredients(pool: &PgPool, recipe_id: i64, new_ingredients: Vec<NewIngredient>) -> Result<()>

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
    pub content: Arc<String>,   // Raw OCR extracted text
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

Migrations are applied at startup via `init_database_schema()`. Each migration is idempotent and recorded in `schema_migrations`.

### Constraints
- **Data Types**: Appropriate PostgreSQL types for performance
- **Indexing Strategy**: Optimized for common query patterns
- **Foreign Keys**: Referential integrity enforcement
- **Text Encoding**: UTF-8 support for international content

### Dependencies
- **sqlx**: Compile-time SQL query checking
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
    content TEXT NOT NULL,          -- Raw OCR extracted text
    recipe_name VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (to_tsvector('english', content));
```

### Ingredients Table
```sql
CREATE TABLE ingredients (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    recipe_id BIGINT REFERENCES recipes(id),
    name VARCHAR(255) NOT NULL,
    quantity NUMERIC(10,3),
    unit VARCHAR(50),
    raw_text TEXT NOT NULL,
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
    applied_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Full-Text Search Implementation

### FTS Configuration
```sql
-- GIN index on recipe content for keyword search
CREATE INDEX recipes_content_tsv_idx
    ON recipes USING GIN (to_tsvector('english', content));
```

### Search Queries
```sql
-- Recipe full-text search (user-scoped)
SELECT * FROM recipes
WHERE telegram_id = $1
  AND to_tsvector('english', content) @@ plainto_tsquery('english', $2);

-- Recipe name lookup
SELECT * FROM recipes
WHERE telegram_id = $1
  AND LOWER(recipe_name) LIKE LOWER('%' || $2 || '%');
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
- Query execution times
- Connection pool utilization
- Database size growth
- Search query performance

### Logging
- Slow query detection
- Connection issues
- Migration execution
- FTS index updates

## Future Enhancements

### Potential Improvements
- **Advanced Search**: Fuzzy matching, synonym support
- **Analytics**: Usage patterns and ingredient insights
- **Backup Strategy**: Automated backup and recovery
- **Data Export**: Recipe export in various formats