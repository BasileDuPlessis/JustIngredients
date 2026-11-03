use anyhow::Result;
use just_ingredients::bot;
use just_ingredients::config::AppConfig;
use just_ingredients::db;
use just_ingredients::db_connection::{self, DEFAULT_CONNECT_BACKOFF};
use just_ingredients::deduplication;
use just_ingredients::dialogue::RecipeDialogue;
use just_ingredients::dialogue_storage::PgDialogueStorage;
use just_ingredients::extraction::gemini::GeminiExtractor;
use just_ingredients::extraction::guard::GuardedExtractor;
use just_ingredients::extraction::SharedExtractor;
use just_ingredients::localization;
use just_ingredients::observability;
use just_ingredients::telegram_connection::{self, TELEGRAM_CONNECT_BACKOFF};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use tracing::{info, warn};

/// Validate environment variables at startup
fn validate_environment_variables() -> Result<()> {
    // Validate TELEGRAM_BOT_TOKEN
    let bot_token = env::var("TELEGRAM_BOT_TOKEN")
        .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN environment variable is required but not set. Please set it to your Telegram bot token."))?;

    if bot_token.trim().is_empty() {
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN cannot be empty"));
    }

    // Basic bot token format validation (Telegram bot tokens have a specific format: numbers:letters)
    if !bot_token.contains(':') {
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN format is invalid. Telegram bot tokens should contain a colon (:) character."));
    }

    let parts: Vec<&str> = bot_token.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "TELEGRAM_BOT_TOKEN format is invalid. Expected format: 'bot_id:bot_token'"
        ));
    }

    // Validate bot ID is numeric
    if parts[0].parse::<u64>().is_err() {
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN bot ID must be numeric"));
    }

    // Validate bot token length (should be reasonably long)
    if parts[1].len() < 20 {
        return Err(anyhow::anyhow!(
            "TELEGRAM_BOT_TOKEN appears to be too short. Please verify it's a valid bot token."
        ));
    }

    // Validate DATABASE_URL
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required but not set. Please set it to your PostgreSQL connection string."))?;

    if database_url.trim().is_empty() {
        return Err(anyhow::anyhow!("DATABASE_URL cannot be empty"));
    }

    // Basic PostgreSQL URL validation
    if !database_url.starts_with("postgresql://") && !database_url.starts_with("postgres://") {
        return Err(anyhow::anyhow!(
            "DATABASE_URL must start with 'postgresql://' or 'postgres://'"
        ));
    }

    // Check for required components (at minimum: postgresql://user:pass@host:port/db)
    let url_parts: Vec<&str> = database_url.split("://").collect();
    if url_parts.len() != 2 {
        return Err(anyhow::anyhow!("DATABASE_URL format is invalid"));
    }

    let connection_part = url_parts[1];
    if !connection_part.contains('@') {
        return Err(anyhow::anyhow!("DATABASE_URL must contain authentication information (user:password@host:port/database)"));
    }

    info!("Environment variables validated successfully");
    Ok(())
}

