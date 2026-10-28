//! Backend calls about this daemon's Workspaces; backend Store calls run
//! in the SDK's process (layerfs-store-service).
mod dispatch;
pub(crate) use dispatch::{admits, dispatch};
pub(crate) use layerfs_store_service::operator::{snapshot, WorkspaceView};

mod workspace;
