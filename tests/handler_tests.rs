//! Updates through the real handler tree (`bot::dispatch::handler_tree`), with a fake Telegram
//! server (`teloxide_tests`), a fake extractor and a throwaway database (backlog IMP-031).
//!
//! Like `db_tests`, they need Postgres: `cargo test --test handler_tests -- --include-ignored`
//! with `DATABASE_URL` set to a throwaway database (each test creates its own).

use std::sync::Arc;

use anyhow::{Context, Result};
use just_ingredients::bot::MessageServices;
use just_ingredients::bot::dispatch::{BotServices, handler_tree};
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
///
/// The bot gets a pool of its own that keeps no connection after use. `MockBot::dispatch` runs
/// the handlers in a runtime it creates and drops each time: a connection opened there and kept
/// idle belongs to a runtime that is gone, and the next query on it failed now and then ("A
/// Tokio 1.x context was found, but it is being shutdown", 2 runs in 25 on 2026-11-05). The
/// test's own `pool` is then only used by the test, in its runtime.
async fn services(pool: &PgPool, ingredients: Vec<ExtractedIngredient>) -> Result<BotServices> {
    just_ingredients::db::init_database_schema(pool).await?;
    let pool = Arc::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .after_release(|_connection, _meta| Box::pin(async { Ok(false) }))
            .connect_lazy_with((*pool.connect_options()).clone()),
    );
    Ok(BotServices {
        messages: MessageServices {
            pool: Arc::clone(&pool),
            localization: create_localization_manager()?,
            deduplicator: None,
            extractor: Arc::new(FakeExtractor { ingredients }),
            api: Arc::new(just_ingredients::bot::api_keys::ApiAccess {
                beta_users: vec![OWNER],
                realm: just_ingredients::engine::tokens::Realm::Staging,
                mcp_url: Some("https://staging.example/mcp".to_string()),
                oauth: false,
            }),
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
    seed_review_with(services, chat, true, None, None).await
}

/// A review stored as after a photo: `flagged` lines to check, the caption's name, the id of
/// the list message.
async fn seed_review_with(
    services: &BotServices,
    chat: teloxide::types::ChatId,
    flagged: bool,
    caption: Option<&str>,
    message_id: Option<i32>,
) -> Result<()> {
    use just_ingredients::MeasurementMatch;
    use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
    let line = |quantity: &str, unit: Option<&str>, name: &str| MeasurementMatch {
        quantity: quantity.to_string(),
        measurement: unit.map(str::to_string),
        ingredient_name: name.to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 0,
        requires_quantity_confirmation: flagged,
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
            message_id,
            extracted_text: String::new(),
            recipe_name_from_caption: caption.map(str::to_string),
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

/// The user of every mock update.
const OWNER: i64 = teloxide_tests::MockUser::ID as i64;

/// The running action of `chat`, if any.
async fn action(
    services: &BotServices,
    chat: teloxide::types::ChatId,
) -> Result<Option<just_ingredients::dialogue::RecipeDialogueState>> {
    Ok(
        just_ingredients::dialogue::RecipeDialogue::new(Arc::clone(&services.dialogues), chat)
            .get()
            .await?,
    )
}

/// The names of the owner's saved recipes, in creation order.
async fn recipe_names(pool: &PgPool) -> Result<Vec<String>> {
    let names: Vec<Option<String>> =
        sqlx::query_scalar("SELECT recipe_name FROM recipes WHERE telegram_id = $1 ORDER BY id")
            .bind(OWNER)
            .fetch_all(pool)
            .await?;
    Ok(names.into_iter().flatten().collect())
}

/// The callback data of a button sent by the bot whose data starts with `prefix`.
fn button_data(responses: &teloxide_tests::Responses, prefix: &str) -> Option<String> {
    let sent = responses.sent_messages.iter();
    let edited = responses
        .edited_messages_reply_markup
        .iter()
        .map(|edit| &edit.message)
        .chain(
            responses
                .edited_messages_text
                .iter()
                .map(|edit| &edit.message),
        );
    sent.chain(edited)
        .filter_map(|message| message.reply_markup())
        .flat_map(|keyboard| keyboard.inline_keyboard.iter().flatten())
        .find_map(|button| match &button.kind {
            teloxide::types::InlineKeyboardButtonKind::CallbackData(data)
                if data.starts_with(prefix) =>
            {
                Some(data.clone())
            }
            _ => None,
        })
}

/// ✅ on a review with the caption's name saves the recipe under that name; the action goes on
/// with the question about its note ("Passer" is its exit).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn confirm_with_a_caption_name_saves_the_recipe(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review_with(&services, chat, false, Some("Tarte aux poireaux"), None).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(list),
        services.clone(),
    );
    bot.dispatch().await;

    assert_eq!(recipe_names(&pool).await?, ["Tarte aux poireaux"]);
    assert!(
        matches!(
            action(&services, chat).await?,
            Some(RecipeDialogueState::EditingRecipeNote { creation: true, .. })
        ),
        "the note is asked next"
    );
    assert_eq!(
        button_data(&bot.get_responses(), "cancel_action").as_deref(),
        Some("cancel_action"),
        "with Passer as its exit"
    );
    Ok(())
}

/// ✅ without a name asks it; the typed name saves the recipe.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn confirm_without_a_name_asks_it_then_saves(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review_with(&services, chat, false, None, None).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::WaitingForRecipeNameAfterConfirm { .. })
    ));
    assert!(recipe_names(&pool).await?.is_empty(), "nothing saved yet");

    bot.update(MockMessageText::new().text("Gratin"));
    bot.dispatch().await;
    assert_eq!(recipe_names(&pool).await?, ["Gratin"]);
    assert!(
        matches!(
            action(&services, chat).await?,
            Some(RecipeDialogueState::EditingRecipeNote { creation: true, .. })
        ),
        "the note is asked next"
    );
    Ok(())
}

/// ❌ on a review saves nothing and ends the action.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn cancel_review_saves_nothing(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review_with(&services, chat, false, Some("Tarte"), None).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("cancel_review").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(recipe_names(&pool).await?.is_empty());
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// The ✅ of a list older than the running review's cannot save it: the button says it is no
/// longer active.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn an_older_list_cannot_save(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let older = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = older.chat.id;
    // The running review's list is a later message than the one pressed
    seed_review_with(&services, chat, false, Some("Tarte"), Some(older.id.0 + 10)).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(older),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(recipe_names(&pool).await?.is_empty(), "nothing saved");
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::ReviewIngredients { .. })
    ));
    let responses = bot.get_responses();
    let answer = responses
        .answered_callback_queries
        .last()
        .context("the button is answered")?;
    assert!(answer.text.is_some(), "with the no-longer-active text");
    Ok(())
}

