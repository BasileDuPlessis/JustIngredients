// No panic outside tests (IMP-018): a panic stops the handling of an update without a reply.
// No wrapping cast (IMP-038): a Telegram id goes through `bot::telegram_id`.
// Errors go up with `?`; the few constant patterns that cannot fail say so with an `allow`.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::cast_possible_wrap
    )
)]

use anyhow::Result;
use just_ingredients::bot;
use just_ingredients::config::AppConfig;
use just_ingredients::db;
use just_ingredients::db_connection::{self, DEFAULT_CONNECT_BACKOFF};
use just_ingredients::deduplication;
use just_ingredients::dialogue_storage::PgDialogueStorage;
use just_ingredients::extraction::SharedExtractor;
use just_ingredients::extraction::gemini::GeminiExtractor;
use just_ingredients::extraction::guard::GuardedExtractor;
use just_ingredients::localization;
use just_ingredients::observability;
use just_ingredients::telegram_connection::{self, TELEGRAM_CONNECT_BACKOFF};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file first
    dotenvy::dotenv().ok();

    // The configuration is read and checked once, here, before anything starts; then logging
    // starts, so that startup messages (including database connection retries) are visible.
    let app_config = AppConfig::from_env()?;
    app_config.validate()?;
    observability::init_logging()?;
    info!("{}", app_config.summary());
    let bot_token = app_config.bot.token.clone();

    info!(
        database = %db_connection::describe_database_target(&app_config.database.url),
        "Initializing database connection"
    );

    // Create the connection pool, waiting for the database if it is restarting
    let pool =
        db_connection::connect_with_retry(&app_config.database, DEFAULT_CONNECT_BACKOFF).await?;

    // Initialize database schema
    db::init_database_schema(&pool).await?;

    // Validate that the database schema is correct
    db::validate_database_schema(&pool).await?;

    // A user's recipes have unique names ("Tarte (2)"): number any duplicates left
    let numbered = db::number_duplicate_recipe_names(&pool).await?;
    if numbered > 0 {
        info!(recipes = numbered, "Numbered duplicate recipe names");
    }
    // Units saved before they were normalized ("G", "litres", "cl"): written like new ones.
    // Never stops the startup: the cards read these units anyway.
    match db::normalize_saved_units(&pool).await {
        Ok(0) => {}
        Ok(normalized) => info!(ingredients = normalized, "Normalized saved units"),
        Err(e) => warn!(error = %e, "Could not normalize the saved units"),
    }

    // Wrap pool in Arc for sharing across async tasks
    let shared_pool = Arc::new(pool);
    // Messages to delete at the end of an action are recorded in the database
    bot::cleanup::init(Arc::clone(&shared_pool));

    // Initialize request deduplicator to prevent duplicate message processing
    let deduplicator = crate::deduplication::create_shared_deduplicator(
        app_config.bot.deduplication_ttl_secs,
        10_000,
    );
    info!("Request deduplicator initialized for duplicate message prevention");

    // The metrics, the public health server and the internal metrics one (logging already
    // runs).
    // Readiness waits for a Telegram success from the start, not only once the heartbeat runs
    observability::health_checks::expect_telegram();
    observability::start(Some(Arc::clone(&shared_pool)), &app_config).await?;
    // The usage history: visits and daily totals, the user and recipe gauges
    tokio::spawn(observability::usage::run(Arc::clone(&shared_pool)));

    // Initialize localization manager
    let localization_manager = localization::create_localization_manager()?;

    // Ingredient extraction engine (Gemini)
    if let Ok(backend) = env::var("EXTRACTION_BACKEND") {
        warn!(
            value = %backend,
            "EXTRACTION_BACKEND is no longer read: the Tesseract fallback was removed, Gemini is used"
        );
    }
    let gemini = GeminiExtractor::new(&app_config.extraction.gemini)?;
    // Its failure does not stop the bot (lists and recipes still work): the owner is told once
    // the alerts run, below
    let gemini_check = gemini.verify_access().await;
    match &gemini_check {
        Ok(()) => info!(model = %app_config.extraction.gemini.model, "Gemini API access verified"),
        Err(error) => tracing::error!(
            model = %app_config.extraction.gemini.model,
            error = %error,
            "Gemini API access check failed: photos will fail until the key or model is fixed"
        ),
    }
    // Concurrency limit and result cache in front of the engine (cost and memory protection)
    let extractor: SharedExtractor = Arc::new(
        GuardedExtractor::new(
            Arc::new(gemini),
            app_config.extraction.gemini.max_concurrency,
        )
        .with_daily_limit(app_config.extraction.daily_limit_per_user),
    );
    info!(
        backend = extractor.name(),
        model = %app_config.extraction.gemini.model,
        "Ingredient extraction initialized"
    );

    // Initialize the bot with custom client configuration for better reliability
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(app_config.bot.http_timeout_secs))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

    let bot = Bot::with_client(bot_token, client);

    // Health checks report whether the bot can reach Telegram, not only that it runs. Started
    // first, so that readiness fails while the bot is still waiting for Telegram below.
    let _telegram_heartbeat = observability::start_telegram_heartbeat(bot.clone());

    // teloxide panics if its first getMe fails: check the token first, waiting for Telegram
    telegram_connection::connect_with_retry(&bot, TELEGRAM_CONNECT_BACKOFF).await?;
    // Ready at once, not only after the heartbeat's next call (up to a minute later)
    observability::health_checks::record_telegram_success();

    // Alerts and the weekly report to the owner, with OWNER_CHAT_ID (plan O4, O5)
    tokio::spawn(observability::alerts::watch_database(Arc::clone(
        &shared_pool,
    )));
    if let Some(owner) = observability::alerts::start(bot.clone(), Arc::clone(&shared_pool)).await {
        if let Err(error) = &gemini_check {
            observability::alerts::gemini_failed_at_start(error);
        }
        tokio::spawn(observability::report::run(
            bot.clone(),
            Arc::clone(&shared_pool),
            owner,
        ));
    }

    // Token messages of /cle are masked 10 minutes after they were sent, also those sent before
    // a restart (they are recorded)
    tokio::spawn(bot::api_keys::run_secret_sweeper(
        bot.clone(),
        Arc::clone(&localization_manager),
    ));

    // The command menu only helps users find the commands: without it the bot still works
    tokio::spawn({
        let bot = bot.clone();
        let localization = Arc::clone(&localization_manager);
        async move {
            match bot::commands::set_command_menu(&bot, &localization).await {
                Ok(()) => info!("Telegram command menu set"),
                Err(e) => warn!(error = %e, "Failed to set the Telegram command menu"),
            }
        }
    });

    info!(
        timeout_secs = app_config.bot.http_timeout_secs,
        "Bot initialized, starting dispatcher"
    );

    // Running actions are kept in the database: a deploy or a restart does not lose them. They
    // are resumed only if this version ran a moment ago, which it notes every minute.
    let dialogue_storage = PgDialogueStorage::new((*shared_pool).clone());
    match dialogue_storage.start().await {
        Ok(0) => {}
        Ok(purged) => info!(dialogues = purged, "Dropped dialogue states not resumed"),
        Err(e) => warn!(error = %e, "Could not prepare the dialogue states"),
    }
    tokio::spawn({
        let storage = Arc::clone(&dialogue_storage);
        async move {
            let mut every_minute = tokio::time::interval(std::time::Duration::from_secs(60));
            every_minute.tick().await;
            loop {
                every_minute.tick().await;
                if let Err(e) = storage.mark_alive().await {
                    warn!(error = %e, "Could not note that the dialogue states are kept");
                }
            }
        }
    });

    // The handler tree, built in the library so that tests build the same one
    let handler = bot::dispatch::handler_tree(bot::dispatch::BotServices {
        messages: bot::MessageServices {
            pool: Arc::clone(&shared_pool),
            localization: Arc::clone(&localization_manager),
            deduplicator: Some(Arc::clone(&deduplicator)),
            extractor: Arc::clone(&extractor),
            api: Arc::new(bot::api_keys::ApiAccess::from_config(&app_config.api)),
        },
        dialogues: Arc::clone(&dialogue_storage),
    });

    let listener = teloxide::update_listeners::polling_default(bot.clone()).await;
    let dispatched = Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .try_dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
    // The last minute of usage counts, before exiting; the next start resumes the actions
    observability::usage::flush(&shared_pool).await;
    if let Err(e) = dialogue_storage.mark_alive().await {
        warn!(error = %e, "Could not note that the dialogue states are kept");
    }
    dispatched
        .map_err(|e| anyhow::anyhow!("Telegram API unreachable when the dispatcher started: {e}"))
}
