//! Backend calls about this daemon's Workspaces; backend Store calls run
//! in the SDK's process (layerfs-store-service).
mod dispatch;
pub(crate) use dispatch::{admits, dispatch};
pub(crate) use layerfs_store_service::operator::{snapshot, workspaces, WorkspaceView};

mod workspace;

/// The installed Store fixture the operator unit test binds without a kernel.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/support/installed_store.rs"]
mod installed_store;
