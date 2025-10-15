use anyhow::Result;
use just_ingredients::bot;
use just_ingredients::cache::CacheManager;
use just_ingredients::db;
use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
use just_ingredients::localization;
use just_ingredients::observability;
use sqlx::postgres::PgPool;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use tracing::info;

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
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN format is invalid. Expected format: 'bot_id:bot_token'"));
    }

    // Validate bot ID is numeric
    if parts[0].parse::<u64>().is_err() {
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN bot ID must be numeric"));
    }

    // Validate bot token length (should be reasonably long)
    if parts[1].len() < 20 {
        return Err(anyhow::anyhow!("TELEGRAM_BOT_TOKEN appears to be too short. Please verify it's a valid bot token."));
    }

    // Validate DATABASE_URL
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required but not set. Please set it to your PostgreSQL connection string."))?;

    if database_url.trim().is_empty() {
        return Err(anyhow::anyhow!("DATABASE_URL cannot be empty"));
    }

    // Basic PostgreSQL URL validation
    if !database_url.starts_with("postgresql://") && !database_url.starts_with("postgres://") {
        return Err(anyhow::anyhow!("DATABASE_URL must start with 'postgresql://' or 'postgres://'"));
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

/// Validate OCR configuration at startup
fn validate_ocr_configuration() -> Result<()> {
    // Force initialization of the lazy static to trigger validation
    let config = just_ingredients::ocr_config::OcrConfig::default();

    // Validate the configuration
    config.validate().map_err(|e| {
        anyhow::anyhow!("OCR configuration validation failed: {}. Please check your configuration values.", e)
    })?;

    info!("OCR configuration validated successfully");
    Ok(())
}

/// Validate HTTP client configuration
fn validate_http_client_config() -> Result<()> {
    // Validate HTTP timeout from environment (default 30 seconds)
    let timeout_secs = env::var("HTTP_CLIENT_TIMEOUT_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_TIMEOUT_SECS must be a valid number of seconds"))?;

    if timeout_secs == 0 {
        return Err(anyhow::anyhow!("HTTP_CLIENT_TIMEOUT_SECS cannot be 0"));
    }

    if timeout_secs > 300 {
        return Err(anyhow::anyhow!("HTTP_CLIENT_TIMEOUT_SECS cannot be greater than 300 seconds (5 minutes)"));
    }

    // Validate metrics server configuration
    let metrics_port = env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9090".to_string())
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("METRICS_PORT must be a valid port number (1-65535)"))?;

    if metrics_port < 1024 && env::var("ALLOW_PRIVILEGED_PORTS").unwrap_or_else(|_| "false".to_string()) != "true" {
        return Err(anyhow::anyhow!("METRICS_PORT {} is a privileged port (< 1024). Set ALLOW_PRIVILEGED_PORTS=true to allow or use a port >= 1024", metrics_port));
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
        return Err(anyhow::anyhow!("DATABASE_MAX_CONNECTIONS cannot be greater than 100"));
    }

    // Validate connection timeout
    let connect_timeout_secs = env::var("DATABASE_CONNECT_TIMEOUT_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("DATABASE_CONNECT_TIMEOUT_SECS must be a valid number of seconds"))?;

    if connect_timeout_secs == 0 {
        return Err(anyhow::anyhow!("DATABASE_CONNECT_TIMEOUT_SECS cannot be 0"));
    }

    if connect_timeout_secs > 300 {
        return Err(anyhow::anyhow!("DATABASE_CONNECT_TIMEOUT_SECS cannot be greater than 300 seconds"));
    }

    info!("HTTP client and server configuration validated successfully");
    Ok(())
}

/// Validate text processing configuration at startup
fn validate_text_processing_config() -> Result<()> {
    // Load and validate measurement units configuration
    let config = just_ingredients::text_processing::load_measurement_units_config();

    // Validate the configuration
    config.validate().map_err(|e| {
        anyhow::anyhow!("Text processing configuration validation failed: {}. Please check your config/measurement_units.json file.", e)
    })?;

    info!("Text processing configuration validated successfully");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file first
    dotenvy::dotenv().ok();

    // Validate environment variables early
    validate_environment_variables()?;

    // Get bot token from environment
    let bot_token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN must be set");

    // Get database path from environment
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    info!(database_url = %database_url, "Initializing database connection");

    // Create database connection pool
    let pool = PgPool::connect(&database_url).await?;

    // Initialize database schema
    db::init_database_schema(&pool).await?;

    // Wrap pool in Arc for sharing across async tasks
    let shared_pool = Arc::new(pool);

    // Initialize cache manager for performance optimization
    let cache_manager = Arc::new(std::sync::Mutex::new(CacheManager::new()));
    info!("Cache manager initialized for performance optimization");

    // Validate OCR configuration before initializing observability
    validate_ocr_configuration()?;

    // Validate text processing configuration
    validate_text_processing_config()?;

    // Validate HTTP client configuration
    validate_http_client_config()?;

    // Initialize complete observability stack with health checks (metrics, tracing, logging)
    observability::init_observability_with_health_checks(
        Some(Arc::clone(&shared_pool)),
        Some(bot_token.clone()),
    )
    .await?;

    // Start background metrics recording tasks
    let _system_metrics_handle = observability::start_system_metrics_recorder();
    let _health_metrics_handle = observability::start_health_metrics_recorder(
        Some(Arc::clone(&shared_pool)),
        Some(bot_token.clone()),
    )
    .await;

    // Initialize localization manager
    let localization_manager = localization::create_localization_manager()?;

    // Initialize the bot with custom client configuration for better reliability
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30)) // 30 second timeout
        .build()
        .expect("Failed to create HTTP client");

    let bot = Bot::with_client(bot_token, client);

    info!("Bot initialized with 30s timeout, starting dispatcher");

    // Create shared dialogue storage
    let dialogue_storage = InMemStorage::<RecipeDialogueState>::new();

    // Set up the dispatcher with shared connection and dialogue support
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint({
            let pool = Arc::clone(&shared_pool);
            let storage = dialogue_storage.clone();
            let localization = Arc::clone(&localization_manager);
            let cache = Arc::clone(&cache_manager);
            move |bot: Bot, msg: Message| {
                let pool = Arc::clone(&pool);
                let storage = storage.clone();
                let localization = Arc::clone(&localization);
                let cache = Arc::clone(&cache);
                let dialogue = RecipeDialogue::new(storage, msg.chat.id);
                async move { bot::message_handler_with_cache(bot, msg, pool, dialogue, localization, cache).await }
            }
        }))
        .branch(Update::filter_callback_query().endpoint({
            let pool = Arc::clone(&shared_pool);
            let storage = dialogue_storage.clone();
            let localization = Arc::clone(&localization_manager);
            let cache = Arc::clone(&cache_manager);
            move |bot: Bot, q: CallbackQuery| {
                let pool = Arc::clone(&pool);
                let storage = storage.clone();
                let localization = Arc::clone(&localization);
                let cache = Arc::clone(&cache);
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
                async move { bot::callback_handler_with_cache(bot, q, pool, dialogue, localization, cache).await }
            }
        }));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