/// A saved recipe edited from its card: its menu, a typed quantity, then ✅ writes it.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn editing_a_saved_recipe_writes_the_typed_quantity(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{NewIngredient, get_recipe_ingredients, save_recipe};
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(
        &pool,
        OWNER,
        Some("fr"),
        "",
        "Gratin",
        &[NewIngredient {
            name: "lait",
            quantity: Some(500.0),
            unit: Some("ml"),
        }],
    )
    .await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!(
                "recipe_action:edit_ingredients:{}",
                saved.recipe_id
            ))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    let view = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the editing view")?;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::EditingSavedIngredients { .. })
    ));

    bot.update(MockCallbackQuery::new().data("item_0").message(view));
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("250 ml"));
    bot.dispatch().await;
    let view = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the editing view, at the bottom")?;

    bot.update(MockCallbackQuery::new().data("confirm").message(view));
    bot.dispatch().await;
    let ingredients = get_recipe_ingredients(&pool, OWNER, saved.recipe_id).await?;
    assert_eq!(ingredients.len(), 1);
    assert_eq!(ingredients[0].quantity, Some(250.0));
    assert!(action(&services, chat).await?.is_none(), "the edit ended");
    Ok(())
}

/// A saved recipe renamed from its card.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn renaming_a_saved_recipe(pool: PgPool) -> Result<()> {
    use just_ingredients::db::save_recipe;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!("recipe_action:rename:{}", saved.recipe_id))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("Gratin dauphinois"));
    bot.dispatch().await;
    assert_eq!(recipe_names(&pool).await?, ["Gratin dauphinois"]);
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// A saved recipe deleted from its card, after the confirmation.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn deleting_a_saved_recipe_after_the_confirmation(pool: PgPool) -> Result<()> {
    use just_ingredients::db::save_recipe;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    let card = MockMessageText::new().text("Gratin").build();

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!("recipe_action:delete:{}", saved.recipe_id))
            .message(card.clone()),
        services.clone(),
    );
    bot.dispatch().await;
    assert_eq!(
        recipe_names(&pool).await?,
        ["Gratin"],
        "not before the confirmation"
    );
    let confirm = button_data(&bot.get_responses(), "confirm_delete_recipe:")
        .context("the confirmation button")?;

    bot.update(MockCallbackQuery::new().data(confirm).message(card));
    bot.dispatch().await;
    assert!(recipe_names(&pool).await?.is_empty());
    Ok(())
}

/// Whether a message sent or edited by the bot has a button with exactly this data.
fn has_button(responses: &teloxide_tests::Responses, data: &str) -> bool {
    button_data(responses, data).as_deref() == Some(data)
}

/// Saves recipes of the owner with no ingredient; returns their ids.
async fn save_recipes(pool: &PgPool, names: &[&str]) -> Result<Vec<i64>> {
    let mut ids = Vec::new();
    for name in names {
        ids.push(
            just_ingredients::db::save_recipe(pool, OWNER, Some("fr"), "", name, &[])
                .await?
                .recipe_id,
        );
    }
    Ok(ids)
}

/// A photo in parts (U9): ✅ saves the main recipe and its part, linked to each other.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_review_in_parts_saves_linked_recipes(pool: PgPool) -> Result<()> {
    use just_ingredients::MeasurementMatch;
    use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    let line = |name: &str, section: Option<&str>| MeasurementMatch {
        quantity: "100".to_string(),
        measurement: Some("g".to_string()),
        ingredient_name: name.to_string(),
        line_number: 0,
        start_pos: 0,
        end_pos: 0,
        requires_quantity_confirmation: false,
        section: section.map(str::to_string),
    };
    RecipeDialogue::new(Arc::clone(&services.dialogues), chat)
        .update(RecipeDialogueState::ReviewIngredients {
            recipe_name: "Recipe".to_string(),
            ingredients: vec![line("farine", None), line("sucre", Some("Pour la sauce"))],
            language_code: Some("fr".to_string()),
            message_id: None,
            extracted_text: String::new(),
            recipe_name_from_caption: Some("Malva pudding".to_string()),
            open_menu: None,
        })
        .await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(list),
        services.clone(),
    );
    bot.dispatch().await;

    // The part is named after its cleaned heading ("Pour la sauce" → "sauce")
    assert_eq!(
        recipe_names(&pool).await?,
        ["Malva pudding", "Malva pudding (sauce)"]
    );
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM recipe_links")
        .fetch_one(&pool)
        .await?;
    assert_eq!(links, 1, "the two parts are linked");
    Ok(())
}

/// `/unites` shows the views; choosing one stores the preference.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn choosing_a_unit_view_stores_it(pool: PgPool) -> Result<()> {
    use just_ingredients::unit_view::UnitView;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;

    let mut bot = mock_bot(MockMessageText::new().text("/unites"), services.clone());
    bot.dispatch().await;
    let responses = bot.get_responses();
    let us = button_data(&responses, "unit_view:us:").context("the US view button")?;
    let chooser = responses
        .sent_messages
        .last()
        .cloned()
        .context("the chooser")?;

    bot.update(MockCallbackQuery::new().data(us).message(chooser));
    bot.dispatch().await;
    assert_eq!(
        just_ingredients::db::unit_view(&pool, OWNER).await?,
        UnitView::Us
    );
    Ok(())
}

/// `/cherche` finds the recipe whose name has the word, not the others.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn search_finds_the_recipe_with_the_word(pool: PgPool) -> Result<()> {
    let services = services(&pool, Vec::new()).await?;
    let ids = save_recipes(&pool, &["Tarte aux poireaux", "Gratin dauphinois"]).await?;

    let mut bot = mock_bot(
        MockMessageText::new().text("/cherche poireaux"),
        services.clone(),
    );
    bot.dispatch().await;
    let responses = bot.get_responses();
    assert!(has_button(
        &responses,
        &format!("recipe_instance:{}", ids[0])
    ));
    assert!(!has_button(
        &responses,
        &format!("recipe_instance:{}", ids[1])
    ));
    Ok(())
}

