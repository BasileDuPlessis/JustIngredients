//! The MCP door (`specs/tasks/engine-api.md`, §5) over a real socket and a throwaway database:
//! raw JSON-RPC requests as an assistant sends them, two users.
//!
//! They need Postgres: `cargo test --test mcp_tests -- --include-ignored` with `DATABASE_URL`
//! set to a throwaway database.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use just_ingredients::db::{NewIngredient, RecipePart, init_database_schema, save_recipe_parts};
use just_ingredients::doors::guard::Limits;
use just_ingredients::doors::mcp;
use just_ingredients::engine::Owner;
use just_ingredients::engine::tokens::{self, Realm};
use serde_json::{Value, json};
use sqlx::PgPool;

const ALICE: i64 = 601;
const BOB: i64 = 602;

/// The door on a free port, answering for 127.0.0.1.
async fn serve(pool: &PgPool) -> Result<SocketAddr> {
    let app = mcp::router(
        pool.clone(),
        Realm::Production,
        vec!["127.0.0.1".to_string()],
        Limits::default(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok(address)
}

/// Alice's tart (in two parts, with a memo) and Bob's soup; Alice's token.
async fn two_users(pool: &PgPool) -> Result<(String, i64, i64)> {
    init_database_schema(pool).await?;
    let tart = save_recipe_parts(
        pool,
        ALICE,
        Some("fr"),
        "Tarte aux poireaux",
        &[
            RecipePart {
                name: "Tarte aux poireaux".to_string(),
                ingredients: vec![
                    NewIngredient {
                        name: "farine",
                        quantity: Some(2.0),
                        unit: Some("cup"),
                    },
                    NewIngredient {
                        name: "levure chimique",
                        quantity: Some(1.0),
                        unit: Some("tsp"),
                    },
                ],
            },
            RecipePart {
                name: "sauce".to_string(),
                ingredients: Vec::new(),
            },
        ],
    )
    .await?
    .first()
    .context("the tart")?
    .recipe_id;
    just_ingredients::db::set_recipe_note(pool, ALICE, tart, Some("Four 180 °C")).await?;
    let soup =
        just_ingredients::db::save_recipe(pool, BOB, Some("fr"), "", "Soupe aux poireaux", &[])
            .await?
            .recipe_id;
    let token = tokens::create(pool, Owner::telegram(ALICE), Realm::Production).await?;
    Ok((token.secret, tart, soup))
}

/// A JSON-RPC message posted as an assistant does; the HTTP status and the JSON answer (from a
/// JSON body or the first event of a stream), `Null` without a body.
async fn post(
    address: SocketAddr,
    token: Option<&str>,
    host: Option<&str>,
    message: &Value,
) -> Result<(u16, Value)> {
    let mut request = reqwest::Client::builder()
        .no_proxy()
        .build()?
        .post(format!("http://{address}/mcp"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .json(message);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(host) = host {
        request = request.header("Host", host);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    let body = response.text().await?;
    let json = body
        .lines()
        .find_map(|line| line.strip_prefix("data:"))
        .unwrap_or(&body)
        .trim()
        .to_string();
    let value = if json.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&json).unwrap_or(Value::String(body))
    };
    Ok((status, value))
}

fn call(id: u64, tool: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    })
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn an_assistant_reads_its_users_recipes(pool: PgPool) -> Result<()> {
    let (token, tart, soup) = two_users(&pool).await?;
    let address = serve(&pool).await?;
    let token = Some(token.as_str());

    // The handshake
    let (status, answer) = post(
        address,
        token,
        None,
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        }),
    )
    .await?;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["result"]["serverInfo"]["name"], "justingredients");
    assert!(
        answer["result"]["capabilities"]["tools"].is_object(),
        "{answer}"
    );
    assert!(
        answer["result"]["instructions"]
            .as_str()
            .is_some_and(|text| text.contains("French"))
    );
    let (status, _) = post(
        address,
        token,
        None,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await?;
    assert!((200..300).contains(&status), "{status}");

    // The tools, all read only
    let (_, answer) = post(
        address,
        token,
        None,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await?;
    let tools = answer["result"]["tools"].as_array().context("tools")?;
    assert_eq!(tools.len(), 4, "{answer}");
    assert!(
        tools
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == true)
    );

    // Alice's recipes only, with the memo, never the photo's text
    let (_, answer) = post(address, token, None, &call(3, "list_recipes", json!({}))).await?;
    let listed = &answer["result"]["structuredContent"];
    let names: Vec<&str> = listed["recipes"]
        .as_array()
        .context("recipes")?
        .iter()
        .filter_map(|recipe| recipe["name"].as_str())
        .collect();
    assert_eq!(names, ["sauce", "Tarte aux poireaux"], "{answer}");
    assert_eq!(listed["total"], 2);
    assert_eq!(listed["truncated"], false);
    assert_eq!(listed["recipes"][1]["memo"], "Four 180 °C");
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("Tarte aux poireaux")),
        "the JSON also as text"
    );

    let (_, answer) = post(
        address,
        token,
        None,
        &call(4, "search_recipes", json!({ "query": "poireaux" })),
    )
    .await?;
    let found = &answer["result"]["structuredContent"]["recipes"];
    assert_eq!(
        found.as_array().map(Vec::len),
        Some(1),
        "not Bob's soup: {answer}"
    );
    assert_eq!(found[0]["id"], tart);

    // One recipe, in the weight view, French unit names
    let (_, answer) = post(
        address,
        token,
        None,
        &call(
            5,
            "get_recipe",
            json!({ "id": tart, "units": "weight", "lang": "fr" }),
        ),
    )
    .await?;
    let recipe = &answer["result"]["structuredContent"];
    assert_eq!(recipe["name"], "Tarte aux poireaux", "{answer}");
    assert_eq!(recipe["memo"], "Four 180 °C");
    assert_eq!(recipe["unit_view"], "weight");
    assert_eq!(recipe["ingredients"][0]["shown"]["text"], "240 g");
    assert_eq!(recipe["ingredients"][0]["stored"]["unit"], "cup");
    assert_eq!(recipe["linked"][0]["name"], "sauce");
    assert!(recipe.get("content").is_none(), "never the photo's text");
    let (_, answer) = post(
        address,
        token,
        None,
        &call(
            6,
            "get_recipe",
            json!({ "id": tart, "units": "recipe", "lang": "fr" }),
        ),
    )
    .await?;
    assert_eq!(
        answer["result"]["structuredContent"]["ingredients"][1]["shown"]["text"],
        "1 cuil. à café"
    );

    // Bob's recipe is not found, as a missing one: told to the assistant
    for id in [soup, tart + 1000] {
        let (_, answer) = post(
            address,
            token,
            None,
            &call(7, "get_recipe", json!({ "id": id })),
        )
        .await?;
        assert_eq!(answer["result"]["isError"], true, "{answer}");
        assert!(answer["result"]["structuredContent"].is_null(), "{answer}");
    }

    let (_, answer) = post(
        address,
        token,
        None,
        &call(
            8,
            "convert_quantity",
            json!({ "quantity": 2, "unit": "cups", "ingredient": "farine", "to": "weight" }),
        ),
    )
    .await?;
    assert_eq!(
        answer["result"]["structuredContent"]["text"], "240 g",
        "{answer}"
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn bad_calls_are_answered_not_crashed(pool: PgPool) -> Result<()> {
    let (token, _, _) = two_users(&pool).await?;
    let address = serve(&pool).await?;
    let token = Some(token.as_str());

    for (arguments, tool) in [
        (json!({ "id": "seven" }), "get_recipe"),
        (json!({ "id": 1, "units": "metric" }), "get_recipe"),
        (json!({ "query": "" }), "search_recipes"),
        (json!({ "query": "x".repeat(201) }), "search_recipes"),
        (json!({ "query": "tarte", "extra": true }), "search_recipes"),
        (
            json!({ "quantity": -1, "unit": "g", "ingredient": "sel", "to": "us" }),
            "convert_quantity",
        ),
        (
            json!({ "quantity": 1, "unit": "g", "ingredient": "sel", "to": "metric" }),
            "convert_quantity",
        ),
    ] {
        let (status, answer) =
            post(address, token, None, &call(1, tool, arguments.clone())).await?;
        assert_eq!(status, 200);
        assert_eq!(
            answer["result"]["isError"], true,
            "{tool} {arguments}: {answer}"
        );
    }
    let (_, answer) = post(
        address,
        token,
        None,
        &call(3, "list_recipes", json!({ "all": true })),
    )
    .await?;
    assert_eq!(
        answer["result"]["isError"], true,
        "list_recipes takes no argument: {answer}"
    );
    let (_, answer) = post(address, token, None, &call(2, "delete_recipe", json!({}))).await?;
    assert_eq!(answer["error"]["code"], -32602, "an unknown tool: {answer}");
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn the_door_needs_a_token_and_its_own_host(pool: PgPool) -> Result<()> {
    let (token, _, _) = two_users(&pool).await?;
    let address = serve(&pool).await?;
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });

    let (status, _) = post(address, None, None, &list).await?;
    assert_eq!(status, 401, "no token");
    let (status, _) = post(address, Some("ji_nope"), None, &list).await?;
    assert_eq!(status, 401, "not a token");
    let (status, _) = post(address, Some(&token), Some("evil.example"), &list).await?;
    assert_eq!(status, 403, "another host (DNS rebinding)");
    let (status, _) = post(address, Some(&token), None, &list).await?;
    assert_eq!(status, 200);
    Ok(())
}

