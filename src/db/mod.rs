//! Database access (Postgres): one module per table group; everything is re-exported here, so
//! callers keep writing `crate::db::…`.

mod api_tokens;
mod chat_messages;
mod events;
mod ingredients;
pub mod migrations;
mod recipes;
mod schema;
mod usage;
mod users;

pub use crate::observability;
pub use api_tokens::*;
pub use chat_messages::*;
pub use events::*;
pub use ingredients::*;
pub use recipes::*;
pub use schema::*;
pub use usage::*;
pub use users::*;