/// The recipe list shows 8 recipes a page: the ninth is on the next page.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_ninth_recipe_is_on_the_next_page(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let names: Vec<String> = (1..=9).map(|n| format!("Recette {n:02}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let ids = save_recipes(&pool, &names).await?;
    let ninth = format!("recipe_instance:{}", ids[8]);

    let mut bot = mock_bot(MockMessageText::new().text("/liste"), services.clone());
    bot.dispatch().await;
    let responses = bot.get_responses();
    assert!(has_button(
        &responses,
        &format!("recipe_instance:{}", ids[0])
    ));
    assert!(!has_button(&responses, &ninth), "not on the first page");
    let next = button_data(&responses, "page:").context("a page button")?;
    let list = responses
        .sent_messages
        .last()
        .cloned()
        .context("the list")?;

    bot.update(MockCallbackQuery::new().data(next).message(list));
    bot.dispatch().await;
    assert!(has_button(&bot.get_responses(), &ninth), "on the next page");
    Ok(())
}

/// Outside an action, a word typed is a search in the user's recipes (U4.2).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn free_text_outside_an_action_is_a_search(pool: PgPool) -> Result<()> {
    let services = services(&pool, Vec::new()).await?;
    let ids = save_recipes(&pool, &["Tarte aux poireaux", "Gratin dauphinois"]).await?;

    let mut bot = mock_bot(MockMessageText::new().text("gratin"), services.clone());
    bot.dispatch().await;
    let responses = bot.get_responses();
    assert!(has_button(
        &responses,
        &format!("recipe_instance:{}", ids[1])
    ));
    assert!(!has_button(
        &responses,
        &format!("recipe_instance:{}", ids[0])
    ));
    Ok(())
}

/// A pasted recipe (several lines) is no search: it gets the reminder that recipes come from
/// photos.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_pasted_recipe_gets_the_photo_tip(pool: PgPool) -> Result<()> {
    let services = services(&pool, Vec::new()).await?;
    let localization = Arc::clone(&services.messages.localization);
    save_recipes(&pool, &["Tarte aux poireaux"]).await?;

    let mut bot = mock_bot(
        MockMessageText::new().text("200 g de farine\n3 œufs\n50 cl de lait"),
        services.clone(),
    );
    bot.dispatch().await;
    let tip = t_lang(&localization, "free-text-tip", None);
    assert!(
        bot.get_responses()
            .sent_messages
            .iter()
            .any(|message| message.text() == Some(tip.as_str())),
        "the photo tip"
    );
    Ok(())
}

/// 🗑️ on a review's line removes it; removing the last one says the list is empty, with ➕.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn deleting_review_lines_down_to_none(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review(&services, chat).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data("delete_0")
            .message(list.clone()),
        services.clone(),
    );
    bot.dispatch().await;
    let RecipeDialogueState::ReviewIngredients { ingredients, .. } =
        stored_review(&services, chat).await?
    else {
        anyhow::bail!("not in the review any more");
    };
    assert_eq!(ingredients.len(), 1);
    assert_eq!(
        ingredients[0].ingredient_name,
        "persil ou de coriandre ciselée"
    );

    bot.update(MockCallbackQuery::new().data("delete_0").message(list));
    bot.dispatch().await;
    let RecipeDialogueState::ReviewIngredients { ingredients, .. } =
        stored_review(&services, chat).await?
    else {
        anyhow::bail!("not in the review any more");
    };
    assert!(ingredients.is_empty());
    assert!(
        has_button(&bot.get_responses(), "add_more"),
        "the empty list offers ➕"
    );
    Ok(())
}

/// 🗑️ on a saved recipe's ingredient, then ✅, deletes it from the recipe.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn deleting_an_ingredient_of_a_saved_recipe(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{NewIngredient, get_recipe_ingredients, save_recipe};
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let ingredient = |name| NewIngredient {
        name,
        quantity: Some(1.0),
        unit: None,
    };
    let saved = save_recipe(
        &pool,
        OWNER,
        Some("fr"),
        "",
        "Gratin",
        &[ingredient("lait"), ingredient("beurre")],
    )
    .await?;
    let card = MockMessageText::new().text("Gratin").build();

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!(
                "recipe_action:edit_ingredients:{}",
                saved.recipe_id
            ))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    let view = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the editing view")?;
    bot.update(
        MockCallbackQuery::new()
            .data("delete_0")
            .message(view.clone()),
    );
    bot.dispatch().await;
    bot.update(MockCallbackQuery::new().data("confirm").message(view));
    bot.dispatch().await;

    let left = get_recipe_ingredients(&pool, OWNER, saved.recipe_id).await?;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "beurre");
    Ok(())
}

/// A new name over the limit is refused with its own message; the rename waits for another.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_too_long_new_name_is_refused(pool: PgPool) -> Result<()> {
    use just_ingredients::db::save_recipe;
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let localization = Arc::clone(&services.messages.localization);
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!("recipe_action:rename:{}", saved.recipe_id))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("G".repeat(300)));
    bot.dispatch().await;

    assert_eq!(recipe_names(&pool).await?, ["Gratin"], "not renamed");
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::RenamingRecipe { .. })
    ));
    let too_long = t_lang(&localization, "recipe-name-too-long", None);
    assert!(
        bot.get_responses()
            .sent_messages
            .iter()
            .any(|message| message.text() == Some(too_long.as_str()))
    );
    Ok(())
}

/// 🔢 on a review line opens its prompt; the typed quantity fixes that line.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_quantity_prompt_of_a_review_line(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review(&services, chat).await?;

    let mut bot = mock_bot(
        MockCallbackQuery::new().data("qty_0").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::EditingIngredient {
            editing_index: 0,
            ..
        })
    ));

    bot.update(MockMessageText::new().text("4"));
    bot.dispatch().await;
    let RecipeDialogueState::ReviewIngredients { ingredients, .. } =
        stored_review(&services, chat).await?
    else {
        anyhow::bail!("back in the review");
    };
    assert_eq!(ingredients[0].quantity, "4");
    assert!(!ingredients[0].requires_quantity_confirmation);
    Ok(())
}