/// The names of the tools a key is shown.
async fn tool_names(address: SocketAddr, token: &str) -> Result<Vec<String>> {
    let (_, answer) = post(
        address,
        Some(token),
        None,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
    )
    .await?;
    Ok(answer["result"]["tools"]
        .as_array()
        .context("tools")?
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect())
}

/// A read-only key cannot change anything; a read-write key renames, writes memos and creates
/// recipes, with an event from the MCP door for each; nothing deletes.
#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn a_read_write_key_changes_recipes_a_read_only_one_cannot(pool: PgPool) -> Result<()> {
    use just_ingredients::engine::tokens::Access;
    let (read_only, tart, soup) = two_users(&pool).await?;
    let read_write = tokens::create_with(
        &pool,
        Owner::telegram(ALICE),
        Realm::Production,
        Access::ReadWrite,
    )
    .await?
    .secret;
    let address = serve(&pool).await?;

    // What each key is shown
    assert_eq!(tool_names(address, &read_only).await?.len(), 4);
    let names = tool_names(address, &read_write).await?;
    assert_eq!(names.len(), 7, "{names:?}");
    assert!(!names.iter().any(|name| name.contains("delete")));

    // A read-only key is refused, nothing changes
    let (_, answer) = post(
        address,
        Some(&read_only),
        None,
        &call(1, "rename_recipe", json!({ "id": tart, "name": "Volée" })),
    )
    .await?;
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    let name = |pool: PgPool, id: i64| async move {
        Ok::<_, anyhow::Error>(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT recipe_name FROM recipes WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await?,
        )
    };
    assert_eq!(
        name(pool.clone(), tart).await?.as_deref(),
        Some("Tarte aux poireaux")
    );

    // A read-write key renames (names stay unique), writes and clears a memo
    let write = |id: u64, tool: &str, arguments: Value| {
        let token = read_write.clone();
        let message = call(id, tool, arguments);
        async move { post(address, Some(&token), None, &message).await }
    };
    let (_, answer) = write(2, "rename_recipe", json!({ "id": tart, "name": "sauce" })).await?;
    assert_eq!(
        answer["result"]["structuredContent"]["name"], "sauce (2)",
        "{answer}"
    );
    let (_, answer) = write(
        3,
        "set_memo",
        json!({ "id": tart, "memo": " Four\n200 °C " }),
    )
    .await?;
    assert_eq!(answer["result"]["structuredContent"]["memo"], "Four 200 °C");
    let (_, answer) = write(4, "set_memo", json!({ "id": tart, "memo": null })).await?;
    assert!(
        answer["result"]["structuredContent"]["memo"].is_null(),
        "{answer}"
    );

    // Bob's recipe is not found, as for reading
    let (_, answer) = write(5, "rename_recipe", json!({ "id": soup, "name": "Mine" })).await?;
    assert_eq!(answer["result"]["isError"], true);
    assert_eq!(
        name(pool.clone(), soup).await?.as_deref(),
        Some("Soupe aux poireaux")
    );

    // Creating from text; a line that cannot be read is told by its number, nothing created
    let (_, answer) = write(
        6,
        "create_recipe",
        json!({ "name": "Crêpes", "ingredients": ["250 g de farine", "3 oeufs", "50 cl de lait"] }),
    )
    .await?;
    let created = answer["result"]["structuredContent"]["id"]
        .as_i64()
        .context("the new recipe")?;
    let units: Vec<(Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT quantity::float8, unit FROM ingredients WHERE recipe_id = $1 ORDER BY id",
    )
    .bind(created)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        units,
        [
            (Some(250.0), Some("g".to_string())),
            (Some(3.0), None),
            (Some(500.0), Some("ml".to_string())),
        ],
        "read as typed lines are"
    );
    let count = |pool: PgPool| async move {
        Ok::<_, anyhow::Error>(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM recipes")
                .fetch_one(&pool)
                .await?,
        )
    };
    let before = count(pool.clone()).await?;
    let (_, answer) = write(
        7,
        "create_recipe",
        json!({ "name": "Gâteau", "ingredients": ["200 g de sucre", "", "0 g de rien"] }),
    )
    .await?;
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("line 3")),
        "the line's place in what was given, blank lines counted: {answer}"
    );
    let too_many: Vec<String> = (0..101).map(|i| format!("{i} g de sucre")).collect();
    let (_, answer) = write(
        8,
        "create_recipe",
        json!({ "name": "Long", "ingredients": too_many }),
    )
    .await?;
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("At most 100")),
        "{answer}"
    );
    assert_eq!(count(pool.clone()).await?, before, "nothing created");

    // Each change has its event from the MCP door, with the key
    let origins: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT kind, origin, token_id FROM events WHERE origin = 'mcp' ORDER BY id",
    )
    .fetch_all(&pool)
    .await?;
    let kinds: Vec<&str> = origins.iter().map(|(kind, _, _)| kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "recipe.renamed",
            "recipe.memo_changed",
            "recipe.memo_changed",
            "recipe.created"
        ]
    );
    assert!(origins.iter().all(|(_, _, token)| token.is_some()));
    Ok(())
}
