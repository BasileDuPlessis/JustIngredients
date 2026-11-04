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
    use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
    use just_ingredients::MeasurementMatch;
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

/// ✅ on a review with the caption's name saves the recipe under that name and ends the action.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn confirm_with_a_caption_name_saves_the_recipe(pool: PgPool) -> Result<()> {
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
    assert!(action(&services, chat).await?.is_none(), "the review ended");
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
    assert!(action(&services, chat).await?.is_none());
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
    use just_ingredients::db::{get_recipe_ingredients, save_recipe, NewIngredient};
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
    use just_ingredients::dialogue::{RecipeDialogue, RecipeDialogueState};
    use just_ingredients::MeasurementMatch;
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
    use just_ingredients::db::{get_recipe_ingredients, save_recipe, NewIngredient};
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
    assert!(bot
        .get_responses()
        .sent_messages
        .iter()
        .any(|message| message.text() == Some(too_long.as_str())));
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
    use just_ingredients::db::{get_recipe_ingredients, save_recipe, NewIngredient};
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
