//! The MCP door: the user's recipes for AI assistants (Claude Code, Cursor, VS Code…), read only
//! (`specs/tasks/engine-api.md`, §5). Served at `/mcp` behind [`super::guard`], which puts the
//! [`Authenticated`] token in the request; each tool calls the engine for that token's owner.
//!
//! Streamable HTTP without sessions (`legacy_session_mode` off, JSON answers): nothing to lose at
//! a deploy, no stream to keep open through Fly's proxy. Requests must name an allowed `Host`
//! (the app's public name), against DNS rebinding. The tools are listed and called by hand: rmcp's
//! `#[tool]` macro expands to code that may panic, which this project forbids.
//!
//! What the tools return is the engine's values as JSON, plus that JSON as text. They never return
//! the text read from a photo (`recipes.content`), which may be someone else's page.

use std::sync::Arc;

use axum::Router;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, ServerHandler};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use super::guard::{Guard, Limits, guarded};
use crate::engine::recipes::{self, MAX_LISTED};
use crate::engine::tokens::{Authenticated, Realm, SCOPE_RECIPES_READ};
use crate::engine::{EngineError, Lang, Owner};
use crate::unit_view::UnitView;

/// The path of the door.
pub const PATH: &str = "/mcp";

/// Largest request body read, in bytes (a tool call is a few hundred).
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Longest search accepted, in characters.
const MAX_QUERY_CHARS: usize = 200;

/// What the server tells an assistant when it connects.
const INSTRUCTIONS: &str = "The recipes of a JustIngredients user (a Telegram bot that reads \
recipe photos). Recipe and ingredient names are in the user's language, mostly French: search \
with words of that language (\"poireaux\", not \"leeks\"), or list every recipe and match the \
names yourself, then read one with get_recipe. A recipe may have a memo (oven temperature, \
cooking time). Measures can be shown in four unit views. Read only: nothing can be changed here.";

/// The MCP door's routes, behind its guard: the hosts it answers for, and the database.
pub fn router(pool: PgPool, realm: Realm, allowed_hosts: Vec<String>, limits: Limits) -> Router {
    let server_pool = pool.clone();
    let service = StreamableHttpService::new(
        move || {
            Ok(RecipesServer {
                pool: server_pool.clone(),
            })
        },
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            .with_allowed_hosts(allowed_hosts)
            .with_max_request_body_bytes(MAX_BODY_BYTES),
    );
    let guard = Guard::new(pool, realm, "mcp", SCOPE_RECIPES_READ, limits);
    guarded(Router::new().nest_service(PATH, service), guard)
}

/// The hosts the door answers for: the Fly app's public name, or the local ones without it.
pub fn allowed_hosts(app_name: Option<&str>) -> Vec<String> {
    match app_name {
        Some(app) => vec![format!("{app}.fly.dev")],
        None => vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ],
    }
}

/// The MCP server of one request.
#[derive(Debug, Clone)]
struct RecipesServer {
    pool: PgPool,
}

impl ServerHandler for RecipesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "justingredients",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let owner = owner_of(&context)?;
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let (tool, outcome) = match request.name.as_ref() {
            "list_recipes" => ("list_recipes", list(&self.pool, owner).await),
            "search_recipes" => ("search_recipes", search(&self.pool, owner, arguments).await),
            "get_recipe" => ("get_recipe", get(&self.pool, owner, arguments).await),
            "convert_quantity" => (
                "convert_quantity",
                convert(&self.pool, owner, arguments).await,
            ),
            _ => return Err(ErrorData::invalid_params("Unknown tool", None)),
        };
        let result = match outcome {
            Ok(value) => {
                crate::observability::mcp_tool(tool, "ok");
                CallToolResult::structured(value)
            }
            // Said to the assistant, which can try again
            Err(Refusal::Told(text)) => {
                crate::observability::mcp_tool(tool, "refused");
                CallToolResult::error(vec![ContentBlock::text(text)])
            }
            Err(Refusal::Failed(error)) => {
                crate::observability::mcp_tool(tool, "failed");
                tracing::warn!(error = %error, tool, "An MCP tool failed");
                return Err(ErrorData::internal_error("Internal error", None));
            }
        };
        Ok(result.into())
    }
}

