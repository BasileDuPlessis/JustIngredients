# Deep-Dive Code Review Prompt — JustIngredients

> Use this prompt with an AI assistant (or as a checklist for human reviewers) to perform a thorough code review of the JustIngredients Rust codebase.

---

## System Context

You are an expert Rust engineer performing a deep-dive code review of **JustIngredients**, a Telegram bot that extracts ingredients from recipe images using Tesseract OCR, stores them in PostgreSQL, and exposes a conversational UI via Teloxide. The codebase is approximately **14,000+ lines of Rust** across ~30 source modules, plus ~16 integration tests and ~77 unit tests.

### Key Technology Stack

| Layer | Technology |
|-------|-----------|
| Async runtime | Tokio 1.48 (full features) |
| Telegram framework | Teloxide 0.17 |
| OCR engine | Tesseract via leptess 0.14 |
| Database | PostgreSQL via sqlx 0.8 |
| Observability | tracing 0.1, metrics 0.24, OpenTelemetry 0.31 |
| Serialization | serde / serde_json |
| Internationalization | fluent-bundle 0.16 |
| Image processing | image 0.25, imageproc, ab_glyph |

---

## Review Instructions

Perform a comprehensive, file-by-file code review of this Rust project. For each area below, provide:

1. **Specific findings** — reference exact file paths, function names, and line numbers.
2. **Severity** — classify each finding as **Critical**, **Major**, **Minor**, or **Nit**.
3. **Recommendation** — actionable fix or refactor with code snippets where helpful.
4. **Positive observations** — call out well-designed patterns worth preserving.

Organize your findings by category, then by severity (critical first).

---

## 1. Correctness & Safety

### 1.1 Error Handling Consistency

The codebase uses **four different error strategies** — audit their usage and interactions:

- `anyhow::Result<T>` — general fallible operations
- `AppResult<T>` / `AppError` enum — application-level errors (`src/errors.rs`)
- `OcrError` enum — OCR-specific errors (`src/ocr_errors.rs`)
- `Result<T, &'static str>` — validation functions (`src/validation.rs`)

**Review checklist:**
- [ ] Are `From` impls between error types correct and complete? Do any conversions lose context?
- [ ] Is `Result<T, &'static str>` in `validation.rs` appropriate, or should it use a typed error enum?
- [ ] Are there places where `anyhow::Result` is used but a typed error would be more informative for callers?
- [ ] Is error context (`.context()` / `.with_context()`) applied consistently at module boundaries?
- [ ] Do any error paths silently swallow errors (e.g., `let _ = ...`, `.ok()`, empty `match` arms)?

### 1.2 Unwrap / Expect / Panic Audit

Project policy: **`.unwrap()` is strictly prohibited; `.expect()` is allowed only for mutex poisoning.**

- [ ] Search for all `.unwrap()` calls — are there any violations?
- [ ] Audit all `.expect()` calls (~35 total, concentrated in `cache.rs`, `circuit_breaker.rs`, `instance_manager.rs`, `db.rs`) — do they all involve mutex/RwLock poisoning? Are messages descriptive?
- [ ] Check `text_processing.rs` for `.expect()` on regex captures — these can panic on unexpected input. Are they truly infallible?
- [ ] Search for `panic!()`, `unreachable!()`, `todo!()` — are any reachable in production code?
- [ ] Verify `.unwrap_or()` / `.unwrap_or_else()` / `.unwrap_or_default()` usages provide sensible defaults.

### 1.3 Concurrency & Thread Safety

- [ ] **Cache deadlock risk**: `MemoryCache::get()` in `src/cache.rs` acquires both a write lock (for stats) and a read lock (for data). Is lock ordering consistent across all methods to prevent deadlock?
- [ ] **Circuit breaker TOCTOU**: `is_open()` in `src/circuit_breaker.rs` reads state in one lock scope and potentially resets in another. Can two threads both see half-open state and both attempt reset?
- [ ] **`Arc<Mutex<>>` database connection**: Is a single mutex-protected connection sufficient, or should this be a connection pool (`sqlx::PgPool`)? What happens under concurrent Telegram requests?
- [ ] **`LazyLock` statics** in `src/bot/image_processing.rs` (`OCR_CONFIG`, `OCR_INSTANCE_MANAGER`, `CIRCUIT_BREAKER`) — are these testable? Do they prevent runtime reconfiguration?
- [ ] **`OcrInstanceManager` growth**: Instances are keyed by `"lang:model_dir"` and cached in a `Mutex<HashMap>` but never evicted. Can this grow unbounded?

