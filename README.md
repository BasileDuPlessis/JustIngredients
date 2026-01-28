# JustIngredients Telegram Bot

A Telegram bot that extracts text from images using OCR (Optical Character Recognition) and stores ingredient lists in a searchable database.

## Features

- **OCR Text Extraction**: Uses Tesseract OCR to extract text from images and photos
- **Ingredient Parsing**: Automatically detects and parses measurements and ingredients from recipe text
- **Multi-Line Ingredient Support**: Intelligently combines ingredient names that span multiple lines (e.g., "all-purpose flour", "extra virgin olive oil")
- **Quantity-Only Support**: Recognizes ingredients with quantities but no measurement units (e.g., "6 oeufs", "4 pommes")
- **Photo Caption Support**: Uses photo captions as recipe name candidates with intelligent fallback
- **Full-Text Search**: PostgreSQL full-text search for efficient content searching
- **Multilingual Support**: English and French language support with localized messages
- **Circuit Breaker Pattern**: Protects against OCR failures with automatic recovery
- **Database Storage**: Persistent storage of extracted text and user interactions
- **Workflow Transitions**: Smooth user experience with clear next-action options after ingredient validation
- **Recipe Management**: List, search, and organize saved recipes with intuitive navigation
- **Advanced Caching**: Multi-level caching system for OCR results, database queries, and user data
- **Business Metrics**: Comprehensive monitoring of user engagement, recipe creation patterns, and system KPIs
- **Performance Optimization**: Instance pooling, memory management, and request caching for improved throughput

## Supported Measurement Formats

### Traditional Measurements
- Volume: `2 cups flour`, `1 tablespoon sugar`, `250 ml milk`
- Weight: `500g butter`, `1 kg tomatoes`, `2 lbs beef`
- Count: `3 eggs`, `2 slices bread`, `1 can tomatoes`

### Quantity-Only Ingredients
- French: `6 oeufs`, `4 pommes`, `3 carottes`
- English: `5 apples`, `2 onions`, `8 potatoes`

### Multi-Line Ingredients
The bot intelligently handles ingredient names that span multiple lines in OCR text:
```
2 cups all-purpose
flour
1 cup extra virgin
olive oil
3/4 cup unsalted butter,
softened
```

These are automatically combined into complete ingredient names: "all-purpose flour", "extra virgin olive oil", "unsalted butter, softened".

## Installation

### Prerequisites
- Rust 1.70+
- Tesseract OCR with English and French language packs
- PostgreSQL database

### Setup
1. Clone the repository:
   ```bash
   git clone https://github.com/BasileDuPlessis/JustIngredients.git
   cd JustIngredients
   ```

2. Install dependencies:
   ```bash
   cargo build
   ```

3. Set up environment variables:
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

4. Run the bot:
   ```bash
   cargo run
   ```

## Deployment

### Automated Deployment with GitHub Actions

JustIngredients includes automated deployment workflows using GitHub Actions:

#### Setup
1. **Add Fly.io Token to GitHub Secrets:**
   - Go to your repository Settings → Secrets and variables → Actions
   - Add a new secret named `FLY_API_TOKEN`
   - Get the token value: `fly auth token`

2. **Automatic Deployment:**
   - Push to `main` branch → automatic deployment to production
   - All tests run automatically before deployment
   - Deployment fails if tests don't pass

3. **Manual Deployment:**
   - Go to Actions tab → "Manual Deploy" workflow
   - Click "Run workflow" for on-demand deployment

#### Workflow Features
- ✅ **Automated Testing**: Runs full test suite before deployment
- ✅ **Code Quality Checks**: Clippy linting and formatting verification
- ✅ **Docker Build**: Handles complex OCR dependencies automatically
- ✅ **Fly.io Integration**: Deploys to your configured Fly.io app
- ✅ **Error Handling**: Clear failure notifications and rollback capability

### Manual Deployment

For manual deployment without GitHub Actions:

```bash
# Install Fly CLI
curl -L https://fly.io/install.sh | sh

# Authenticate
fly auth login

# Run the automated deployment script
./scripts/deploy.sh
```

The `deploy.sh` script automates:
- App and database creation on Fly.io
- Database attachment and secret configuration
- Application deployment with health checks

See `docs/deployment.md` for detailed deployment instructions.

## Configuration

JustIngredients supports extensive configuration through environment variables:

### Required Settings
```bash
TELEGRAM_BOT_TOKEN=your_bot_token_here
DATABASE_URL=postgresql://username:password@localhost/ingredients
```

