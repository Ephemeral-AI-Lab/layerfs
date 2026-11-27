//! PostgreSQL Store engine (#506): connection with TLS, settings, schema,
//! lane, objects, metadata and history statements, and Init's acquisition.
pub(crate) mod acquisition;
pub(crate) mod compact;
pub(crate) mod diagnostic;
pub(crate) mod history;
#[allow(clippy::module_inception)]
pub(crate) mod lane;
mod mechanics;
pub(crate) mod merge_records;
pub(crate) mod metadata;
pub(crate) mod objects;
pub(crate) mod schema;
pub(crate) mod server;
pub(crate) mod session;
pub(crate) mod upgrade;