### 1.4 Resource Management

- [ ] **`std::mem::forget(temp_file)`** in `src/bot/image_processing.rs` — if `TempFileGuard` construction fails between the `forget` and the guard creation, the temp file leaks. Is there a safer pattern?
- [ ] **Temp file cleanup**: Is the `TempFileGuard` RAII pattern reliable in all error paths? What if the process is killed (SIGKILL)?
- [ ] **Database connections**: Are connections properly returned to the pool on error? Is `init_database_schema()` idempotent?
- [ ] **Tesseract instances**: Are they properly cleaned up when the bot shuts down?

---

## 2. Architecture & Design

### 2.1 Module Size & Decomposition

Several files exceed 1,500 lines — review whether they should be split:

| File | Lines | Concern |
|------|-------|---------|
| `src/ocr.rs` | ~2,300 | Mixes validation, HOCR parsing, constrained OCR, confidence scoring |
| `src/text_processing.rs` | ~1,832 | Mixes config loading, regex building, detection, post-processing |
| `src/bot/dialogue_manager.rs` | ~1,663 | 10+ handler functions, parameter structs, database saves |
| `src/db.rs` | ~1,513 | CRUD, schema init, migrations, caching wrappers, FTS |
| `src/error_correction.rs` | ~713 | Correction pipeline, fuzzy matcher, hardcoded dictionaries |
| `src/cache.rs` | ~689 | Generic cache, OCR cache, DB cache, manager, stats |

- [ ] Propose a decomposition plan for each file >800 lines. Which functions/types form natural sub-modules?
- [ ] Is the `src/bot/` directory structure effective? Should `dialogue_manager.rs` be split per dialogue flow?

### 2.2 Configuration Architecture

- [ ] **Duplication**: `main.rs` validates environment variables independently from `AppConfig::from_env()` in `config.rs`. This creates two sources of truth. Should `main.rs` delegate entirely to `AppConfig`?
- [ ] **Static globals vs dependency injection**: OCR config, instance manager, and circuit breaker are `LazyLock` statics. Should they be injected via Teloxide's dependency injection instead?
- [ ] **Measurement config fallback**: `text_processing.rs` silently falls back to empty config if `measurement_units.json` is not found. Should this be a hard error at startup?
- [ ] **Localization bundles**: `LocalizationManager` creates Fluent bundles on every call instead of caching them. What's the performance impact?

### 2.3 State Management

- [ ] **`InMemStorage` for dialogue state**: State is lost on restart. Is this acceptable for production? What's the user impact? Should this migrate to Redis or database-backed storage?
- [ ] **`RecipeDialogueState` enum**: Multiple variants carry duplicated fields (`language_code`, `message_id`, `extracted_text`, `recipe_name_from_caption`). Should a shared `DialogueContext` struct be extracted?

### 2.4 Database Design

- [ ] **Manual cascade deletes** in `db.rs` instead of `ON DELETE CASCADE` foreign keys — why? Is this intentional?
- [ ] **Mixed query styles**: Some queries use `sqlx::query!` (compile-time checked) while others use `sqlx::query` + `.get(i)` (runtime checked). Should this be standardized?
- [ ] **Single connection vs pool**: The codebase wraps a single connection in `Arc<Mutex<>>`. With concurrent Telegram updates, this serializes all DB access. Should this use `sqlx::PgPool`?

---

## 3. Performance

### 3.1 Hot Paths

