//! One SQLite connection: profile, transactions, statements, rows and counters.
pub(crate) mod connection;
pub(crate) mod prepared;
pub(crate) mod profile;
pub(crate) mod query;
pub(crate) mod rows;
pub(crate) mod statement_work;
pub(crate) mod thread_work;
pub(crate) mod transaction;
