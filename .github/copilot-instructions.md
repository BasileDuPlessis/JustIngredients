# Ingredients Telegram Bot - AI Coding Guidelines

## Project Overview
This is a Telegram bot that extracts text from images using OCR (Optical Character Recognition). It processes
 photos and image files sent by users, extracts text using Tesseract, and stores the results in a PostgreSQL database with full-text search capabilities.

## Key Features

### Ingredient Extraction & Processing
- **Smart Measurement Detection**: Recognizes quantities, units, and ingredient names from recipe images
- **Multi-Language Support**: Handles both English and French measurement units (cups, grams, liters, etc.)
- **Quantity-Only Ingredients**: Detects items like "6 eggs" or "4 apples" without explicit units
- **Fraction Support**: Processes fractional quantities (½ cup, ¾ teaspoon, ⅓ liter)
- **Unicode Fraction Support**: Handles Unicode fraction characters (¼, ½, ¾, ⅓, ⅔, etc.)
- **Comprehensive Unit Recognition**: Supports volume (cups, tablespoons, liters), weight (grams, pounds, kg)
, and count units (pieces, slices, cans)
- **Advanced Text Processing**: Regex-based pattern matching with case-insensitive and accent-insensitive mat
ching
- **Ingredient Name Extraction**: Intelligent parsing of ingredient names from measurement text
- **Post-processing**: Automatic correction of common OCR errors and ingredient name normalization

### Telegram Bot Interface
- **Photo Processing**: Accepts photos sent directly in Telegram chats
- **Document Support**: Handles image files uploaded as documents (PNG, JPEG, BMP, TIFF)
- **Photo Caption Support**: Automatically uses photo captions as recipe name candidates with validation
- **Caption Validation**: Intelligent fallback to "Recipe" for invalid/empty captions
- **Real-Time Feedback**: Provides processing status updates and formatted results
- **Multi-Language UI**: Localized responses in English and French based on user preferences
- **Error Recovery**: Graceful handling of processing failures with user-friendly messages
- **Dialogue System**: Interactive recipe name input with validation and state management
- **Workflow Transitions**: Post-confirmation workflow with action buttons (Add Another, List Recipes, Search
)
- **Confirmation Flow**: Clear user experience after ingredient validation with next-action choices
- **Command Support**: /start, /help commands with comprehensive user guidance

### Data Persistence & Search
- **User-Scoped Storage**: Each user's ingredients are isolated and searchable
- **Full-Text Search**: PostgreSQL FTS enables searching through all extracted text
- **Structured Ingredient Data**: Stores parsed quantity, unit, name, and raw text for each ingredient
- **OCR History**: Maintains complete history of processed images and extracted content
- **Recipe Organization**: Groups ingredients by recipe names for better organization
- **Database Schema**: Three main tables (users, ocr_entries, ingredients) with proper relationships

### Reliability & Performance
- **Circuit Breaker Protection**: Prevents system overload during OCR failures with configurable thresholds
- **Instance Pooling**: Reuses Tesseract instances for faster processing (eliminates 100-500ms initialization
 overhead)
- **Format Validation**: Pre-validates image formats and sizes before processing (PNG: 15MB, JPEG: 10MB, BMP:
 5MB, TIFF: 20MB)
- **Resource Management**: Automatic cleanup of temporary files and connection pooling
- **Timeout Protection**: 30-second timeouts on OCR operations to prevent hanging
- **Memory Estimation**: Pre-calculates memory usage before processing large images
- **Retry Logic**: Exponential backoff with jitter (3 retries, 1s-10s delays)

### Testing & Quality Assurance
- **Comprehensive Test Suite**: 93 total tests covering all functionality
  - Unit Tests: 77 tests across core modules
  - Integration Tests: 16 tests for end-to-end functionality
- **Test Organization**: Proper separation of unit tests (src/) and integration tests (tests/)
- **Database Testing**: In-memory database testing with proper isolation
- **OCR Testing**: Mocked OCR operations for reliable testing
- **Localization Testing**: Multi-language support validation
- **Circuit Breaker Testing**: Failure and recovery scenario testing

## Architecture Components

