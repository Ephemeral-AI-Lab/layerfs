//! Small formats other crates read: shallow JSON, PostgreSQL URL hosts and
//! the platforms LayerFS removed.
#[cfg(unix)]
pub mod json;
#[cfg(unix)]
pub(crate) mod postgres_url;
pub mod removed_platforms;
