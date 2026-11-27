//! Workspace route selection for mounted FORGET/drain proofs.
use layerfs_protocol::provision::install::WorkspaceToken;
use layerfs_daemon::control::Service;
use layerfs_vfs::Route;
pub fn route(service: &Service, token: WorkspaceToken) -> Route {
    service.operation(token).unwrap().workspace().route()
}