### Core Modules
- **`main.rs`**: Application entry point, initializes services and starts the bot dispatcher with dialogue su
pport
- **`bot.rs`**: Telegram message handling, image download/processing, user interaction logic, dialogue manage
ment
- **`ocr.rs`**: OCR processing with Tesseract, circuit breaker pattern, format validation, instance managemen
t, memory estimation
- **`db.rs`**: PostgreSQL database operations with FTS (Full-Text Search) support, schema initialization, CRU
D operations
- **`dialogue.rs`**: Recipe dialogue state management, validation, and user interaction flow
- **`text_processing.rs`**: Advanced text processing with regex patterns, measurement detection, ingredient extraction (loads patterns from config/measurement_units.json)
- **`localization.rs`**: Internationalization using Fluent bundles (English/French)
- **`ocr_config.rs`**: Configuration structures for OCR settings, recovery, and format limits
- **`ocr_errors.rs`**: Custom error types for OCR operations with proper error handling
- **`circuit_breaker.rs`**: Circuit breaker implementation for fault tolerance
- **`instance_manager.rs`**: OCR instance pooling and management for performance optimization

### Key Dependencies
- **teloxide**: Telegram bot framework with async support
- **leptess**: Tesseract OCR Rust bindings with instance management
- **sqlx**: PostgreSQL database access with compile-time query checking
- **fluent-bundle**: Internationalization framework with Fluent syntax
- **tokio**: Async runtime with timeout and concurrency support
- **anyhow**: Error handling with context and chaining
- **regex**: Advanced regular expressions for text processing
- **tempfile**: Secure temporary file handling
- **image**: Image format detection and validation

## Critical Developer Workflows

### Environment Setup
```bash
# Required environment variables
TELEGRAM_BOT_TOKEN=your_bot_token_here
DATABASE_URL=postgresql://username:password@localhost/ingredients

# Optional
HEALTH_PORT=8080
LOG_FORMAT=json|pretty
RUST_LOG=debug,sqlx=warn
```

### Build & Run
```bash
cargo build                    # Debug build
cargo build --release         # Optimized build
cargo run                     # Run with environment variables
cargo test                    # Run complete test suite (93 tests)
```

### Database Management
- Schema auto-initializes on startup via `db::init_database_schema()`
- **Database Design**: PostgreSQL with three main tables:
  - `users`: User management with language preferences
  - `ocr_entries`: OCR processing history with full-text search
  - `ingredients`: Parsed ingredient data with recipe grouping
- Triggers maintain FTS table synchronization automatically
- Connection pooling with `Arc<Mutex<>>` for thread safety

## Project-Specific Patterns & Conventions

### OCR Processing Architecture
- **Circuit Breaker Pattern**: Prevents cascading failures during OCR operations
  - Configured in `RecoveryConfig` with threshold and reset timeout
  - Records failures/successes to protect system stability
- **Instance Reuse**: OCR instances cached by language combination for performance
  - Managed by `OcrInstanceManager` with `Arc<Mutex<LepTess>>`
  - Eliminates Tesseract initialization overhead (~100-500ms per instance)
- **Format-Specific Validation**: Different size limits per image format
  - PNG: 15MB, JPEG: 10MB, BMP: 5MB, TIFF: 20MB
  - Memory usage estimation before processing
- **Retry Logic**: Exponential backoff with jitter (3 retries, 1s-10s delays)

### Error Handling Strategy
- **Custom Error Types**: `OcrError` enum with specific variants (Validation, Timeout, Initialization, etc.)
- **User-Friendly Messages**: Localized error responses via Fluent bundles
- **Graceful Degradation**: Circuit breaker prevents system overload
- **Resource Cleanup**: Temporary files always removed after processing
- **Context Preservation**: Error chaining with `anyhow::Context` for debugging

### Database Design
- **Three-Tier Schema**: Separate tables for users, OCR entries, and ingredients
- **Auto-Sync Triggers**: INSERT/UPDATE/DELETE triggers maintain FTS index
- **User Isolation**: All queries filtered by `telegram_id` for multi-tenancy
- **Full-Text Search**: PostgreSQL tsvector with GIN indexes for performance
- **Recipe Grouping**: Ingredients linked to recipes for organization

