use anyhow::Result;
use just_ingredients::bot;
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

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file first
    dotenv::dotenv().ok();

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

    // Initialize complete observability stack with health checks (metrics, tracing, logging)
    observability::init_observability_with_health_checks(
        Some(Arc::clone(&shared_pool)),
        Some(bot_token.clone()),
    )
    .await?;

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
            move |bot: Bot, msg: Message| {
                let pool = Arc::clone(&pool);
                let storage = storage.clone();
                let localization = Arc::clone(&localization);
                let dialogue = RecipeDialogue::new(storage, msg.chat.id);
                async move { bot::message_handler(bot, msg, pool, dialogue, localization).await }
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
                async move { bot::callback_handler(bot, q, pool, dialogue, localization).await }
            }
        }));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
