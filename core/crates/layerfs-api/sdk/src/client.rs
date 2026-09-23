//! One configured host authority; the operation itself has only name and path.
use layerfs_api_core::{Error, Project};
use layerfs_bridge::adapters::native::connection::VerifiedPeer;
use layerfs_service::Service;
use std::path::Path;

pub struct Client<'a> {
    service: &'a Service,
    peer: &'a VerifiedPeer,
    store: u32,
}

impl<'a> Client<'a> {
    /// Bind the SDK to one already-authorized host Service and Store.
    pub fn new(service: &'a Service, peer: &'a VerifiedPeer, store: u32) -> Self {
        Self {
            service,
            peer,
            store,
        }
    }

    /// Import one host-visible directory and return its published genesis.
    pub fn init_project(&self, project_name: &str, path: &Path) -> Result<Project, Error> {
        let created = self
            .service
            .init_project(self.peer, self.store, project_name, path)?;
        Ok(Project {
            id: created.stack.stack,
            genesis_layer: created.stack.head_layer,
            root: created.root,
            root_serial: created.root_serial,
        })
    }
}
