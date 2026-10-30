//! SQLite-only connection, transaction, query, schema and physical mechanics,
//! grouped by role (#506 A1).
pub(crate) mod acquisition;
pub(crate) mod heads;
pub(crate) mod lane;
pub(crate) mod lifecycle;
mod mechanics;
pub(crate) mod metadata;
pub(crate) mod objects;
pub(crate) mod schema;
pub(crate) mod session;
pub(crate) mod snapshot;
pub(crate) mod storage;
