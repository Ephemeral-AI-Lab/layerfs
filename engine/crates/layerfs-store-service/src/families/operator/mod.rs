//! Operator Store calls: Store and file statistics and the read model.
mod dispatch;
pub mod files;
pub mod list_workspaces;
pub mod storage;
pub use dispatch::dispatch;
pub use list_workspaces::{snapshot, workspaces, WorkspaceView};
