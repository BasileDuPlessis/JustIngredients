# JustIngredients Telegram Bot

A Telegram bot that extracts text from images using OCR (Optical Character Recognition) and stores ingredient lists in a searchable database.

## Features

- **OCR Text Extraction**: Uses Tesseract OCR to extract text from images and photos
- **Ingredient Parsing**: Automatically detects and parses measurements and ingredients from recipe text
- **Quantity-Only Support**: Recognizes ingredients with quantities but no measurement units (e.g., "6 oeufs", "4 pommes")
- **Photo Caption Support**: Uses photo captions as recipe name candidates with intelligent fallback
- **Full-Text Search**: PostgreSQL full-text search for efficient content searching
- **Multilingual Support**: English and French language support with localized messages
- **Circuit Breaker Pattern**: Protects against OCR failures with automatic recovery
- **Database Storage**: Persistent storage of extracted text and user interactions
- **Workflow Transitions**: Smooth user experience with clear next-action options after ingredient validation
- **Recipe Management**: List, search, and organize saved recipes with intuitive navigation

## Supported Measurement Formats

### Traditional Measurements
- Volume: `2 cups flour`, `1 tablespoon sugar`, `250 ml milk`
- Weight: `500g butter`, `1 kg tomatoes`, `2 lbs beef`
- Count: `3 eggs`, `2 slices bread`, `1 can tomatoes`

### Quantity-Only Ingredients
- French: `6 oeufs`, `4 pommes`, `3 carottes`
- English: `5 apples`, `2 onions`, `8 potatoes`

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
   # Edit .env with your Telegram bot token
   ```

4. Run the bot:
   ```bash
   cargo run
   ```

## Monitoring & Observability

JustIngredients includes a comprehensive monitoring stack for production deployments:

### Features
- **Metrics Collection**: Prometheus metrics for requests, OCR operations, database queries, and system health
- **Distributed Tracing**: OpenTelemetry traces for request tracking and performance analysis
- **Structured Logging**: JSON logs with full context for production debugging
- **Health Checks**: Liveness and readiness probes for container orchestration
- **Grafana Dashboards**: Pre-built dashboards for bot overview and OCR performance monitoring
- **Alerting**: Configurable alerts for critical issues and performance degradation

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

See `grafana/README.md` for detailed setup instructions and configuration options.

### OCR Configuration
- **Languages**: English + French (`eng+fra`)
- **File Size Limits**: PNG: 15MB, JPEG: 10MB, BMP: 5MB, TIFF: 20MB
- **Timeout**: 30 seconds per OCR operation
- **Circuit Breaker**: 3 failures trigger, 60-second reset timeout

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

## Architecture

### Core Modules
- **`main.rs`**: Application entry point and Telegram bot dispatcher
- **`bot.rs`**: Message handling, image processing, and user interactions
- **`ocr.rs`**: Tesseract OCR integration with circuit breaker pattern
- **`db.rs`**: PostgreSQL database operations with full-text search support
- **`text_processing.rs`**: Measurement detection and ingredient parsing
- **`localization.rs`**: Internationalization support (English/French)

### Key Dependencies
- `teloxide`: Telegram bot framework
- `leptess`: Tesseract OCR Rust bindings
- `sqlx`: PostgreSQL database access
- `fluent-bundle`: Internationalization framework
- `tokio`: Async runtime

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

## Examples

See the `examples/` directory for usage examples:

- `recipe_parser.rs`: Comprehensive recipe parsing demonstration
- Shows both traditional measurements and quantity-only ingredients
- Demonstrates configuration options and post-processing

## Database Schema

The bot uses a PostgreSQL schema with full-text search support:

```sql
-- Users table: Maps Telegram IDs to internal IDs and tracks language preference
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    telegram_id BIGINT UNIQUE NOT NULL,
    language_code VARCHAR(10) DEFAULT 'en',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Recipes table: Stores full OCR text blocks for audit/traceability
CREATE TABLE recipes (
    id SERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL,
    content TEXT NOT NULL,
    recipe_name VARCHAR(255),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
);