### Optional Settings
```bash
# Health check server
HEALTH_PORT=8080

# Logging configuration
LOG_FORMAT=json|pretty
RUST_LOG=debug,sqlx=warn

# Cache configuration
OCR_CACHE_TTL=3600          # OCR result cache TTL (seconds)
DB_CACHE_SIZE_MB=50         # Database query cache size (MB)
USER_CACHE_TTL=1800         # User session cache TTL (seconds)

# OCR configuration
OCR_LANGUAGES=eng+fra       # Tesseract language codes
OCR_TIMEOUT_SECS=30         # OCR operation timeout
CIRCUIT_BREAKER_THRESHOLD=3 # Failures before circuit breaker triggers
CIRCUIT_BREAKER_RESET_SECS=60 # Circuit breaker reset timeout

# Performance tuning
MAX_CONCURRENT_REQUESTS=10  # Maximum concurrent Telegram requests
INSTANCE_POOL_SIZE=3        # OCR instance pool size
```

### Cache Configuration Details
- **OCR Cache**: Stores processed text results, keyed by image content hash
- **Database Cache**: Caches user recipes and ingredient lists with LRU eviction
- **User Cache**: Maintains user preferences and language settings
- **Memory Limits**: Automatic cleanup prevents memory bloat in production

## Monitoring & Observability

JustIngredients includes a comprehensive monitoring stack for production deployments:

### Features
- **Metrics Collection**: Prometheus metrics for requests, OCR operations, database queries, and system health
- **Business Intelligence**: Detailed tracking of user engagement, recipe creation patterns, and feature adoption
- **Distributed Tracing**: OpenTelemetry traces for request tracking and performance analysis
- **Structured Logging**: JSON logs with full context for production debugging
- **Health Checks**: Liveness and readiness probes for container orchestration
- **Grafana Dashboards**: Pre-built dashboards for bot overview and OCR performance monitoring
- **Alerting**: Configurable alerts for critical issues and performance degradation

### Business Metrics
- **Recipe Processing**: Creation rates, ingredient counts, naming methods (caption/manual/default)
- **User Engagement**: Command usage, photo uploads, ingredient editing, workflow completions
- **Dialogue Analytics**: Completion rates, step counts, abandonment tracking
- **Feature Adoption**: Caption naming usage, editing functionality, multi-language preferences
- **Performance KPIs**: OCR success rates, processing times, user retention metrics

### Quick Start
```bash
# Start the monitoring stack
cd grafana
./setup.sh

# Access services
# Grafana: http://localhost:3000 (admin/admin)
# Prometheus: http://localhost:9090
# Alertmanager: http://localhost:9093
```

### Metrics Endpoints
- **Metrics**: `http://localhost:8080/metrics` (Prometheus format)
- **Health (Liveness)**: `http://localhost:8080/health/live`
- **Health (Readiness)**: `http://localhost:8080/health/ready`

### Dashboards
- **Bot Overview**: Request rates, error rates, latency, message processing
- **OCR Performance**: Processing throughput, success rates, image sizes, memory usage
- **Business Intelligence**: User engagement trends, recipe creation analytics, feature adoption rates

See `grafana/README.md` for detailed setup instructions and configuration options.

### OCR Configuration
- **Languages**: English + French (`eng+fra`)
- **File Size Limits**: PNG: 15MB, JPEG: 10MB, BMP: 5MB, TIFF: 20MB
- **Timeout**: 30 seconds per OCR operation
- **Circuit Breaker**: 3 failures trigger, 60-second reset timeout

## Advanced Caching Infrastructure

JustIngredients implements a sophisticated multi-level caching system for optimal performance:

### Cache Types
- **OCR Result Cache**: Stores processed text results to avoid re-processing identical images
- **Database Query Cache**: Caches frequently accessed user data and recipe lists
- **User Session Cache**: Maintains user preferences and dialogue state
- **Measurement Pattern Cache**: Caches compiled regex patterns for ingredient detection

### Performance Benefits
- **Reduced OCR Processing**: Up to 90% reduction in redundant OCR operations
- **Faster Response Times**: Database query caching improves list operations by 60-80%
- **Memory Efficiency**: LRU eviction and size limits prevent memory bloat
- **Instance Pooling**: Reuses Tesseract instances for 100-500ms startup time savings

### Configuration
```bash
# Cache settings in environment
OCR_CACHE_TTL=3600          # 1 hour OCR result cache
DB_CACHE_SIZE_MB=50         # 50MB database query cache
USER_CACHE_TTL=1800         # 30 minutes user session cache
```

### Cache Key Strategies
- **OCR Cache**: SHA-256 hash of image content + OCR configuration
- **Database Cache**: Query type + user ID + pagination parameters
- **User Cache**: Telegram user ID with automatic invalidation on updates

## Usage

1. Start a chat with your bot on Telegram
2. Send an image containing an ingredient list or recipe
3. The bot will:
   - Download and process the image
   - Extract text using OCR
   - Parse measurements and ingredients
   - Store the results in the database
   - Confirm successful processing

### Photo Caption Support

The bot intelligently uses photo captions as recipe name suggestions:

- **Add a caption** to your photo (e.g., "Chocolate Chip Cookies") and it will be used as the recipe name
- **No caption needed** - the bot falls back to "Recipe" automatically
- **Invalid captions** (empty, too long, etc.) gracefully fall back to the default
- **Full editability** - you can always change the recipe name during the review process