/// 🔢 on a saved recipe's line opens its prompt; the typed quantity, then ✅, writes it.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_quantity_prompt_of_a_saved_line(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{NewIngredient, get_recipe_ingredients, save_recipe};
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(
        &pool,
        OWNER,
        Some("fr"),
        "",
        "Gratin",
        &[NewIngredient {
            name: "lait",
            quantity: Some(500.0),
            unit: Some("ml"),
        }],
    )
    .await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!(
                "recipe_action:edit_ingredients:{}",
                saved.recipe_id
            ))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    let view = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the editing view")?;
    bot.update(MockCallbackQuery::new().data("qty_0").message(view.clone()));
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::EditingSavedIngredient {
            editing_index: 0,
            ..
        })
    ));

    bot.update(MockMessageText::new().text("300"));
    bot.dispatch().await;
    bot.update(MockCallbackQuery::new().data("confirm").message(view));
    bot.dispatch().await;
    let ingredients = get_recipe_ingredients(&pool, OWNER, saved.recipe_id).await?;
    assert_eq!(ingredients[0].quantity, Some(300.0));
    assert_eq!(ingredients[0].unit.as_deref(), Some("ml"), "the unit stays");
    Ok(())
}

/// The note of the owner's recipe `name`.
async fn note_of(pool: &PgPool, name: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT note FROM recipes WHERE telegram_id = $1 AND recipe_name = $2")
            .bind(OWNER)
            .bind(name)
            .fetch_one(pool)
            .await?,
    )
}

/// Whether the bot sent a message with exactly this text.
fn sent(
    bot: &MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>,
    text: &str,
) -> bool {
    bot.get_responses()
        .sent_messages
        .iter()
        .any(|message| message.text() == Some(text))
}

/// ✅ with the caption's name, ready to answer the note question: the bot, the services, and the
/// chat.
async fn creation_asking_the_note(
    pool: &PgPool,
) -> Result<(
    MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>,
    BotServices,
    teloxide::types::ChatId,
)> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review_with(&services, chat, false, Some("Tarte"), None).await?;
    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    Ok((bot, services, chat))
}

/// The note typed right after a creation is saved clean, on one line, and ends the action.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_note_typed_after_a_creation_is_saved_clean(pool: PgPool) -> Result<()> {
    let (mut bot, services, chat) = creation_asking_the_note(&pool).await?;
    assert_eq!(note_of(&pool, "Tarte").await?, None);

    bot.update(MockMessageText::new().text("  Four 180 °C\n\n25 min \u{200B}<b>"));
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Tarte").await?.as_deref(),
        Some("Four 180 °C 25 min <b>"),
        "one line; markup kept as text"
    );
    assert!(
        action(&services, chat).await?.is_none(),
        "the creation ended"
    );
    Ok(())
}

/// "Passer" keeps the recipe without a note and ends the action.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn skipping_the_note_question_keeps_the_recipe(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let (mut bot, services, chat) = creation_asking_the_note(&pool).await?;

    let prompt = MockMessageText::new().text("📌 Un mémo").build();
    bot.update(
        MockCallbackQuery::new()
            .data("cancel_action")
            .message(prompt),
    );
    bot.dispatch().await;
    assert_eq!(recipe_names(&pool).await?, ["Tarte"]);
    assert_eq!(note_of(&pool, "Tarte").await?, None);
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// A cancel word typed at the question after a creation is "Passer": nothing is cancelled, the
/// recipe is saved already.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_cancel_word_at_the_note_question_skips_it(pool: PgPool) -> Result<()> {
    let (mut bot, services, chat) = creation_asking_the_note(&pool).await?;

    bot.update(MockMessageText::new().text("annuler"));
    bot.dispatch().await;
    assert_eq!(recipe_names(&pool).await?, ["Tarte"]);
    assert_eq!(note_of(&pool, "Tarte").await?, None);
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// A note that is too long or empty after cleaning is refused with its reason: nothing is
/// written, the question stays open for another answer.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_refused_note_keeps_the_question_open(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    let (mut bot, services, chat) = creation_asking_the_note(&pool).await?;
    let localization = Arc::clone(&services.messages.localization);

    bot.update(MockMessageText::new().text("x".repeat(201)));
    bot.dispatch().await;
    assert!(sent(
        &bot,
        &t_lang(&localization, "note-too-long", Some("fr"))
    ));
    bot.update(MockMessageText::new().text("\u{200B}\u{FEFF}"));
    bot.dispatch().await;
    assert!(sent(&bot, &t_lang(&localization, "note-empty", Some("fr"))));

    assert_eq!(note_of(&pool, "Tarte").await?, None, "nothing written");
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::EditingRecipeNote { creation: true, .. })
    ));
    // And a good answer still works
    bot.update(MockMessageText::new().text("Four 180 °C"));
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Tarte").await?.as_deref(),
        Some("Four 180 °C")
    );
    Ok(())
}

/// ✅ without a caption name: the typed name saves the recipe, then the note is asked and saved.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_note_follows_a_typed_name(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let list = MockMessageText::new().text("📝 Ingrédients").build();
    let chat = list.chat.id;
    seed_review_with(&services, chat, false, None, None).await?;
    let mut bot = mock_bot(
        MockCallbackQuery::new().data("confirm").message(list),
        services.clone(),
    );
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("Gratin"));
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("Four 200 °C, 35 min"));
    bot.dispatch().await;

    assert_eq!(recipe_names(&pool).await?, ["Gratin"]);
    assert_eq!(
        note_of(&pool, "Gratin").await?.as_deref(),
        Some("Four 200 °C, 35 min")
    );
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// The 📌 button of a card: the typed note replaces the old one; 🗑 clears it; ❌ leaves it.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_note_of_a_saved_recipe_is_written_cleared_or_kept(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{save_recipe, set_recipe_note};
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    set_recipe_note(&pool, OWNER, saved.recipe_id, Some("Four 180 °C")).await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;
    let ask = || {
        MockCallbackQuery::new()
            .data(format!("recipe_action:note:{}", saved.recipe_id))
            .message(MockMessageText::new().text("Gratin").build())
    };

    let mut bot = mock_bot(ask(), services.clone());
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::EditingRecipeNote {
            creation: false,
            ..
        })
    ));
    assert!(
        bot.get_responses()
            .sent_messages
            .iter()
            .any(|message| message
                .text()
                .is_some_and(|text| text.contains("Four 180 °C"))),
        "the question shows the current note"
    );
    assert_eq!(
        button_data(&bot.get_responses(), "clear_recipe_note:").as_deref(),
        Some(format!("clear_recipe_note:{}", saved.recipe_id).as_str()),
        "the clear button names its recipe"
    );

    // Replaced
    bot.update(MockMessageText::new().text("Four 200 °C, 30 min"));
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Gratin").await?.as_deref(),
        Some("Four 200 °C, 30 min")
    );
    assert!(action(&services, chat).await?.is_none());

    // Left with ❌: unchanged
    bot.update(ask());
    bot.dispatch().await;
    bot.update(
        MockCallbackQuery::new()
            .data("cancel_action")
            .message(MockMessageText::new().text("📌").build()),
    );
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Gratin").await?.as_deref(),
        Some("Four 200 °C, 30 min")
    );
    assert!(action(&services, chat).await?.is_none());

    // Cleared with 🗑
    bot.update(ask());
    bot.dispatch().await;
    bot.update(
        MockCallbackQuery::new()
            .data(format!("clear_recipe_note:{}", saved.recipe_id))
            .message(MockMessageText::new().text("📌").build()),
    );
    bot.dispatch().await;
    assert_eq!(note_of(&pool, "Gratin").await?, None);
    assert!(action(&services, chat).await?.is_none());
    Ok(())
}

