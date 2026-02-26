# Codebase Improvement Tasks - JustIngredients

Based on the deep-dive code review, this document outlines prioritized tasks to improve the JustIngredients codebase. Tasks are ranked by risk-reduction-to-effort ratio, focusing on maintainability, performance, and reliability.

## High Priority (Critical/Major Issues)

### 1. Decompose Large Modules
**Effort**: 1-2 weeks  
**Rationale**: Four modules exceed 1500 lines, violating single-responsibility principle and making maintenance difficult.  
**Files**: `src/ocr.rs` (2300 lines), `src/text_processing.rs` (1832 lines), `src/bot/dialogue_manager.rs` (1663 lines), `src/db.rs` (1513 lines)

**Tasks**:
- Split `src/ocr.rs` into:
  - `src/ocr/validation.rs` - Input validation and format checking
  - `src/ocr/hocr.rs` - HOCR parsing and XML processing
  - `src/ocr/processing.rs` - Core OCR logic and instance management
- Split `src/text_processing.rs` into:
  - `src/text_processing/config.rs` - Configuration loading
  - `src/text_processing/patterns.rs` - Regex compilation and pattern management
  - `src/text_processing/detection.rs` - Ingredient detection logic
- Split `src/bot/dialogue_manager.rs` into:
  - `src/bot/dialogue/handlers.rs` - Individual dialogue handlers
  - `src/bot/dialogue/state.rs` - State management
  - `src/bot/dialogue/validation.rs` - Input validation
- Split `src/db.rs` into:
  - `src/db/operations.rs` - CRUD operations
  - `src/db/cache.rs` - Caching logic
  - `src/db/migrations.rs` - Schema management

### 2. Eliminate Configuration Duplication
**Effort**: 2-3 days  
**Rationale**: `main.rs` duplicates environment validation logic from `AppConfig::from_env()`, creating maintenance overhead.  
**Files**: `src/main.rs`, `src/config.rs`

**Tasks**:
- Remove `validate_environment_variables()` from `main.rs`
- Replace with direct call to `AppConfig::from_env()` with proper error handling
- Ensure all validation logic is centralized in `config.rs`
- Update error messages for consistency

### 3. Implement Persistent Dialogue Storage
**Effort**: 3-5 days  
**Rationale**: `InMemStorage` loses all user conversations on restart, creating poor UX for multi-step recipe input.  
**Files**: `src/main.rs`, dialogue-related modules

**Tasks**:
- Add Redis dependency to `Cargo.toml`
- Replace `InMemStorage` with `RedisStorage` in `main.rs`
- Add Redis connection configuration to `AppConfig`
- Implement graceful fallback to `InMemStorage` if Redis unavailable
- Add migration documentation for existing users

## Medium Priority (Performance/Architecture Improvements)

### 4. Optimize Observability Overhead
**Effort**: 2-3 days  
**Rationale**: Performance tests show 63.6% overhead for medium-complexity OCR operations, potentially impacting throughput.  
**Files**: `src/ocr.rs`, observability modules

**Tasks**:
- Make tracing levels configurable per operation type
- Add conditional tracing based on `tracing::enabled!()`
- Profile and optimize hot paths in OCR processing
- Consider async metrics recording to avoid blocking

### 5. Add Database Cascade Deletes
**Effort**: 1-2 days  
**Rationale**: Foreign key constraints lack `ON DELETE CASCADE`, requiring manual cleanup and increasing error risk.  
**Files**: Database schema, `src/db.rs`

**Tasks**:
- Add `ON DELETE CASCADE` to foreign keys in schema migration
- Update manual delete logic in `db.rs` to rely on cascade
- Test cascade behavior with integration tests
- Document schema changes for deployment

### 6. Replace Static Globals with Dependency Injection
**Effort**: 1 week  
**Rationale**: OCR config, instance manager, and circuit breaker are `LazyLock` statics, complicating testing and runtime reconfiguration.  
**Files**: `src/bot/image_processing.rs`, `src/main.rs`

**Tasks**:
- Create a `ProcessingContext` struct containing shared resources
- Pass context through function parameters instead of static access
- Update all callers to accept context parameter
- Enable runtime reconfiguration and better testability

## Low Priority (Quality of Life Improvements)

### 7. Add Comprehensive Test Coverage Metrics
**Effort**: 3-5 days  
**Rationale**: Core modules lack documented coverage metrics; unknown test coverage for critical paths.  
**Files**: Test files, CI configuration

**Tasks**:
- Set up `cargo-tarpaulin` for coverage reporting
- Add coverage badges to README
- Identify and test uncovered HOCR parsing edge cases
- Add integration tests for OCR failure scenarios

### 8. Implement Health Checks for OCR Dependencies
**Effort**: 2-3 days  
**Rationale**: No liveness/readiness probes for Tesseract availability or database connectivity.  
**Files**: `src/main.rs`, observability modules

**Tasks**:
- Add `/health` HTTP endpoint checking:
  - Database connectivity
  - Tesseract initialization
  - Redis availability (if implemented)
- Integrate with existing Prometheus metrics
- Add health check to Docker health checks

### 9. Standardize Parameter Structs for Complex Functions
**Effort**: 1 week  
**Rationale**: Some functions have 8+ parameters, reducing readability and maintainability.  
**Files**: `src/bot/dialogue_manager.rs`, related handlers

**Tasks**:
- Identify functions with >6 parameters
- Create parameter structs (e.g., `DialogueContext`, `ProcessingParams`)
- Refactor function signatures to use structs
- Update all call sites

### 10. Add Rollback Strategy Documentation
**Effort**: 1 day  
**Rationale**: No documented rollback strategy for failed deployments.  
**Files**: `docs/deployment.md`, Fly.io configuration

**Tasks**:
- Document Fly.io rollback commands
- Add database migration rollback procedures
- Include health check verification steps
- Add rollback testing checklist

## Implementation Guidelines

### Testing Requirements
- All changes must maintain 100% test pass rate
- Add tests for new modules/functions
- Run `cargo clippy --all-targets --all-features -- -D warnings` before commit
- Run `cargo fmt --all` for formatting

### Deployment Considerations
- Schema changes require migration scripts
- Static global removal may affect startup performance
- Module decomposition requires updating import statements
- Redis addition requires infrastructure changes

### Risk Assessment
- **High Risk**: Module decomposition (affects many files)
- **Medium Risk**: Static global removal (affects testing)
- **Low Risk**: Configuration cleanup, documentation updates

## Success Metrics

- Reduce largest module size to <1000 lines
- Eliminate all `.unwrap()` and panic usage in production code
- Achieve >90% test coverage on critical paths
- Reduce OCR processing overhead to <50%
- Implement zero-downtime deployment capability

## Timeline Estimate

- **Phase 1 (Weeks 1-2)**: Configuration cleanup, cascade deletes, health checks
- **Phase 2 (Weeks 3-6)**: Module decomposition, parameter structs
- **Phase 3 (Weeks 7-8)**: Static global removal, persistent storage
- **Phase 4 (Weeks 9-10)**: Performance optimization, testing improvements</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/JustIngredients/docs/codebase_improvement_tasks.md