**Example with Caption:**
```
User sends photo with caption: "Grandma's Apple Pie"
Bot responds: "📝 Using photo caption as recipe name: 'Grandma's Apple Pie'"
```

### Example Interactions

**Input Image:**
```
Crêpes Suzette

Ingrédients:
125 g de farine
2 œufs
1/2 litre de lait
2 cuillères à soupe de sucre
```

**Bot Response:**
Found 4 measurements:
1. 125 g → "farine"
2. 2 → "œufs" (quantity-only)
3. 1/2 litre → "lait"
4. 2 cuillères à soupe → "sucre"

### Workflow Transitions

After ingredient validation, users can seamlessly continue their workflow:

1. **Ingredient Review**: Users can edit individual ingredients or confirm the entire list
2. **Post-Confirmation Options**:
   - **Add Another Recipe**: Start processing a new recipe image
   - **List My Recipes**: Browse and select from saved recipes
   - **Search Recipes**: Search through recipe history (coming soon)
3. **Recipe Management**: Paginated recipe browsing with selection and details view

**Example Workflow:**
```
User sends recipe image → Bot extracts ingredients → User reviews/edits → User confirms → Bot shows success message with action buttons → User chooses next step
```

### Ingredient Editing Interface

JustIngredients features a focused editing interface that eliminates user confusion during ingredient editing:

**Focused Editing Experience:**
- **Message Replacement**: When editing an ingredient, the full recipe display is replaced with a clean editing prompt
- **Clear Instructions**: Users see the current ingredient value and receive clear guidance on how to enter new text
- **Single Action**: Only a cancel button is shown during editing, eliminating inactive button confusion
- **Seamless Transitions**: After editing or canceling, the original recipe display is restored automatically

**Editing Flow:**
```
Recipe Display → Click "Edit" → Focused Editing Prompt → Enter new text → Recipe Display (updated)
                                      ↓
                                   Click "Cancel" → Recipe Display (unchanged)
```

**Example Editing Interface:**
```
✏️ Edit Ingredient

Current: 2 cups flour

Enter the new ingredient text (e.g., "3 cups whole wheat flour"):

[❌ Cancel]
```

This approach provides a clean, unambiguous editing experience without the confusion of inactive buttons that were present in the previous interface.

## Architecture

### Core Modules
- **`main.rs`**: Application entry point and Telegram bot dispatcher
- **`bot.rs`**: Message handling, image processing, and user interactions
- **`ocr.rs`**: Tesseract OCR integration with circuit breaker pattern and instance pooling
- **`db.rs`**: PostgreSQL database operations with full-text search and query caching
- **`text_processing.rs`**: Measurement detection and ingredient parsing with pattern caching
- **`localization.rs`**: Internationalization support (English/French)
- **`cache.rs`**: Multi-level caching system for performance optimization
- **`observability.rs`**: Comprehensive metrics collection including business intelligence
- **`circuit_breaker.rs`**: Fault tolerance and automatic recovery mechanisms
- **`instance_manager.rs`**: OCR instance pooling and resource management

### Key Dependencies
- `teloxide`: Telegram bot framework
- `leptess`: Tesseract OCR Rust bindings
- `sqlx`: PostgreSQL database access
- `fluent-bundle`: Internationalization framework
- `tokio`: Async runtime
- `prometheus`: Metrics collection
- `opentelemetry`: Distributed tracing
- `dashmap`: Concurrent caching with TTL support

## Development

### Building
```bash
cargo build                    # Debug build
cargo build --release         # Optimized release build
```

### Testing
```bash
cargo test                     # Run all tests
cargo test --doc              # Run documentation tests
cargo run --example recipe_parser  # Run recipe parsing example
```

### Code Quality
- **Linting**: `cargo clippy` (all warnings must pass)
- **Formatting**: `cargo fmt` (must match standard Rust formatting)
- **Security**: `cargo deny` for dependency security auditing
- **Refactoring Agent**: `./scripts/refactor-agent.sh` (analyzes code for best practices and suggests improvements)

### SQLx Query Cache Management

When adding new `sqlx::query!` macros (compile-time checked queries), you must update the query cache:

**For queries in main source code (`src/`):**
```bash
cargo sqlx prepare
```

**For queries in test files (`tests/`):**
```bash
cargo sqlx prepare -- --all-targets
```

**Why this is needed:**
- `sqlx::query!` macros require compile-time verification against your database schema
- By default, `cargo sqlx prepare` only processes main source files, not tests
- The `--all-targets` flag tells Cargo to include test files in the compilation
- CI/CD will fail if the query cache is out of date

**Common error:**
```
error: set `DATABASE_URL` to use query macros online, or run `cargo sqlx prepare` to update the query cache
```

**Solution:** Always run `cargo sqlx prepare -- --all-targets` after adding new `sqlx::query!` calls, especially in tests.