/// Why a tool gave no result: something to tell the assistant, or a failure of ours (logged, not
/// told).
enum Refusal {
    Told(String),
    Failed(EngineError),
}

impl From<EngineError> for Refusal {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::NotFound => {
                Self::Told("No recipe of the user has this id: list or search them.".into())
            }
            EngineError::Invalid(_) => {
                Self::Told("The quantity must be a positive number, at most 10000.".into())
            }
            other => Self::Failed(other),
        }
    }
}

/// The token's owner, put in the request by the guard. A request without one never gets here.
fn owner_of(context: &RequestContext<RoleServer>) -> Result<Owner, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Authenticated>())
        .map(|authenticated| authenticated.owner)
        .ok_or_else(|| {
            tracing::warn!("An MCP request reached a tool without its token");
            ErrorData::internal_error("Not authenticated", None)
        })
}

fn arguments<T: serde::de::DeserializeOwned>(arguments: Value) -> Result<T, Refusal> {
    serde_json::from_value(arguments)
        .map_err(|error| Refusal::Told(format!("Invalid arguments: {error}")))
}

/// A unit view by its name; the owner's own when not given.
fn unit_view(name: Option<&str>) -> Result<Option<UnitView>, Refusal> {
    name.map(|name| {
        UnitView::parse(name)
            .ok_or_else(|| Refusal::Told("units must be one of recipe, weight, french, us.".into()))
    })
    .transpose()
}

/// A language by its code; the owner's own when not given.
async fn language(pool: &PgPool, owner: Owner, code: Option<&str>) -> Result<Lang, Refusal> {
    match code {
        Some("fr") => Ok(Lang::Fr),
        Some("en") => Ok(Lang::En),
        Some(_) => Err(Refusal::Told("lang must be fr or en.".into())),
        None => Ok(crate::engine::language_of(pool, owner).await?),
    }
}

