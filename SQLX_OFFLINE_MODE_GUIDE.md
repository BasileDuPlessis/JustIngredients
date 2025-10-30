# SQLx Offline Mode Implementation Guide

## Overview

This guide provides step-by-step instructions to implement SQLx offline mode for the JustIngredients project. Offline mode enables compile-time query validation without requiring a database connection during builds, making CI/CD pipelines faster and more reliable.

## Why Offline Mode?

### Benefits
- ✅ **No CI database required** - Builds work without external database dependencies
- ✅ **Faster builds** - No network calls to validate queries
- ✅ **Compile-time SQL validation** - Catches SQL errors during compilation
- ✅ **Reliable CI/CD** - No database connection failures in automated builds
- ✅ **Schema versioning** - Query metadata is versioned with your code

### Current Issues Solved
- Manual `cargo sqlx prepare` maintenance
- CI failures due to database connectivity issues
- Stale query metadata in `.sqlx/` directory

## Prerequisites

1. **SQLx CLI installed**:
   ```bash
   cargo install sqlx-cli
   ```

2. **Database schema file** (create if not exists):
   ```bash
   # Export your current schema
   pg_dump --schema-only --no-owner your_database > schema.sql
   ```

3. **Current project state**: Working application with existing `.sqlx/` directory

## Implementation Steps

### Step 1: Update Cargo.toml Configuration

Add the `offline` feature to your SQLx dependency:

```toml
[dependencies.sqlx]
version = "0.8"
features = [
    "postgres",
    "runtime-tokio-native-tls",
    "chrono",
    "offline"  # Add this line
]
```

### Step 2: Create Schema File

Generate a schema file from your current database:

```bash
# Connect to your development database and export schema
pg_dump --schema-only --no-owner --clean your_database_name > schema.sql

# Or if using a connection string:
pg_dump --schema-only --no-owner "postgresql://user:pass@localhost/justingredients" > schema.sql
```

**Important**: Ensure your schema file includes:
- All tables (users, recipes, ingredients)
- All indexes and constraints
- Any custom types or functions

### Step 3: Regenerate .sqlx Directory in Offline Mode

```bash
# Remove current .sqlx directory
rm -rf .sqlx/

# Generate new .sqlx with offline mode
cargo sqlx prepare -- --schema schema.sql

# Verify the .sqlx directory was created
ls -la .sqlx/
```

### Step 4: Update .gitignore

**Remove** `.sqlx/` from `.gitignore` (if present) since we'll commit it:

```gitignore
# Keep other entries, remove this line if it exists:
# .sqlx/
```

### Step 5: Commit Schema and .sqlx Files

```bash
# Add the files to git
git add schema.sql .sqlx/

# Commit with descriptive message
git commit -m "Implement SQLx offline mode

- Add schema.sql with current database schema
- Regenerate .sqlx/ directory for offline query validation
- Enable compile-time SQL validation without database connection"
```

### Step 6: Update CI/CD Pipeline

Modify `build-test.yml` to use offline mode:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4

    - name: Setup Rust
      uses: dtolnay/rust-toolchain@stable

    - name: Install system dependencies
      run: sudo apt-get update && sudo apt-get install -y libleptonica-dev libtesseract-dev tesseract-ocr

    - name: Cache dependencies
      uses: Swatinem/rust-cache@v2

    # Add offline mode preparation
    - name: Verify SQLx queries (offline mode)
      run: cargo sqlx prepare --check -- --schema schema.sql

    - name: Build
      run: cargo build

    - name: Run tests
      run: cargo test --lib -- --test-threads=8
```

### Step 7: Test the Implementation

1. **Local testing**:
   ```bash
   # Build should work without database connection
   cargo build

   # Run tests (these will still need database)
   cargo test
   ```

2. **CI/CD testing**:
   - Push changes to trigger build pipeline
   - Verify build succeeds without database connection errors
   - Check that SQL validation happens at compile time

## Configuration Details

### Environment Variables

Set these for offline mode:

```bash
# In your application environment
SQLX_OFFLINE=true  # Optional, automatically detected

# Keep DATABASE_URL for runtime connections
DATABASE_URL=postgresql://user:pass@host/db
```

### Build vs Runtime

- **Build time**: Uses `schema.sql` and `.sqlx/` for query validation
- **Runtime**: Uses `DATABASE_URL` for actual database connections
- **Tests**: Can use either mode depending on your test setup

## Maintenance Workflow

### When Schema Changes

1. **Update schema.sql**:
   ```bash
   pg_dump --schema-only --no-owner your_database > schema.sql
   ```

2. **Regenerate .sqlx**:
   ```bash
   cargo sqlx prepare -- --schema schema.sql
   ```

3. **Test compilation**:
   ```bash
   cargo build  # Should catch SQL/schema mismatches
   ```

4. **Commit changes**:
   ```bash
   git add schema.sql .sqlx/
   git commit -m "Update database schema and SQLx queries"
   ```

### When Adding New Queries

1. **Add query to code** (using `sqlx::query!` for compile-time validation)
2. **Run preparation**:
   ```bash
   cargo sqlx prepare -- --schema schema.sql
   ```
3. **Test compilation**:
   ```bash
   cargo build
   ```

## Troubleshooting

### Common Issues

**1. "query is not prepared" error**
```
error: query is not prepared: SELECT * FROM users WHERE id = $1
```
**Solution**: Run `cargo sqlx prepare -- --schema schema.sql`

**2. Schema mismatch**
```
error: schema mismatch: column 'new_column' not found
```
**Solution**: Update `schema.sql` and regenerate `.sqlx/`

**3. Offline mode not working**
```
error: SQLx offline mode is not enabled
```
**Solution**: Add `"offline"` feature to `Cargo.toml`

### Verification Commands

```bash
# Check if offline mode is enabled
cargo tree | grep sqlx

# Verify .sqlx directory contents
ls -la .sqlx/

# Test compilation without database
SQLX_OFFLINE=true cargo build

# Check schema file validity
head -20 schema.sql
```

## Migration Path

### From Current Setup

1. ✅ You already have `.sqlx/` directory
2. ✅ You have working queries
3. 🔄 Follow steps 1-5 above
4. 🔄 Test thoroughly before deploying

### Gradual Adoption

- Start with offline mode for CI/CD
- Keep runtime database connections unchanged
- Gradually migrate dynamic queries to compile-time queries

## Benefits Summary

| Aspect | Before (Online) | After (Offline) |
|--------|----------------|-----------------|
| CI Database | Required ❌ | Not needed ✅ |
| Build Speed | Slower (network calls) | Faster ✅ |
| Reliability | Database-dependent | Database-independent ✅ |
| SQL Validation | Runtime | Compile-time ✅ |
| Maintenance | Manual `sqlx prepare` | Automated ✅ |

## Next Steps

1. Follow the implementation steps above
2. Test locally and in CI/CD
3. Consider migrating to `sqlx::query!` for compile-time query validation
4. Update your development workflow documentation

## Additional Resources

- [SQLx Offline Mode Documentation](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#offline-mode)
- [SQLx Compile-time Verification](https://github.com/launchbadge/sqlx#compile-time-verification)
- [PostgreSQL pg_dump Documentation](https://www.postgresql.org/docs/current/app-pgdump.html)