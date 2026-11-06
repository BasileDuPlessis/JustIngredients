//! The doors other than Telegram (`specs/tasks/engine-api.md`): an MCP server first. Each door
//! is served by the public HTTP server (`observability::server::start_with_doors`) behind the
//! [`guard`]: a valid personal token, and limits that keep the bot's own work safe.

pub mod guard;
pub mod mcp;

/// The public host of the Fly app (`just-ingredients.fly.dev`): the only `Host` the doors answer
/// for, and the address `/cle` gives. `None` without an app name (local runs).
pub fn public_host(app_name: Option<&str>) -> Option<String> {
    app_name.map(|app| format!("{app}.fly.dev"))
}