async fn list(pool: &PgPool, owner: Owner) -> Result<Value, Refusal> {
    let listed = recipes::list(pool, owner).await?;
    Ok(json!({
        "count": listed.len(),
        "truncated": listed.len() >= MAX_LISTED,
        "recipes": listed,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArguments {
    query: String,
}

async fn search(pool: &PgPool, owner: Owner, arguments_value: Value) -> Result<Value, Refusal> {
    let SearchArguments { query } = arguments(arguments_value)?;
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return Err(Refusal::Told(format!(
            "query must be 1 to {MAX_QUERY_CHARS} characters."
        )));
    }
    let found = recipes::search(pool, owner, query).await?;
    Ok(json!({ "query": query, "recipes": found }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetRecipeArguments {
    id: i64,
    #[serde(default)]
    units: Option<String>,
    #[serde(default)]
    lang: Option<String>,
}

async fn get(pool: &PgPool, owner: Owner, arguments_value: Value) -> Result<Value, Refusal> {
    let GetRecipeArguments { id, units, lang } = arguments(arguments_value)?;
    let view = unit_view(units.as_deref())?;
    let lang = language(pool, owner, lang.as_deref()).await?;
    let recipe = recipes::recipe(pool, owner, id, view, lang).await?;
    serde_json::to_value(recipe)
        .map_err(|error| Refusal::Failed(EngineError::Internal(error.into())))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConvertArguments {
    quantity: f64,
    unit: String,
    ingredient: String,
    to: String,
    #[serde(default)]
    lang: Option<String>,
}

async fn convert(pool: &PgPool, owner: Owner, arguments_value: Value) -> Result<Value, Refusal> {
    let ConvertArguments {
        quantity,
        unit,
        ingredient,
        to,
        lang,
    } = arguments(arguments_value)?;
    let Some(view) = unit_view(Some(&to))? else {
        return Err(Refusal::Told(
            "to must be one of recipe, weight, french, us.".into(),
        ));
    };
    let lang = language(pool, owner, lang.as_deref()).await?;
    let shown = recipes::convert(quantity, &unit, &ingredient, view, lang)?;
    Ok(json!({ "text": shown.text, "converted": shown.converted, "view": view }))
}

/// An object schema from its properties and required names.
fn schema(properties: Value, required: &[&str]) -> Arc<JsonObject> {
    let mut object = JsonObject::new();
    object.insert("type".into(), json!("object"));
    object.insert("properties".into(), properties);
    object.insert("required".into(), json!(required));
    object.insert("additionalProperties".into(), json!(false));
    Arc::new(object)
}

/// A read-only tool of the user's own data: never changes anything, nothing outside it.
fn read_only(name: &'static str, description: &'static str, input: Arc<JsonObject>) -> Tool {
    Tool::new(name, description, input).with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn tools() -> Vec<Tool> {
    let units = json!({
        "type": "string",
        "enum": ["recipe", "weight", "french", "us"],
        "description": "How measures are shown: recipe (as read), weight (grams, for a kitchen \
                        scale), french (g, cl, spoons), us (cups, tablespoons, ounces). Default: \
                        the user's own choice."
    });
    let lang = json!({
        "type": "string",
        "enum": ["fr", "en"],
        "description": "Language of unit names (\"cuil. à café\" or \"tsp\"). Default: the user's."
    });
    vec![
        read_only(
            "list_recipes",
            "List every recipe of the user (id, name, memo), sorted by name. Names are in the \
             user's language, mostly French: use this to find a recipe by meaning (leeks → \
             \"Tarte aux poireaux\"), then get_recipe.",
            schema(json!({}), &[]),
        ),
        read_only(
            "search_recipes",
            "Search the user's recipes by words of their language (mostly French: \"poireaux\", \
             not \"leeks\"), in recipe and ingredient names; best first. No translation.",
            schema(
                json!({ "query": {
                    "type": "string", "minLength": 1, "maxLength": MAX_QUERY_CHARS,
                    "description": "Words to find, in the recipes' language"
                }}),
                &["query"],
            ),
        ),
        read_only(
            "get_recipe",
            "One recipe of the user: name, memo, ingredients (each with its measure as saved and \
             as shown in the unit view), and the recipes linked to it (other parts of the same \
             page).",
            schema(
                json!({
                    "id": { "type": "integer", "description": "From list_recipes or search_recipes" },
                    "units": units,
                    "lang": lang,
                }),
                &["id"],
            ),
        ),
        read_only(
            "convert_quantity",
            "Convert one measure of an ingredient to a unit view, as the user's recipe cards \
             would show it (2 cups of flour in grams: \"240 g\"). The result is text, rounded \
             for the kitchen; converted is false when the view keeps the measure as it is.",
            schema(
                json!({
                    "quantity": { "type": "number", "exclusiveMinimum": 0, "maximum": 10000 },
                    "unit": { "type": "string", "description": "g, kg, ml, l, cup, tbsp, tsp, oz, lb, cl, \"cuil. à café\"…" },
                    "ingredient": { "type": "string", "description": "Its name, for densities (farine, sugar…)" },
                    "to": units,
                    "lang": lang,
                }),
                &["quantity", "unit", "ingredient", "to"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_read_only_tools_with_closed_schemas() {
        let tools = tools();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            names,
            [
                "list_recipes",
                "search_recipes",
                "get_recipe",
                "convert_quantity"
            ]
        );
        for tool in &tools {
            let annotations = tool.annotations.as_ref();
            assert_eq!(
                annotations.and_then(|annotations| annotations.read_only_hint),
                Some(true),
                "{}",
                tool.name
            );
            assert_eq!(
                tool.input_schema.get("additionalProperties"),
                Some(&json!(false)),
                "{}",
                tool.name
            );
        }
    }

    #[test]
    fn the_door_answers_for_the_apps_public_name_only() {
        assert_eq!(
            allowed_hosts(Some("just-ingredients")),
            ["just-ingredients.fly.dev"]
        );
        assert!(allowed_hosts(None).contains(&"127.0.0.1".to_string()));
    }

    #[test]
    fn arguments_are_checked() {
        assert!(matches!(
            arguments::<SearchArguments>(json!({ "query": "tarte", "extra": 1 })),
            Err(Refusal::Told(_))
        ));
        assert!(matches!(
            arguments::<GetRecipeArguments>(json!({ "id": "seven" })),
            Err(Refusal::Told(_))
        ));
        assert!(matches!(unit_view(Some("metric")), Err(Refusal::Told(_))));
        assert!(matches!(
            unit_view(Some("weight")),
            Ok(Some(UnitView::Weight))
        ));
        assert!(matches!(unit_view(None), Ok(None)));
    }
}
