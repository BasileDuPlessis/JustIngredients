# JustIngredients Telegram Bot

A Telegram bot that reads the ingredient list of a recipe photo, lets you check and correct it,
and saves it as a recipe. Messages are in French or English, following the Telegram app language.

## What It Does
- **Photo → ingredients**: send a photo (or an image file) of a recipe; the Gemini vision API
  reads the ingredients, quantities and units. The photo caption becomes the recipe name.
- **Review before saving**: edit or delete lines, fix doubtful quantities (⚠️), confirm, name
  the recipe. Nothing is saved before you confirm.
- **My recipes**: list, open, rename, edit the ingredients of, and delete saved recipes.
- **Home message**: every finished action ends with a home message (number of recipes, the
  latest ones as buttons, the commands); the intermediate messages are deleted.
- **Commands**: `/liste` (`/list`) and `/aide` (`/help`), in the Telegram command menu; `/start`
  shows the welcome message; `/cherche` (`/search`) comes with search.

## Stack
Rust (teloxide, long polling), PostgreSQL (sqlx), Gemini API (`gemini-3.5-flash-lite`),
Fly.io. Specifications: [`specs/`](specs/README.md), in particular
[Telegram Bot](specs/telegram-bot.md), [Ingredient Extraction](specs/llm-extraction.md),
[Database Design](specs/database-design.md) and [Deployment](specs/deployment.md).

## Running Locally
Prerequisites: Rust 1.99, PostgreSQL, a Telegram bot token from @BotFather and a Gemini API key.

```bash
git clone https://github.com/BasileDuPlessis/JustIngredients.git
cd JustIngredients
cp .env.example .env   # set TELEGRAM_BOT_TOKEN, DATABASE_URL and GEMINI_API_KEY
cargo run
```

Use a test bot token locally, never the production one: two bots polling the same token steal
each other's updates.

## Configuration
Required: `TELEGRAM_BOT_TOKEN`, `DATABASE_URL`, `GEMINI_API_KEY`. Optional, with defaults:
`GEMINI_MODEL`, `GEMINI_TIMEOUT_SECS`, `GEMINI_THINKING_LEVEL`, `GEMINI_MAX_CONCURRENCY`,
`METRICS_AUTH_TOKEN`, `METRICS_BIND_ALL_INTERFACES`, ports and database pool; see
[`.env.example`](.env.example). Some observability variables
listed there are not read yet (IMP-051, [Deployment](specs/deployment.md)).

## Development
```bash
./scripts/hooks/install.sh   # once: pre-push hook (unwrap check, fmt, clippy, unit tests)
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

- `.unwrap()` is not allowed outside tests (`scripts/check-unwraps.sh`).
- Database tests are ignored by default; run them against a throwaway database:
  `DATABASE_URL=postgres://.../ji_test cargo test --test db_tests -- --include-ignored`.
- Benchmarks (`tests/performance_tests.rs`) are ignored by default: `-- --ignored`.
- Plans and progress: [`specs/tasks/`](specs/tasks/); code-quality items:
  [improvement backlog](specs/tasks/improvement-backlog.md).

## Deployment
Push to `main` → "Build and Test" (tests, then one image) → "Deploy to Staging" (permanent app
with the test bot, smoke tests) → run "Deploy to Production" by hand and approve the
`production` environment; it deploys the same image and rolls back if it is unhealthy. Details, secrets and
health endpoints: [Deployment](specs/deployment.md).

## License
MIT
