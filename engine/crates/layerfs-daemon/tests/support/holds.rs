//! Workspace route selection for mounted FORGET/drain proofs.
use layerfs_daemon::control::Service;
use layerfs_protocol::daemon::WorkspaceToken;
use layerfs_vfs::Route;
pub fn route(service: &Service, token: WorkspaceToken) -> Route {
    service.operation(token).unwrap().workspace().route()
}
