//! The key-indexed metadata tree: format, ordered update and ordered builder.
pub mod build;
pub mod cursor;
pub mod delete;
pub mod format;
pub mod update;
pub use cursor::KeyCursor;
pub(crate) use update::{edges, edges_raw, stored_key_limit};