### Internationalization Approach
- **Fluent Framework**: Uses `.ftl` files in `locales/{lang}/main.ftl`
- **Language Detection**: Based on Telegram `user.language_code`
- **Fallback Strategy**: Unsupported languages default to English
- **Message Keys**: Descriptive keys like `error-ocr-timeout`, `success-extraction`
- **Plural Support**: Proper pluralization handling in both languages

### Async Patterns
- **Shared State**: Database connection wrapped in `Arc<Mutex<>>` for thread safety
- **Timeout Protection**: 30-second timeouts on OCR operations using `tokio::time::timeout`
- **Background Processing**: Image downloads and OCR run asynchronously
- **Dialogue State**: Persistent conversation state using `InMemStorage`

### Code Refactoring Patterns
- **Parameter Structs**: Functions with >6 parameters use dedicated parameter structs for maintainability
  - Common structs: `DialogueContext` (bot, message, dialogue, localization)
  - Function-specific structs: `RecipeNameInputParams`, `IngredientEditInputParams`, etc.
  - Reduces complexity from 8-11 parameters to 2-3 parameters
- **Workflow Transitions**: Post-confirmation UX with action buttons replacing edit/delete options
  - `create_post_confirmation_keyboard()` provides Add Another/List Recipes/Search options
  - Improves user flow after ingredient validation
- **Type Safety**: Parameter structs ensure compile-time validation of required fields

### Testing Patterns
- **Unit Tests**: Pure logic testing without external dependencies
- **Integration Tests**: Database and OCR operations with proper setup/teardown
- **Mock Data**: Temporary files and in-memory databases for testing
- **Test Isolation**: Each test runs in isolation with clean state
- **Async Testing**: Proper async test handling with `tokio::test`

## Integration Points & External Dependencies

### Telegram Bot API
- **Message Types**: Handles text, photos, documents, unsupported messages
- **File Downloads**: Downloads via Telegram API with authentication and size limits
- **User Context**: Extracts language codes for localization
- **Dialogue System**: State management for multi-step conversations

### OCR Engine (Tesseract)
- **Language Support**: Configured for English + French (`eng+fra`)
- **Image Formats**: PNG, JPEG, BMP, TIFF with format detection
- **Performance Optimization**: Instance pooling and reuse
- **Error Recovery**: Circuit breaker protection against OCR failures

### File System Operations
- **Temporary Files**: Images downloaded to temp files, always cleaned up
- **Path Handling**: Absolute paths required for Tesseract operations
- **Security**: Secure temporary file creation with `tempfile` crate

## Code Quality Standards

### ⚠️ CRITICAL: Testing Requirements
**CODE MUST PASS ALL TESTS AT ALL TIMES - NO EXCEPTIONS**

- **Zero Test Failures**: All 93 tests must pass before any code changes are committed
- **Continuous Testing**: Run `cargo test` after every significant change
- **Test-First Development**: Write tests before implementing new features
- **Regression Prevention**: Tests catch breaking changes immediately
- **Quality Gate**: Code that fails tests cannot be merged or deployed

### Testing Approach
- **Unit Tests**: Comprehensive coverage for all modules (77 tests)
  - Pure logic testing without external dependencies
  - Fast execution for rapid development feedback
- **Integration Tests**: End-to-end functionality (16 tests)
  - Database operations with proper transaction isolation
  - OCR validation with mocked external services
  - Full pipeline testing from input to output
- **Mock Data**: Temporary files and in-memory databases for testing
- **Async Testing**: Proper handling of async operations with `tokio::test`
- **Database Testing**: Isolated test databases with automatic cleanup

### Security Considerations
- **Input Validation**: File size limits, format restrictions, path traversal protection
- **Resource Limits**: Memory estimation, timeout protection, connection pooling
- **Audit Configuration**: `deny.toml` for dependency security scanning
- **Temporary File Security**: Secure file creation and automatic cleanup
- **Secret Hygiene**: Never hard-code API keys, passwords, or tokens; keep `.env` files out of git via `.gitignore`; add placeholders to `.env.example` instead
- **GitHub Secrets**: Store deployment keys, database passwords, and OAuth tokens in GitHub repository or environment secrets; avoid printing them in CI logs; rotate credentials that are exposed or outdated
- **Fly Secrets**: Use `flyctl secrets set KEY=VALUE` (or GitHub Actions `flyctl secrets import`) to manage runtime credentials; do not place secrets in `fly.toml`, commit history, or Fly release notes

