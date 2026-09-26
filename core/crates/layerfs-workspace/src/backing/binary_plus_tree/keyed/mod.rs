//! The key-indexed metadata tree: format, ordered update and ordered builder.
pub mod build;
pub mod format;
pub mod update;
pub(crate) use update::{edges, edges_raw, stored_key_limit};