/// A card's note button for a recipe that does not exist (or is another user's) starts nothing;
/// a 🗑 pressed with no note question running changes nothing.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn stale_or_forged_note_buttons_do_nothing(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{save_recipe, set_recipe_note};
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    set_recipe_note(&pool, OWNER, saved.recipe_id, Some("Four 180 °C")).await?;
    let message = MockMessageText::new().text("Gratin").build();
    let chat = message.chat.id;

    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!("recipe_action:note:{}", saved.recipe_id + 1000))
            .message(message.clone()),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(action(&services, chat).await?.is_none(), "no such recipe");

    let clear = |recipe_id: i64| {
        MockCallbackQuery::new()
            .data(format!("clear_recipe_note:{recipe_id}"))
            .message(message.clone())
    };
    bot.update(clear(saved.recipe_id));
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Gratin").await?.as_deref(),
        Some("Four 180 °C"),
        "no question was running"
    );

    // A question runs for this recipe: the button of another recipe's question does nothing
    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_action:note:{}", saved.recipe_id))
            .message(message.clone()),
    );
    bot.dispatch().await;
    bot.update(clear(saved.recipe_id + 1));
    bot.dispatch().await;
    assert_eq!(
        note_of(&pool, "Gratin").await?.as_deref(),
        Some("Four 180 °C"),
        "another recipe's button"
    );
    assert!(
        action(&services, chat).await?.is_some(),
        "the question stays"
    );
    Ok(())
}

/// The active API tokens of the owner, as stored.
async fn active_tokens(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM api_tokens t JOIN users u ON u.id = t.user_id \
         WHERE u.telegram_id = $1 AND t.revoked_at IS NULL",
    )
    .bind(OWNER)
    .fetch_one(pool)
    .await?)
}

/// `/cle` for a beta user: the keys message; "➕ Nouvelle clé" creates a token, shown once in a
/// protected message (monospace under a spoiler), recorded to be masked, and the list comes
/// back with its revoke button; "🗑️ Révoquer" revokes it.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_beta_user_creates_and_revokes_an_api_key(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let mut bot = mock_bot(MockMessageText::new().text("/cle"), services.clone());
    bot.dispatch().await;
    let keys = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the keys message")?;
    assert_eq!(
        button_data(&bot.get_responses(), "api_key_new").as_deref(),
        Some("api_key_new")
    );

    bot.update(MockCallbackQuery::new().data("api_key_new").message(keys));
    bot.dispatch().await;
    assert_eq!(active_tokens(&pool).await?, 1);
    let responses = bot.get_responses();
    let secret = responses
        .sent_messages_text
        .iter()
        .find(|sent| sent.bot_request.text.contains("<tg-spoiler><code>ji_stg_"))
        .context("the token message")?;
    assert_ne!(
        secret.bot_request.protect_content,
        Some(true),
        "a protected message cannot be copied, the token would be useless"
    );
    assert!(
        secret
            .bot_request
            .text
            .contains("https://staging.example/mcp"),
        "the server's address"
    );
    let recorded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages WHERE kind = 'secret' AND message_id = $1",
    )
    .bind(i64::from(secret.message.id.0))
    .fetch_one(&pool)
    .await?;
    assert_eq!(recorded, 1, "recorded to be masked");
    let revoke = button_data(&responses, "api_key_revoke:").context("the revoke button")?;
    let list = responses
        .sent_messages
        .last()
        .cloned()
        .context("the keys message again")?;
    let token = secret
        .bot_request
        .text
        .split("<code>")
        .nth(1)
        .and_then(|rest| rest.split("</code>").next())
        .context("the token")?;
    assert_eq!(token.len(), "ji_stg_".len() + 43);
    let shown = list.text().context("the list's text")?;
    assert!(
        !shown.contains(token),
        "the list shows hints, never the secret"
    );
    assert!(shown.contains(&token[..13]), "but the hint: {shown}");

    bot.update(MockCallbackQuery::new().data(revoke).message(list));
    bot.dispatch().await;
    assert_eq!(active_tokens(&pool).await?, 0, "revoked");
    Ok(())
}

