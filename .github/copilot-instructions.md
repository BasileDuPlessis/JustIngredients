# Ingredients Telegram Bot - AI Coding Guidelines

## Core Philosophy: Spec-Driven Development

**ALL DEVELOPMENT MUST FOLLOW SPEC-DRIVEN METHODOLOGY**

### What This Means
- **Write Specs First**: Create detailed specifications before implementation
- **Clear Success Criteria**: Every task must have measurable outcomes
- **Incremental Progress**: Break work into 1-hour micro-tasks with validation
- **Quality Gates**: Code must pass all tests, linting, and formatting

### Spec Maintenance Requirements
- **Feature Specs**: Version only for major architecture changes (e.g., `feature-v2.0.md`)
- **Task Specs**: Update in-place for progress tracking (no versioning)
- **Store Location**: `specs/` for features, `specs/tasks/` for implementation tasks
- **Update When**: Architecture changes, new requirements, completed milestones, significant decisions
- **Don't Update For**: Minor implementation details or routine bug fixes
- **Version History**: Document changes within feature spec documents for major updates

### Current Active Specs
- [Ingredient Extraction] `specs/llm-extraction.md` - Gemini vision API, image preparation, errors
- [Telegram Bot] `specs/telegram-bot.md` - User interface and conversation management
- [Database Design] `specs/database-design.md` - Data persistence and full-text search
- [Deployment] `specs/deployment.md` - Infrastructure and production operations
- [UX Improvements] `specs/tasks/ux-improvements.md` - Current UX plan
- [Improvement Backlog] `specs/tasks/improvement-backlog.md` - Verified code-quality items

## Project Overview
Telegram bot that reads the ingredients of recipe photos with the Gemini vision API, lets the user review them, and stores recipes in PostgreSQL.

**Key Features**: Smart measurement detection, multi-language support, real-time processing, user-scoped storage.

## Critical Quality Standards

### ⚠️ ZERO TOLERANCE POLICIES
- **All tests must pass** - Run both `cargo test` AND `cargo test --doc` after every change
- **No `.unwrap()`, `.expect()`, or `panic!`** - Use proper error handling with `?` operator and Result types
- **Specific Error Types** - Use domain-specific error enums instead of generic `String` or `Box<dyn std::error::Error>`
- **Result Propagation** - Functions return `Result<T, E>` with meaningful error types, never panic
- **Test Error Handling** - Test functions return `Result<(), TestError>` and use `?` for error propagation
- **All code must format** - Run `cargo fmt --all -- --check`
- **Zero clippy warnings** - Run `cargo clippy --all-targets --all-features -- -D warnings`

### Development Workflow
1. **Read Active Specs** - Understand requirements and success criteria
2. **Implement Incrementally** - Complete one task at a time
3. **Validate Continuously** - Run tests, linting, formatting after each change
4. **Update Specs** - Mark completed tasks, document decisions

## Architecture Highlights

### Core Modules
- `main.rs` - App entry point with bot dispatcher
- `bot/` - Telegram message handling, dialogue, home message, cleanup, HTML rendering
- `extraction/` - Gemini client, image preparation, guard (concurrency, cache), mapping
- `db.rs` - PostgreSQL operations
- `search.rs` - Recipe search (names and ingredients, French-aware)
- `units.rs` - Canonical units; `validation.rs` - typed ingredient lines and quantities
- `text_processing.rs` - `MeasurementMatch`, the ingredient line type

### Key Patterns
- **Circuit Breaker**: `extraction::breaker` stops calling Gemini after repeated failures
- **Pure renderers**: messages and keyboards are built by pure, unit-tested functions; handlers stay thin
- **Parameter Structs**: Functions >6 params use dedicated structs
- **Async Everywhere**: Tokio runtime with timeouts
- **Error Handling**: Domain-specific error enums with `std::error::Error` implementation
- **Result Propagation**: Use `?` operator for clean error handling, avoid unwrap/expect/panic

### Error Handling Best Practices
- **Never use `unwrap()`, `expect()`, or `panic!`** in production code
- **Create specific error enums** for each module (e.g., `ExtractionError`, `AppError`)
- **Implement `std::error::Error`** with proper `Display` and `source()` methods
- **Test functions return `Result<(), TestError>`** to avoid panics in test code
- **Use `?` operator** for error propagation instead of manual match statements
- **Convert `Option` to `Result`** using `ok_or()` or `ok_or_else()` with meaningful errors

## Environment Setup
```bash
# Required
TELEGRAM_BOT_TOKEN=your_token
DATABASE_URL=postgresql://user:pass@localhost/ingredients

# Build & Test
cargo build && cargo test && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all -- --check
```

## Deployment
- **Fly.io** with automated CI/CD
- **Preflight Checks**: All quality gates must pass
- **Secrets**: Managed via `flyctl secrets` (never in code)

Remember: **Specs drive development, quality gates enforce standards, incremental progress ensures success.**