/// Validate HTTP client configuration
fn validate_http_client_config() -> Result<()> {
    // Validate HTTP timeout from environment (default 30 seconds)
    let timeout_secs = env::var("HTTP_CLIENT_TIMEOUT_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<u64>()
        .map_err(|_| {
            anyhow::anyhow!("HTTP_CLIENT_TIMEOUT_SECS must be a valid number of seconds")
        })?;

    if timeout_secs == 0 {
        return Err(anyhow::anyhow!("HTTP_CLIENT_TIMEOUT_SECS cannot be 0"));
    }

    if timeout_secs > 300 {
        return Err(anyhow::anyhow!(
            "HTTP_CLIENT_TIMEOUT_SECS cannot be greater than 300 seconds (5 minutes)"
        ));
    }

    // Validate database connection pool settings
    let max_connections = env::var("DATABASE_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("DATABASE_MAX_CONNECTIONS must be a valid number"))?;

    if max_connections == 0 {
        return Err(anyhow::anyhow!("DATABASE_MAX_CONNECTIONS cannot be 0"));
    }

    if max_connections > 100 {
        return Err(anyhow::anyhow!(
            "DATABASE_MAX_CONNECTIONS cannot be greater than 100"
        ));
    }

    // Validate connection timeout
    let connect_timeout_secs = env::var("DATABASE_CONNECT_TIMEOUT_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<u64>()
        .map_err(|_| {
            anyhow::anyhow!("DATABASE_CONNECT_TIMEOUT_SECS must be a valid number of seconds")
        })?;

    if connect_timeout_secs == 0 {
        return Err(anyhow::anyhow!("DATABASE_CONNECT_TIMEOUT_SECS cannot be 0"));
    }

    if connect_timeout_secs > 300 {
        return Err(anyhow::anyhow!(
            "DATABASE_CONNECT_TIMEOUT_SECS cannot be greater than 300 seconds"
        ));
    }

    info!("HTTP client and server configuration validated successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file first
    dotenvy::dotenv().ok();

    // Validate environment variables early
    validate_environment_variables()?;

    // Load configuration and start logging before anything else, so that startup
    // messages (including database connection retries) are visible.
    let app_config = AppConfig::from_env()?;
    observability::init_logging()?;

    // Get bot token from environment
    let bot_token = env::var("TELEGRAM_BOT_TOKEN")
        .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN must be set"))?;

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
    let deduplicator = crate::deduplication::create_shared_deduplicator(300, 10000); // 5 min TTL, 10k entries
    info!("Request deduplicator initialized for duplicate message prevention");

    // Validate HTTP client configuration
    validate_http_client_config()?;

    // The metrics, the public health server and the internal metrics one (logging already
    // runs).
    // Readiness waits for a Telegram success from the start, not only once the heartbeat runs
    observability::health_checks::expect_telegram();
    observability::start(
        Some(Arc::clone(&shared_pool)),
        Some(bot_token.clone()),
        &app_config,
    )
    .await?;
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
    match gemini.verify_access().await {
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
        .timeout(Duration::from_secs(30)) // 30 second timeout
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
        tokio::spawn(observability::report::run(
            bot.clone(),
            Arc::clone(&shared_pool),
            owner,
        ));
    }

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

    info!("Bot initialized with 30s timeout, starting dispatcher");

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

    // Set up the dispatcher with shared connection and dialogue support
    let handler = dptree::entry()
        // Recipes belong to one person: groups and channels only get a "private chats" answer
        .branch(
            Update::filter_message()
                .filter(|msg: Message| !msg.chat.is_private())
                .endpoint({
                    let localization = Arc::clone(&localization_manager);
                    move |bot: Bot, msg: Message| {
                        let localization = Arc::clone(&localization);
                        async move {
                            bot::command_handlers::handle_group_message(&bot, &msg, &localization)
                                .await
                        }
                    }
                }),
        )
        .branch(
            Update::filter_callback_query()
                .filter(|q: CallbackQuery| {
                    q.message.as_ref().is_some_and(|m| !m.chat().is_private())
                })
                .endpoint({
                    let localization = Arc::clone(&localization_manager);
                    move |bot: Bot, q: CallbackQuery| {
                        let localization = Arc::clone(&localization);
                        async move {
                            let text = localization::t_lang(
                                &localization,
                                "private-chat-only",
                                q.from.language_code.as_deref(),
                            );
                            bot.answer_callback_query(q.id).text(text).await?;
                            anyhow::Ok(())
                        }
                    }
                }),
        )
        .branch(Update::filter_message().endpoint({
            let storage = dialogue_storage.clone();
            let services = bot::MessageServices {
                pool: Arc::clone(&shared_pool),
                localization: Arc::clone(&localization_manager),
                deduplicator: Some(Arc::clone(&deduplicator)),
                extractor: Arc::clone(&extractor),
            };
            move |bot: Bot, msg: Message| {
                let dialogue = RecipeDialogue::new(storage.clone(), msg.chat.id);
                let services = services.clone();
                async move { bot::message_handler_with_cache(bot, msg, dialogue, services).await }
            }
        }))
        .branch(Update::filter_callback_query().endpoint({
            let pool = Arc::clone(&shared_pool);
            let storage = dialogue_storage.clone();
            let localization = Arc::clone(&localization_manager);
            move |bot: Bot, q: CallbackQuery| {
                let pool = Arc::clone(&pool);
                let storage = storage.clone();
                let localization = Arc::clone(&localization);
                // Use the chat ID from the original message that contained the inline keyboard
                let chat_id = match &q.message {
                    Some(msg) => match msg {
                        teloxide::types::MaybeInaccessibleMessage::Regular(msg) => msg.chat.id,
                        teloxide::types::MaybeInaccessibleMessage::Inaccessible(_) => {
                            ChatId::from(q.from.id)
                        }
                    },
                    None => ChatId::from(q.from.id),
                };
                let dialogue = RecipeDialogue::new(storage, chat_id);
                async move {
                    bot::callback_handler_with_cache(bot, q, pool, dialogue, localization).await
                }
            }
        }));

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