/// `/cle` lists an assistant connected with OAuth; "🔌 Déconnecter" revokes its grant: its token
/// no longer opens the door.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_connected_assistant_is_listed_and_disconnected(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::oauth::{
        self, Callback, CodeExchange, Decision, SignInRequest, SignInSecrets,
    };
    use just_ingredients::engine::tokens::{self, Checked, Realm};
    use just_ingredients::secret::SecretString;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;

    // claude.ai connected by the owner, reading only
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let client = "https://claude.ai/oauth/mcp-oauth-client-metadata";
    let callback = "https://claude.ai/api/mcp/auth_callback";
    let resource = "https://staging.example/mcp";
    let telegram_verifier = SecretString::new("t");
    oauth::start_sign_in(
        &pool,
        &SignInRequest {
            client_id: client.to_string(),
            redirect_uri: callback.to_string(),
            client_state: None,
            code_challenge: oauth::pkce_challenge(verifier),
            scopes: Vec::new(),
            resource: resource.to_string(),
        },
        &SignInSecrets {
            state: "s",
            binding: "b",
            verifier: &telegram_verifier,
            nonce: "n",
        },
    )
    .await?;
    let Callback::Pending(pending) = oauth::find_sign_in(&pool, "s", Some("b")).await? else {
        anyhow::bail!("pending");
    };
    oauth::signed_in(
        &pool,
        pending.id,
        just_ingredients::engine::Owner::telegram(OWNER),
        None,
    )
    .await?;
    oauth::open_consent(&pool, "b", "f").await?;
    let code = oauth::decide(&pool, "b", "f", Decision::Allow { write: false })
        .await?
        .and_then(|decided| decided.code)
        .context("a code")?;
    let issued = oauth::redeem_code(
        &pool,
        Realm::Staging,
        CodeExchange {
            code: code.expose(),
            client_id: client,
            redirect_uri: callback,
            verifier,
            resource: None,
        },
    )
    .await?;

    let mut bot = mock_bot(MockMessageText::new().text("/cle"), services.clone());
    bot.dispatch().await;
    let responses = bot.get_responses();
    let keys = responses
        .sent_messages
        .last()
        .cloned()
        .context("the keys message")?;
    let shown = keys.text().context("its text")?;
    let localization = Arc::clone(&services.messages.localization);
    assert!(
        shown.contains(&t_lang(&localization, "api-grants-title", None)),
        "{shown}"
    );
    let read_only = t_lang(&localization, "api-keys-access-read", None);
    assert!(
        shown.contains(&format!("claude.ai ({read_only})")),
        "{shown}"
    );
    let disconnect =
        button_data(&responses, "api_grant_revoke:").context("the disconnect button")?;
    assert_eq!(disconnect, format!("api_grant_revoke:{}", issued.grant_id));

    bot.update(MockCallbackQuery::new().data(disconnect).message(keys));
    bot.dispatch().await;
    assert!(
        oauth::list_grants(&pool, just_ingredients::engine::Owner::telegram(OWNER))
            .await?
            .is_empty()
    );
    assert_eq!(
        tokens::check(
            &pool,
            Realm::Staging,
            issued.access_token.expose(),
            Some(resource)
        )
        .await?,
        Checked::Known,
        "the token no longer opens the door"
    );
    let edited = bot
        .get_responses()
        .edited_messages_text
        .last()
        .cloned()
        .context("the list again")?;
    assert!(
        !edited
            .message
            .text()
            .unwrap_or_default()
            .contains("claude.ai ("),
        "no longer listed"
    );
    Ok(())
}

/// Outside the beta, `/cle` is an unknown command and a forged button creates nothing.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn outside_the_beta_there_are_no_api_keys(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let mut services = services(&pool, Vec::new()).await?;
    services.messages.api = Arc::new(just_ingredients::bot::api_keys::ApiAccess::default());
    let localization = Arc::clone(&services.messages.localization);
    let mut bot = mock_bot(MockMessageText::new().text("/key"), services.clone());
    bot.dispatch().await;
    let unknown = t_lang(&localization, "unknown-command", None);
    assert!(bot.get_responses().sent_messages.iter().any(|message| {
        message
            .text()
            .is_some_and(|text| text.starts_with(&unknown))
    }));
    assert!(button_data(&bot.get_responses(), "api_key_new").is_none());

    bot.update(
        MockCallbackQuery::new()
            .data("api_key_new")
            .message(MockMessageText::new().text("🔑").build()),
    );
    bot.dispatch().await;
    assert_eq!(
        active_tokens(&pool).await?,
        0,
        "a forged button creates nothing"
    );
    Ok(())
}

/// "➕ Nouvelle clé" pressed while another action runs (a rename prompt is open) ends it, like a
/// command: the next text is not taken as the recipe's new name.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_new_key_ends_the_running_action(pool: PgPool) -> Result<()> {
    use just_ingredients::db::save_recipe;
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let saved = save_recipe(&pool, OWNER, Some("fr"), "", "Gratin", &[]).await?;
    let card = MockMessageText::new().text("Gratin").build();
    let chat = card.chat.id;
    let mut bot = mock_bot(
        MockCallbackQuery::new()
            .data(format!("recipe_action:rename:{}", saved.recipe_id))
            .message(card),
        services.clone(),
    );
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::RenamingRecipe { .. })
    ));

    bot.update(
        MockCallbackQuery::new()
            .data("api_key_new")
            .message(MockMessageText::new().text("🔑").build()),
    );
    bot.dispatch().await;
    assert_eq!(active_tokens(&pool).await?, 1);
    assert!(action(&services, chat).await?.is_none(), "the rename ended");
    bot.update(MockMessageText::new().text("gratin"));
    bot.dispatch().await;
    assert_eq!(
        recipe_names(&pool).await?,
        ["Gratin"],
        "not renamed by the next text"
    );
    Ok(())
}

/// Opens `recipe_id` from a list button: the card is sent at the bottom, recorded as active.
async fn open_card(
    bot: &mut MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>,
    recipe_id: i64,
) -> Result<teloxide::types::Message> {
    use teloxide_tests::MockCallbackQuery;
    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_instance:{recipe_id}"))
            .message(MockMessageText::new().text("📚").build()),
    );
    bot.dispatch().await;
    bot.get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the card")
}

/// The text of the last message the bot edited.
fn last_edit(bot: &MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>) -> String {
    bot.get_responses()
        .edited_messages_text
        .last()
        .and_then(|edit| edit.message.text().map(str::to_string))
        .unwrap_or_default()
}

