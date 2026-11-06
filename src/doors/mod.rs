//! The doors other than Telegram (`specs/tasks/engine-api.md`): an MCP server first. Each door
//! is served by the public HTTP server (`observability::server::start_with_doors`) behind the
//! [`guard`]: a valid personal token, and limits that keep the bot's own work safe.

pub mod guard;
