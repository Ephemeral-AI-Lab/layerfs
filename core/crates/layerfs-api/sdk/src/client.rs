//! One configured host authority; the operation itself has only name and path.
use crate::project::ProjectApi;
use layerfs_api_core::{Error, Project};
use layerfs_bridge::adapters::native::connection::VerifiedPeer;
use layerfs_server::Service;
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
        ProjectApi::new(self.service, self.peer, self.store).init(project_name, path)
    }
}