/// The callback data of a keyboard's buttons, row by row.
fn callbacks(keyboard: Option<&teloxide::types::InlineKeyboardMarkup>) -> Vec<String> {
    keyboard
        .map(|keyboard| {
            keyboard
                .inline_keyboard
                .iter()
                .flatten()
                .filter_map(|button| match &button.kind {
                    teloxide::types::InlineKeyboardButtonKind::CallbackData(data) => {
                        Some(data.clone())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The buttons of the last message the bot edited with its text.
fn text_buttons(
    bot: &MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>,
) -> Vec<String> {
    callbacks(
        bot.get_responses()
            .edited_messages_text
            .last()
            .and_then(|edit| edit.message.reply_markup()),
    )
}

/// The buttons of the last message the bot edited without its text.
fn markup_buttons(
    bot: &MockBot<anyhow::Error, teloxide_tests::mock_bot::DistributionKey>,
) -> Vec<String> {
    callbacks(
        bot.get_responses()
            .edited_messages_reply_markup
            .last()
            .and_then(|edit| edit.message.reply_markup()),
    )
}

/// The scale recorded for card `id`.
async fn card_scale(pool: &PgPool, id: teloxide::types::MessageId) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar(
            "SELECT scale FROM chat_messages WHERE chat_id = $1 AND message_id = $2",
        )
        .bind(OWNER)
        .bind(i64::from(id.0))
        .fetch_one(pool)
        .await?,
    )
}

/// A tart of 3 eggs and 250 g of flour, saved.
async fn save_tart(pool: &PgPool) -> Result<i64> {
    use just_ingredients::db::{NewIngredient, save_recipe};
    let saved = save_recipe(
        pool,
        OWNER,
        Some("fr"),
        "",
        "Tarte",
        &[
            NewIngredient {
                name: "oeufs",
                quantity: Some(3.0),
                unit: None,
            },
            NewIngredient {
                name: "farine",
                quantity: Some(250.0),
                unit: Some("g"),
            },
        ],
    )
    .await?;
    Ok(saved.recipe_id)
}

/// ⚖️ on a card: the menu stays open while the user adjusts, each tap updating the card above
/// (× 2, × 2 again is × 4, ÷ 2); ◀️ gives the card its buttons back, its ↩️ shows it as saved.
/// The recipe is never changed.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_card_is_adjusted_in_the_open_menu_then_shown_as_saved(pool: PgPool) -> Result<()> {
    use just_ingredients::db::get_recipe_ingredients;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let recipe_id = save_tart(&pool).await?;
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services.clone());
    let card = open_card(&mut bot, recipe_id).await?;
    let press = |data: String| MockCallbackQuery::new().data(data).message(card.clone());

    bot.update(press(format!("recipe_scale:menu:{recipe_id}")));
    bot.dispatch().await;
    assert_eq!(
        markup_buttons(&bot),
        [
            format!("recipe_scale:pick:{recipe_id}:1/2"),
            format!("recipe_scale:pick:{recipe_id}:2/1"),
            format!("recipe_scale:ask:{recipe_id}"),
            format!("recipe_scale:back:{recipe_id}"),
        ],
        "÷ 2, × 2, Autre…, ◀️"
    );

    bot.update(press(format!("recipe_scale:pick:{recipe_id}:2/1")));
    bot.dispatch().await;
    let doubled = last_edit(&bot);
    assert!(doubled.contains("× 2"), "{doubled}");
    assert!(doubled.contains("500 g"), "{doubled}");
    assert_eq!(card_scale(&pool, card.id).await?.as_deref(), Some("2/1"));
    assert_eq!(
        text_buttons(&bot),
        [
            format!("recipe_scale:pick:{recipe_id}:1/1"),
            format!("recipe_scale:pick:{recipe_id}:4/1"),
            format!("recipe_scale:ask:{recipe_id}"),
            format!("recipe_scale:pick:{recipe_id}:1/1"),
            format!("recipe_scale:back:{recipe_id}"),
        ],
        "the menu stays open, from × 2: ÷ 2, × 2, Autre…, ↩️, ◀️"
    );

    bot.update(press(format!("recipe_scale:pick:{recipe_id}:4/1")));
    bot.dispatch().await;
    let quadrupled = last_edit(&bot);
    assert!(quadrupled.contains("× 4"), "{quadrupled}");
    assert!(quadrupled.contains("12 oeufs"), "{quadrupled}");
    assert_eq!(card_scale(&pool, card.id).await?.as_deref(), Some("4/1"));

    bot.update(press(format!("recipe_scale:pick:{recipe_id}:2/1")));
    bot.dispatch().await;
    assert!(last_edit(&bot).contains("500 g"), "÷ 2 of × 4");

    bot.update(press(format!("recipe_scale:back:{recipe_id}")));
    bot.dispatch().await;
    assert!(
        markup_buttons(&bot).contains(&format!("recipe_scale:set:{recipe_id}:1/1")),
        "the card's buttons, with ↩️: {:?}",
        markup_buttons(&bot)
    );

    bot.update(press(format!("recipe_scale:set:{recipe_id}:1/1")));
    bot.dispatch().await;
    let saved = last_edit(&bot);
    assert!(saved.contains("250 g"), "{saved}");
    assert!(!saved.contains("⚖️ "), "{saved}");
    assert_eq!(card_scale(&pool, card.id).await?, None);

    let ingredients = get_recipe_ingredients(&pool, OWNER, recipe_id).await?;
    let mut quantities: Vec<_> = ingredients.iter().filter_map(|i| i.quantity).collect();
    quantities.sort_by(f64::total_cmp);
    assert_eq!(quantities, [3.0, 250.0], "the recipe is unchanged");
    Ok(())
}

/// "Autre…": a scale not understood keeps the question open and says why; "4 oeufs" shows the
/// card at 4/3 at the bottom, recorded with its scale; a rename afterwards keeps the scale.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_typed_scale_is_read_refused_or_shown(pool: PgPool) -> Result<()> {
    use just_ingredients::dialogue::RecipeDialogueState;
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let localization = Arc::clone(&services.messages.localization);
    let recipe_id = save_tart(&pool).await?;
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services.clone());
    let card = open_card(&mut bot, recipe_id).await?;
    let chat = card.chat.id;

    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_scale:ask:{recipe_id}"))
            .message(card.clone()),
    );
    bot.dispatch().await;
    assert!(matches!(
        action(&services, chat).await?,
        Some(RecipeDialogueState::ScalingRecipe { .. })
    ));

    bot.update(MockMessageText::new().text("beaucoup"));
    bot.dispatch().await;
    let refused = t_lang(&localization, "scale-not-understood", None);
    assert!(
        bot.get_responses()
            .sent_messages
            .iter()
            .any(|message| message.text().is_some_and(|text| text.contains(&refused))),
        "the refusal says why"
    );
    assert!(
        matches!(
            action(&services, chat).await?,
            Some(RecipeDialogueState::ScalingRecipe { .. })
        ),
        "the question stays open"
    );

    bot.update(MockMessageText::new().text("4 oeufs"));
    bot.dispatch().await;
    assert!(action(&services, chat).await?.is_none());
    let scaled = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the scaled card")?;
    let text = scaled.text().unwrap_or_default().to_string();
    assert!(text.contains("× 4/3"), "{text}");
    assert!(text.contains("3 → 4"), "{text}");
    assert!(text.contains("335 g"), "{text}");
    assert_eq!(card_scale(&pool, scaled.id).await?.as_deref(), Some("4/3"));
    let menu = callbacks(scaled.reply_markup());
    assert_eq!(
        menu[..2],
        [
            format!("recipe_scale:pick:{recipe_id}:2/3"),
            format!("recipe_scale:pick:{recipe_id}:8/3"),
        ],
        "still adjusting: the menu is open, from × 4/3"
    );

    // A rename refreshes the card at its scale
    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_action:rename:{recipe_id}"))
            .message(scaled.clone()),
    );
    bot.dispatch().await;
    bot.update(MockMessageText::new().text("Tarte aux oeufs"));
    bot.dispatch().await;
    let refreshed = bot
        .get_responses()
        .edited_messages_text
        .iter()
        .rev()
        .find(|edit| edit.message.id == scaled.id)
        .and_then(|edit| edit.message.text().map(str::to_string))
        .context("the card refreshed")?;
    assert!(refreshed.contains("Tarte aux oeufs"), "{refreshed}");
    assert!(refreshed.contains("× 4/3"), "{refreshed}");
    assert!(refreshed.contains("335 g"), "{refreshed}");
    Ok(())
}