- [ ] **OCR pipeline**: `extract_text_from_image()` → `perform_constrained_ocr()` is the critical path. Profile the retry logic — 3 retries with 1-10s exponential backoff means up to ~15s worst case on top of the 30s timeout. Is this appropriate?
- [ ] **Regex compilation**: `text_processing.rs` uses `lazy_static!` for compiled patterns — confirm they're compiled once. Are all regex patterns in hot paths pre-compiled?
- [ ] **HOCR parsing via regex** in `ocr.rs`: Would a proper XML parser (e.g., `quick-xml`) be faster and more correct?
- [ ] **Localization bundle creation**: `LocalizationManager` re-parses `.ftl` strings on every message. With high throughput, this adds up.

### 3.2 Memory

- [ ] **Image memory estimation** in `ocr.rs` — is the estimation accurate for all supported formats?
- [ ] **Cache eviction**: `MemoryCache<K,V>` uses TTL but has no max-size cap. `DbQueryCache` has size-based eviction but the `cleanup()` method has a known bug (doesn't update `current_size_bytes`). Audit memory growth under sustained load.
- [ ] **`OcrInstanceManager`** keeps Tesseract instances alive forever. Each instance holds ~20-50MB of trained data in memory. With multiple language combinations, this could exhaust memory.

### 3.3 Unused Dependencies

- [ ] **`parking_lot 0.12.5`** is declared in `Cargo.toml` but all synchronization uses `std::sync::Mutex`/`RwLock`. Remove or migrate?
- [ ] **`rusttype` + `ab_glyph`**: Two font rendering libraries — are both needed? Can one be eliminated?
- [ ] Run `cargo udeps` (if available) or manually audit for other unused dependencies.

---

## 4. Security

### 4.1 Input Validation

- [ ] **Path traversal**: `src/path_validation.rs` is comprehensive. But is it consistently used before all file system operations? Are there any `std::fs` or `tokio::fs` calls that bypass path validation?
- [ ] **Image format validation**: Dual-layer (extension + magic bytes). Are there format confusion attacks possible?
- [ ] **Telegram input sanitization**: Are recipe names, ingredient text, and user inputs sanitized before database insertion? Is there SQL injection risk with the raw `sqlx::query` calls?
- [ ] **Bot token handling**: Is the token logged anywhere (even at debug level)? Check all `tracing::debug!` / `tracing::info!` calls.

### 4.2 Denial of Service

- [ ] **File size limits** (PNG: 15MB, JPEG: 10MB, etc.) — can a user bypass these by sending a malformed Content-Length header?
- [ ] **Memory estimation** — is it possible to craft an image that passes size validation but expands to excessive memory during decoding (e.g., a decompression bomb)?
- [ ] **Rate limiting**: Is there any per-user rate limiting beyond the deduplication check? Can a user flood the bot with images?
- [ ] **Circuit breaker scope**: Is it global (one bad user's images can trip the breaker for everyone)?

### 4.3 Dependency Security

- [ ] Review `config/deny.toml` — is it actively enforced in CI?
- [ ] Run `cargo audit` to check for known vulnerabilities in dependencies.
- [ ] Are there any `unsafe` blocks in the codebase?

---

## 5. Testing

### 5.1 Coverage Gaps

The project has 93 tests (77 unit + 16 integration). Review coverage for:

- [ ] **`src/ocr.rs`** (2,300 lines) — what percentage is covered? Are edge cases for HOCR parsing tested?
- [ ] **`src/bot/dialogue_manager.rs`** — are all 10+ dialogue state transitions tested?
- [ ] **`src/cache.rs`** — are concurrent access patterns tested? Is the deadlock risk scenario covered?
- [ ] **`src/circuit_breaker.rs`** — is the TOCTOU race condition testable?
- [ ] **Error paths**: Are failure scenarios (DB down, OCR timeout, invalid image) tested end-to-end?
- [ ] **Localization**: Are all message keys tested for both English and French?

### 5.2 Test Quality

- [ ] Are integration tests properly isolated (clean DB state, no test interdependencies)?
- [ ] Do tests use realistic data or trivial/synthetic inputs?
- [ ] Are async tests using `#[tokio::test]` correctly?
- [ ] Is there test coverage for the preprocessing pipeline (`src/preprocessing/`)?
- [ ] Are the `bin/` utilities (`generate_training_data.rs`, `train_tesseract.rs`) tested?

### 5.3 Test Infrastructure

- [ ] Is CI running `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` on every PR?
- [ ] Are integration tests run against a real PostgreSQL instance or mocked?
- [ ] Is there a test for the full pipeline: Telegram message → image download → OCR → ingredient extraction → DB storage → search?

---

## 6. Code Quality & Idioms

### 6.1 Rust Idioms

- [ ] **Builder pattern**: Are configuration types using builders where appropriate, or relying on large constructors?
- [ ] **`impl From<X> for Y`** vs **`impl TryFrom<X> for Y`**: Are fallible conversions correctly using `TryFrom`?
- [ ] **Lifetime annotations**: Are there unnecessary lifetime annotations? Could some `&str` parameters be `impl AsRef<str>`?
- [ ] **Clone usage**: Are there unnecessary `.clone()` calls, especially on `String` or `Vec`?
- [ ] **`to_string()` vs `to_owned()` vs `.into()`**: Is string conversion idiomatic and consistent?

### 6.2 API Design

- [ ] Are public APIs minimal? Are internal functions incorrectly marked `pub`?
- [ ] Do functions take the most general parameter types (e.g., `&str` vs `&String`, `&[T]` vs `&Vec<T>`)?
- [ ] Are return types informative enough for callers to handle errors appropriately?
- [ ] Is `HandlerContext<'a>` the right abstraction for passing common state through handlers?

### 6.3 Documentation

- [ ] Are public types and functions documented with `///` doc comments?
- [ ] Are complex algorithms (HOCR parsing, fuzzy matching, constrained OCR) explained in comments?
- [ ] Are `// TODO`, `// FIXME`, `// HACK` comments tracked and addressed?
- [ ] Is `docs/` up to date with the current architecture?

### 6.4 Code Duplication

- [ ] **Config validation**: `main.rs` and `config.rs` both validate environment variables.
- [ ] **Message handler match arms**: The 10+ arm match in `message_handler.rs` has repetitive `DialogueContext` construction. Can this be abstracted?
- [ ] **Error logging**: `error_logging` module in `errors.rs` has many similar `log_*_error()` functions. Could a macro reduce this?
- [ ] **Cached DB functions**: The `*_cached()` wrappers in `db.rs` follow an identical pattern. Could a `#[cached]` macro or generic wrapper reduce boilerplate?

---

## 7. Observability & Operability

- [ ] **Health checks**: Are liveness/readiness probes properly implemented? Do they check real dependencies (DB connectivity, Tesseract availability)?
- [ ] **Metrics completeness**: Are key operations instrumented (OCR latency, DB query time, cache hit rates, Telegram API errors)?
- [ ] **Structured logging**: Is `tracing` used consistently with span context? Are log levels appropriate?
- [ ] **Graceful shutdown**: Does the bot handle SIGTERM cleanly (finish in-progress operations, close DB connections)?
- [ ] **Alerting**: Are Prometheus alerting rules defined for critical thresholds (OCR failure rate, DB connection loss)?

---

## 8. Deployment & Operations

- [ ] **Dockerfile** (`deploy/Dockerfile`): Is it a multi-stage build? Are layers optimized for caching? Is the final image minimal?
- [ ] **Fly.io configuration**: Are resource limits (memory, CPU) appropriate for Tesseract OCR workloads?
- [ ] **Database migrations**: Is `init_database_schema()` safe to run on an existing database? Are migrations idempotent?
- [ ] **Secrets management**: Are all secrets loaded from environment variables? Is `.env` in `.gitignore`?
- [ ] **Rollback strategy**: Can a failed deployment be rolled back without data loss?

---

## Deliverables

After completing the review, provide:

1. **Executive Summary** — 3-5 paragraph overview of codebase health, top risks, and strategic recommendations.
2. **Findings Table** — all findings sorted by severity with file references and recommended actions.
3. **Architecture Diagram** — (optional) Mermaid diagram of the current module dependency graph showing coupling hotspots.
4. **Prioritized Action Plan** — top 10 improvements ranked by risk-reduction-to-effort ratio.
5. **Positive Patterns** — design decisions worth preserving and replicating.
