//! Updates through the real handler tree (`bot::dispatch::handler_tree`), with a fake Telegram
//! server (`teloxide_tests`), a fake extractor and a throwaway database (backlog IMP-031).
//!
//! Like `db_tests`, they need Postgres: `cargo test --test handler_tests -- --include-ignored`
//! with `DATABASE_URL` set to a throwaway database (each test creates its own).

use std::sync::Arc;

use anyhow::{Context, Result};
use just_ingredients::bot::dispatch::{handler_tree, BotServices};
use just_ingredients::bot::MessageServices;
use just_ingredients::dialogue_storage::PgDialogueStorage;
use just_ingredients::extraction::{
    ExtractedIngredient, Extraction, ExtractionFuture, ImageInput, IngredientExtractor,
    RecipeLanguage,
};
use just_ingredients::localization::{create_localization_manager, t_lang};
use sqlx::PgPool;
use teloxide_tests::{MockBot, MockMessageText};

/// A fake bot on the handler tree. The cleanup's database is a global: it is set once
/// `MockBot::new` holds the lock that runs one fake bot at a time.
fn mock_bot(
    update: impl teloxide_tests::IntoUpdate,
    services: BotServices,
) -> MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey> {
    let pool = Arc::clone(&services.messages.pool);
    let bot = MockBot::new(update, handler_tree(services));
    just_ingredients::bot::cleanup::init(pool);
    bot
}

/// Answers every photo with the same ingredients, without calling Gemini.
#[derive(Debug)]
struct FakeExtractor {
    ingredients: Vec<ExtractedIngredient>,
}

impl IngredientExtractor for FakeExtractor {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn extract<'a>(&'a self, _image: &'a ImageInput) -> ExtractionFuture<'a> {
        let extraction = Extraction {
            is_recipe: true,
            title: None,
            language: RecipeLanguage::Fr,
            transcription: String::new(),
            ingredients: self.ingredients.clone(),
        };
        Box::pin(async move { Ok(extraction) })
    }
}

/// The services of the bot on the test's database (its schema created).
async fn services(pool: &PgPool, ingredients: Vec<ExtractedIngredient>) -> Result<BotServices> {
    just_ingredients::db::init_database_schema(pool).await?;
    let pool = Arc::new(pool.clone());
    Ok(BotServices {
        messages: MessageServices {
            pool: Arc::clone(&pool),
            localization: create_localization_manager()?,
            deduplicator: None,
            extractor: Arc::new(FakeExtractor { ingredients }),
        },
        dialogues: PgDialogueStorage::new((*pool).clone()),
    })
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn start_answers_the_welcome(pool: PgPool) -> Result<()> {
    let services = services(&pool, Vec::new()).await?;
    let localization = Arc::clone(&services.messages.localization);
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services);
    bot.dispatch().await;

    let responses = bot.get_responses();
    let welcome = t_lang(&localization, "welcome-title", Some("en"));
    let texts: Vec<&str> = responses
        .sent_messages
        .iter()
        .filter_map(|message| message.text())
        .collect();
    texts
        .iter()
        .find(|text| text.contains(&welcome))
        .with_context(|| format!("no welcome among {texts:?}"))?;
    Ok(())
}

/// A review stored as after a photo with two quantities to check, as on 2026-11-03.
async fn seed_review(services: &BotServices, chat: teloxide::types::ChatId) -> Result<()> {
    use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
    use just_ingredients::MeasurementMatch;
    let line = |quantity: &str, unit: Option<&str>, name: &str| MeasurementMatch {
        quantity: quantity.to_string(),
        measurement: unit.map(str::to_string),
        ingredient_name: name.to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 0,
        requires_quantity_confirmation: true,
        section: None,
    };
    RecipeDialogue::new(Arc::clone(&services.dialogues), chat)
        .update(RecipeDialogueState::ReviewIngredients {
            recipe_name: "Recipe".to_string(),
            ingredients: vec![
                line("1", None, "échalote ou 1 oignon, hachés fin"),
                line("1", Some("tbsp"), "persil ou de coriandre ciselée"),
            ],
            language_code: Some("fr".to_string()),
            message_id: None,
            extracted_text: String::new(),
            recipe_name_from_caption: None,
            open_menu: None,
        })
        .await?;
    Ok(())
}

async fn stored_review(
    services: &BotServices,
    chat: teloxide::types::ChatId,
) -> Result<just_ingredients::dialogue::RecipeDialogueState> {
    just_ingredients::dialogue::RecipeDialogue::new(Arc::clone(&services.dialogues), chat)
        .get()
        .await?
        .context("the review is still running")
}

/// Whether a message carries the review's buttons (✅ among them).
fn has_review_buttons(message: &teloxide::types::Message) -> bool {
    message.reply_markup().is_some_and(|keyboard| {
        keyboard.inline_keyboard.iter().flatten().any(|button| {
            matches!(&button.kind, teloxide::types::InlineKeyboardButtonKind::CallbackData(data) if data == "confirm")
        })
    })
}

/// The user's report of 2026-11-03: a ⚠️ line tapped (its menu opens), then the quantity
/// typed without choosing 🔢. The answer is that line's quantity, and the list comes down below
/// it with its buttons.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_quantity_typed_with_the_menu_open_fixes_that_line(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;

    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review(&services, chat).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("item_1").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    let state = stored_review(&services, chat).await?;
    assert!(
        matches!(
            state,
            RecipeDialogueState::ReviewIngredients {
                open_menu: Some(1),
                ..
            }
        ),
        "the menu of the second line is open: {state:?}"
    );

    bot.update(MockMessageText::new().text("1/2 cuillère à soupe"));
    bot.dispatch().await;
    let RecipeDialogueState::ReviewIngredients {
        ingredients,
        open_menu,
        message_id,
        ..
    } = stored_review(&services, chat).await?
    else {
        anyhow::bail!("not in the review any more");
    };
    assert_eq!(ingredients[1].quantity, "0.5");
    assert_eq!(ingredients[1].measurement.as_deref(), Some("tbsp"));
    assert!(
        !ingredients[1].requires_quantity_confirmation,
        "the ⚠️ goes"
    );
    assert!(
        ingredients[0].requires_quantity_confirmation,
        "the other line is unchanged"
    );
    assert_eq!(open_menu, None);

    let responses = bot.get_responses();
    let last = responses
        .sent_messages
        .last()
        .context("the list is sent again")?;
    assert!(
        has_review_buttons(last),
        "the list at the bottom has its buttons"
    );
    assert_eq!(
        message_id,
        Some(last.id.0),
        "the review follows the list at the bottom"
    );
    Ok(())
}

/// A text the review does not understand gets an answer that says what to tap, and the list
/// comes down below it with its buttons (instead of a copy without buttons).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_text_not_understood_brings_the_list_down(pool: PgPool) -> Result<()> {
    let services = services(&pool, Vec::new()).await?;
    let localization = Arc::clone(&services.messages.localization);
    let typed = MockMessageText::new().text("bonjour");
    let chat = typed.chat.id;
    seed_review(&services, chat).await?;

    let mut bot = mock_bot(typed, services.clone());
    bot.dispatch().await;

    let responses = bot.get_responses();
    let sent = &responses.sent_messages;
    let not_understood = t_lang(&localization, "review-not-understood", Some("fr"));
    let answer = sent
        .iter()
        .position(|message| message.text() == Some(not_understood.as_str()))
        .context("the not-understood answer")?;
    let list = sent.last().context("the list")?;
    assert!(sent.len() > answer + 1, "the list comes after the answer");
    assert!(has_review_buttons(list));
    Ok(())
}
