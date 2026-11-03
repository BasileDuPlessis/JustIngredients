//! Database access (Postgres): one module per table group; everything is re-exported here, so
//! callers keep writing `crate::db::…`.

mod chat_messages;
mod ingredients;
pub mod migrations;
mod recipes;
mod schema;
mod usage;
mod users;

pub use crate::observability;
pub use chat_messages::*;
pub use ingredients::*;
pub use recipes::*;
pub use schema::*;
pub use usage::*;
pub use users::*;
