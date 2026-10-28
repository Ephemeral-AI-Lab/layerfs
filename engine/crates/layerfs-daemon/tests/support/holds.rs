//! Workspace route selection for mounted FORGET/drain proofs.
use layerfs_bridge::control::WorkspaceToken;
use layerfs_daemon::control::Service;
use layerfs_workspace::Route;
pub fn route(service: &Service, token: WorkspaceToken) -> Route {
    service.operation(token).unwrap().workspace().route()
}
