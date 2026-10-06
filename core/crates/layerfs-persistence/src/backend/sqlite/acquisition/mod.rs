//! SQLite mechanics of the initial-acquisition working tables.
//!
//! Every function runs inside one caller-owned transaction and reaches rows
//! only through the operation-prefixed keys of the shipped statements.
pub(crate) mod accounting;
pub(crate) mod cleanup;
pub(crate) mod reads;
mod root_windows;
pub(crate) mod statements;
pub(crate) mod writes;
