//! Authorized host-direct Project Init.
use layerfs_api_core::{Error, Project};
use layerfs_bridge::adapters::native::connection::VerifiedPeer;
use layerfs_server::Service;
use std::path::Path;

pub struct ProjectApi<'a> {
    service: &'a Service,
    peer: &'a VerifiedPeer,
    store: u32,
}
impl<'a> ProjectApi<'a> {
    pub fn new(service: &'a Service, peer: &'a VerifiedPeer, store: u32) -> Self {
        Self {
            service,
            peer,
            store,
        }
    }
    pub fn init(&self, name: &str, path: &Path) -> Result<Project, Error> {
        let created = self
            .service
            .init_project(self.peer, self.store, name, path)?;
        Ok(Project {
            id: created.stack.stack,
            genesis_layer: created.stack.head_layer,
            root: created.root,
            root_serial: created.root_serial,
        })
    }
}
