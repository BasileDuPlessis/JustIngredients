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
- [OCR Processing] `specs/ocr-processing.md` - Text extraction and ingredient parsing
- [Telegram Bot] `specs/telegram-bot.md` - User interface and conversation management
- [Database Design] `specs/database-design.md` - Data persistence and full-text search
- [Deployment] `specs/deployment.md` - Infrastructure and production operations
- [OCR Refactor Strategy] `specs/tasks/ocr-refactor-strategy.md` - Current modularization work (Tasks 3-4 completed)

## Project Overview
Telegram bot that extracts ingredients from recipe images using OCR, stores in PostgreSQL with full-text search.

**Key Features**: Smart measurement detection, multi-language support, real-time processing, user-scoped storage.

## Critical Quality Standards

### ⚠️ ZERO TOLERANCE POLICIES
- **All 135 tests must pass** - Run `cargo test` after every change
- **No `.unwrap()` or panics** - Use proper error handling with `?` operator
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
- `bot.rs` - Telegram message handling and dialogue management
- `ocr.rs` - OCR processing (currently being refactored into submodules)
- `db.rs` - PostgreSQL operations with FTS
- `text_processing.rs` - Ingredient parsing and measurement detection

### Key Patterns
- **Circuit Breaker**: Prevents OCR failure cascades
- **Instance Pooling**: Reuses Tesseract instances for performance
- **Parameter Structs**: Functions >6 params use dedicated structs
- **Async Everywhere**: Tokio runtime with timeouts

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