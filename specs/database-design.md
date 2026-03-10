# Database Design Feature Specification

## Overview
The database design feature provides persistent storage for user data, OCR processing results, and ingredient information. It uses PostgreSQL with full-text search capabilities to enable efficient querying and retrieval of recipe data.

## Requirements

### Functional Requirements
- **User Management**: Store user profiles and preferences
- **OCR History**: Maintain complete processing history
- **Ingredient Storage**: Structured storage of parsed ingredients
- **Full-Text Search**: Search across all extracted text
- **Recipe Organization**: Group ingredients by recipes
- **Data Integrity**: Maintain referential integrity
- **Performance**: Efficient queries for common operations

### Non-Functional Requirements
- **Scalability**: Support growing user base and data volume
- **Performance**: Sub-second query response times
- **Reliability**: ACID compliance for data consistency
- **Security**: User data isolation and access control
- **Maintainability**: Clear schema design and documentation

### Success Criteria
- [ ] All user data properly stored and retrieved
- [ ] OCR results searchable and retrievable
- [ ] Ingredient data accurately parsed and stored
- [ ] Full-text search working across all content
- [ ] Performance meets sub-second requirements
- [ ] Data integrity maintained through constraints

## Architecture

### Schema Design
```
Database Schema
├── users table          # User profiles and preferences
├── ocr_entries table    # OCR processing history
├── ingredients table    # Parsed ingredient data
├── user_recipes table   # Recipe organization
└── FTS indexes         # Full-text search support
```

### Table Relationships
```
users (1) ──── (N) ocr_entries (1) ──── (N) ingredients
   │                    │
   └── (N) user_recipes ─┘
```

### Data Flow
1. **User Registration**: Create user profile on first interaction
2. **OCR Processing**: Store processing metadata and results
3. **Ingredient Parsing**: Extract and store structured data
4. **Recipe Creation**: Group ingredients under recipe names
5. **Search Queries**: Full-text search across all content

## API Design

### Core Operations
```rust
// User management
pub async fn create_user(db: &DbConnection, telegram_id: i64) -> Result<User, DbError>
pub async fn get_user(db: &DbConnection, telegram_id: i64) -> Result<Option<User>, DbError>

// OCR operations
pub async fn save_ocr_entry(db: &DbConnection, entry: &OcrEntry) -> Result<i64, DbError>
pub async fn get_ocr_history(db: &DbConnection, user_id: i64) -> Result<Vec<OcrEntry>, DbError>

// Ingredient operations
pub async fn save_ingredients(db: &DbConnection, ingredients: &[Ingredient]) -> Result<(), DbError>
pub async fn get_recipe_ingredients(db: &DbConnection, recipe_id: i64) -> Result<Vec<Ingredient>, DbError>

// Search operations
pub async fn search_ingredients(db: &DbConnection, user_id: i64, query: &str) -> Result<Vec<Ingredient>, DbError>
pub async fn search_recipes(db: &DbConnection, user_id: i64, query: &str) -> Result<Vec<Recipe>, DbError>
```

### Data Models
```rust
pub struct User {
    pub id: i64,
    pub telegram_id: i64,
    pub language_code: String,
    pub created_at: DateTime<Utc>,
}

pub struct OcrEntry {
    pub id: i64,
    pub user_id: i64,
    pub image_path: String,
    pub extracted_text: String,
    pub confidence_score: f32,
    pub processing_time_ms: i32,
    pub created_at: DateTime<Utc>,
}

pub struct Ingredient {
    pub id: i64,
    pub ocr_entry_id: i64,
    pub quantity: Option<f32>,
    pub unit: Option<String>,
    pub name: String,
    pub raw_text: String,
    pub confidence: f32,
}
```

## Implementation Notes

### Key Decisions
- **PostgreSQL Backend**: Robust relational database with FTS
- **User Isolation**: telegram_id based multi-tenancy
- **Auto-sync Triggers**: Automatic FTS index maintenance
- **Connection Pooling**: Efficient connection management
- **Migration System**: Versioned schema changes

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
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### OCR Entries Table
```sql
CREATE TABLE ocr_entries (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT REFERENCES users(id),
    image_path TEXT NOT NULL,
    extracted_text TEXT,
    confidence_score REAL,
    processing_time_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Ingredients Table
```sql
CREATE TABLE ingredients (
    id BIGSERIAL PRIMARY KEY,
    ocr_entry_id BIGINT REFERENCES ocr_entries(id),
    quantity REAL,
    unit VARCHAR(50),
    name TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    confidence REAL DEFAULT 1.0
);
```

## Full-Text Search Implementation

### FTS Configuration
```sql
-- Create FTS trigger function
CREATE OR REPLACE FUNCTION update_fts_index()
RETURNS TRIGGER AS $$
BEGIN
    -- Update search vector with all relevant text
    NEW.search_vector := to_tsvector('english', COALESCE(NEW.extracted_text, ''));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger
CREATE TRIGGER ocr_entries_fts_update
    BEFORE INSERT OR UPDATE ON ocr_entries
    FOR EACH ROW EXECUTE FUNCTION update_fts_index();
```

### Search Queries
```sql
-- Basic text search
SELECT * FROM ocr_entries
WHERE search_vector @@ plainto_tsquery('english', 'chicken breast');

-- Ranked search with relevance
SELECT *, ts_rank(search_vector, query) as rank
FROM ocr_entries, plainto_tsquery('english', 'chicken breast') as query
WHERE search_vector @@ query
ORDER BY rank DESC;
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
- **Analytics**: Usage patterns and insights
- **Backup Strategy**: Automated backup and recovery
- **Caching Layer**: Redis for frequently accessed data
- **Data Export**: Recipe export in various formats
- **User Analytics**: Ingredient usage statistics