/// ❌ on the scale question of a doubled card: the question ends, the card gets its buttons
/// back still doubled (it went into the history while the question was open).
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_scale_question_cancelled_keeps_the_card_as_it_was(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let recipe_id = save_tart(&pool).await?;
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services.clone());
    let card = open_card(&mut bot, recipe_id).await?;
    let chat = card.chat.id;
    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_scale:set:{recipe_id}:2/1"))
            .message(card.clone()),
    );
    bot.dispatch().await;

    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_scale:ask:{recipe_id}"))
            .message(card.clone()),
    );
    bot.dispatch().await;
    bot.update(
        MockCallbackQuery::new()
            .data("cancel_action")
            .message(MockMessageText::new().text("⚖️").build()),
    );
    bot.dispatch().await;
    assert!(action(&services, chat).await?.is_none());
    assert_eq!(card_scale(&pool, card.id).await?.as_deref(), Some("2/1"));
    let buttons = bot
        .get_responses()
        .edited_messages_reply_markup
        .last()
        .and_then(|edit| edit.message.reply_markup().cloned())
        .context("the card's buttons back")?;
    assert!(
        buttons
            .inline_keyboard
            .iter()
            .flatten()
            .any(|button| matches!(
                &button.kind,
                teloxide::types::InlineKeyboardButtonKind::CallbackData(data)
                    if *data == format!("recipe_scale:set:{recipe_id}:1/1")
            )),
        "still doubled: {buttons:?}"
    );
    Ok(())
}

/// From a doubled card, a linked recipe's button opens it doubled too.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_linked_recipe_opens_at_the_same_scale(pool: PgPool) -> Result<()> {
    use just_ingredients::db::{NewIngredient, save_recipe};
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let recipe_id = save_tart(&pool).await?;
    let sauce = save_recipe(
        &pool,
        OWNER,
        Some("fr"),
        "",
        "Sauce",
        &[NewIngredient {
            name: "crème",
            quantity: Some(20.0),
            unit: Some("cl"),
        }],
    )
    .await?;
    sqlx::query("INSERT INTO recipe_links (recipe_a, recipe_b) VALUES ($1, $2)")
        .bind(recipe_id.min(sauce.recipe_id))
        .bind(recipe_id.max(sauce.recipe_id))
        .execute(&pool)
        .await?;
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services.clone());
    let card = open_card(&mut bot, recipe_id).await?;

    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_scale:set:{recipe_id}:2/1"))
            .message(card.clone()),
    );
    bot.dispatch().await;
    let link = button_data(&bot.get_responses(), "recipe_link:").context("the linked button")?;
    assert_eq!(link, format!("recipe_link:{}:2/1", sauce.recipe_id));

    bot.update(MockCallbackQuery::new().data(link).message(card));
    bot.dispatch().await;
    let opened = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the sauce's card")?;
    let text = opened.text().unwrap_or_default().to_string();
    assert!(text.contains("Sauce"), "{text}");
    assert!(text.contains("× 2"), "{text}");
    assert!(text.contains("40 cl"), "{text}");
    assert_eq!(card_scale(&pool, opened.id).await?.as_deref(), Some("2/1"));
    Ok(())
}

/// `/unites` on a doubled card: the card comes back in the chosen view, still doubled.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_doubled_card_stays_doubled_in_another_unit_view(pool: PgPool) -> Result<()> {
    use teloxide_tests::MockCallbackQuery;
    let services = services(&pool, Vec::new()).await?;
    let recipe_id = save_tart(&pool).await?;
    let mut bot = mock_bot(MockMessageText::new().text("/start"), services.clone());
    let card = open_card(&mut bot, recipe_id).await?;
    bot.update(
        MockCallbackQuery::new()
            .data(format!("recipe_scale:set:{recipe_id}:2/1"))
            .message(card),
    );
    bot.dispatch().await;

    bot.update(MockMessageText::new().text("/unites"));
    bot.dispatch().await;
    let weight =
        button_data(&bot.get_responses(), "unit_view:weight:").context("the weight view button")?;
    assert_eq!(weight, format!("unit_view:weight:{recipe_id}"));
    let chooser = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the chooser")?;
    bot.update(MockCallbackQuery::new().data(weight).message(chooser));
    bot.dispatch().await;
    let again = bot
        .get_responses()
        .sent_messages
        .last()
        .cloned()
        .context("the card again")?;
    let text = again.text().unwrap_or_default().to_string();
    assert!(text.contains("× 2"), "{text}");
    assert!(text.contains("500 g"), "{text}");
    assert_eq!(card_scale(&pool, again.id).await?.as_deref(), Some("2/1"));
    Ok(())
}
