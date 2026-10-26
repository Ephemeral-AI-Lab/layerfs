//! Operator Store calls: Store and file statistics and the read model.
pub mod args;
mod dispatch;
pub mod files;
pub mod inspect;
pub mod store;
pub use dispatch::dispatch;
pub use inspect::{snapshot, WorkspaceView};