-- Ingredients table: Links to users and optionally to recipes, stores parsed data
CREATE TABLE ingredients (
    id SERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id),
    recipe_id BIGINT REFERENCES recipes(id),
    name VARCHAR(255) NOT NULL,
    quantity DECIMAL(10,3),
    unit VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id),
    FOREIGN KEY (recipe_id) REFERENCES recipes(id)
);

-- Indexes for performance
CREATE INDEX recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
CREATE INDEX ingredients_user_id_idx ON ingredients(user_id);
CREATE INDEX ingredients_recipe_id_idx ON ingredients(recipe_id);
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass: `cargo test`
6. Format code: `cargo fmt`
7. Lint code: `cargo clippy`
8. Commit your changes
9. Push to your fork
10. Create a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Changelog

### v0.1.4 (2025-10-09)
- **New**: Photo caption support for automatic recipe naming
  - Uses photo captions as recipe name candidates with intelligent validation
  - Graceful fallback to "Recipe" for missing or invalid captions
  - User feedback messages when captions are used or rejected
  - Full backward compatibility - no caption required
- **Enhanced**: User experience with real-time caption feedback
  - Clear messages when captions are accepted: "📝 Using photo caption as recipe name: 'Chocolate Cookies'"
  - Informative messages for invalid captions: "⚠️ Caption is invalid, using default recipe name instead"
  - Multi-language support for all caption-related messages
- **Improved**: Testing coverage for caption functionality
  - 9 new tests covering caption extraction, validation, and processing
  - Integration tests for complete photo-with-caption workflows
  - Edge case testing for Unicode, special characters, and boundary conditions
  - Backward compatibility verification ensuring existing functionality preserved

### v0.1.3 (2025-10-08)
- **New**: Workflow transitions after ingredient validation with action buttons
  - Added confirmation message with "Add Another Recipe", "List My Recipes", and "Search Recipes" options
  - Improved user experience by removing edit/delete buttons after confirmation
  - Enhanced recipe management workflow with clear next-action choices
- **Refactored**: Function signatures with too many arguments using parameter structs
  - Created `DialogueContext`, `RecipeNameInputParams`, `RecipeNameAfterConfirmInputParams`, etc.
  - Improved code maintainability and reduced parameter complexity
  - Added `ImageProcessingParams` for image processing functions
- **Enhanced**: Testing coverage for new workflow functionality
  - Added 4 new test functions covering workflow transitions and localization
  - Comprehensive testing of post-confirmation keyboard creation
  - Validation of workflow message formatting in both languages
- **Improved**: Documentation and localization
  - Added workflow-related localization keys in English and French
  - Updated README with workflow transition examples
  - Enhanced copilot instructions with new feature documentation

### v0.1.2 (2025-10-02)
- **Renamed**: `ocr_entries` table to `recipes` for better semantic clarity
- **Renamed**: `OcrEntry` struct to `Recipe` 
- **Renamed**: All related functions from `*_ocr_entry*` to `*_recipe*`
- **Updated**: Foreign key `ocr_entry_id` to `recipe_id` in ingredients table
- **Removed**: `raw_text` field from ingredients table (deemed unnecessary)
- **Updated**: All tests, documentation, and code references

### v0.1.1 (2025-09-29)
- **Removed**: Conversion ratios table and related functionality
- **Refactored**: Measurement units moved to external JSON configuration (`config/measurement_units.json`)
- **Updated**: Database schema simplified to 3 core tables (users, recipes, ingredients)
- **Improved**: Code cleanup and removal of unused imports
- **Fixed**: Clippy warnings and placeholder tests

### v0.1.0 (2025-09-26)
- Initial release with OCR text extraction and ingredient parsing
- Support for traditional measurements (cups, grams, liters, etc.)
- **New**: Quantity-only ingredient support (e.g., "6 oeufs", "4 pommes")
- PostgreSQL database with full-text search
- English and French localization
- Circuit breaker pattern for OCR reliability
- Telegram bot integration</content>
<parameter name="filePath">/Users/basile.du.plessis/Documents/ingredients/README.md