### Performance Optimizations
- **Connection Reuse**: Single database connection shared across requests
- **Instance Caching**: OCR instances reused to reduce initialization time
- **Format Pre-validation**: Quick rejection of unsupported/oversized files
- **Memory Management**: Pre-calculation of memory requirements
- **Async Processing**: Non-blocking operations for scalability

## Linting, Formatting, and Code Review Standards

### ⚠️ CRITICAL: Code Quality Enforcement
**ALL CODE MUST PASS LINTING AND FORMATTING CHECKS - NO EXCEPTIONS**

- **Clippy Enforcement**: All code must pass `cargo clippy --all-targets --all-features -- -D warnings`
  - Treats all warnings as errors for maximum code quality
  - Only add Clippy allow attributes when strictly justified and document the rationale inline
  - Common allowed lints: `too_many_arguments` for database functions
- **Rustfmt Enforcement**: All code must be formatted with `rustfmt`
  - Run `cargo fmt --all -- --check` to verify formatting
  - CI rejects PRs with formatting issues
- **CI Integration**: PRs are automatically rejected if they fail:
  - `cargo test` (all 93 tests must pass)
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo fmt --all -- --check`
- **Copilot and AI Contributions**: AI-generated code must meet ALL quality standards
  - No exceptions for AI-generated code
  - Must pass all tests, linting, and formatting checks
  - Follow established patterns and conventions

## Common Development Tasks

### Adding New Features
1. **Write Tests First**: Create comprehensive tests before implementation
2. **Database Changes**: Update schema in `db.rs`, add migration logic, update tests
3. **New Commands**: Add handlers in `bot.rs` message processing logic with tests
4. **OCR Enhancements**: Modify `ocr.rs` with new validation or processing logic and tests
5. **Localization**: Add keys to `.ftl` files, update `localization.rs` if needed, test translations
6. **Text Processing**: Update patterns in `config/measurement_units.json` and `text_processing.rs`, add tests for new cases
7. **UI/UX Improvements**: Add workflow transitions and keyboard functions in `dialogue_manager.rs`
8. **Function Refactoring**: Use parameter structs for functions with >6 parameters to maintain code quality

### Debugging Issues
- **OCR Failures**: Check circuit breaker state, Tesseract logs, temp file cleanup, test isolation
- **Database Issues**: Verify FTS triggers, connection sharing, schema initialization, test database state
- **Localization**: Confirm language detection, Fluent bundle loading, test translations
- **Performance**: Monitor instance pooling, memory usage, timeout configurations
- **UI Flow Issues**: Check workflow transitions and keyboard creation in dialogue handlers

### Deployment Considerations
- **Environment Variables**: Secure token storage, database path configuration
- **Resource Limits**: Monitor memory usage, file system space for temp files
- **Monitoring**: Circuit breaker metrics, OCR success/failure rates, test coverage
- **Health Checks**: Database connectivity, OCR instance availability, localization loading
- **Credential Audits**: Before deploying, re-check that no secrets are staged in git, console logs, or shared links

### Fly.io Deployment Workflow
- **Preflight Checks**: Run `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo fmt --all -- --check` before triggering `fly deploy`
- **Secrets Management**: Prefer `flyctl secrets import < .env.production` for batch updates; confirm the resulting release shows `Secrets are staged for release` and avoid copying secrets into the terminal history
- **Deployment Automation**: When using GitHub Actions, authenticate via a short-lived Fly access token stored in `F fly-api-token`; revoke and rotate tokens after audits or role changes
- **Post-Deploy Verification**: Use `fly status`, `fly logs -a just-ingredients`, and the Prometheus/Grafana dashboards to confirm the app is healthy; roll back immediately if OCR errors spike
- **Incident Response**: If a password or token leaks, revoke it in Fly/GitHub, rotate database credentials, and update `TELEGRAM_BOT_TOKEN` via `flyctl secrets set` before redeploying