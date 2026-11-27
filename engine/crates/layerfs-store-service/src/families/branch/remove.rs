//! `branch.remove`.
use crate::families::args::{project, text, Answer};
use crate::{history::branch_remove, StoreService};
use layerfs_protocol::sdk::SdkValue;

/// `remove({ project, branch })`: hide one Branch by its Store marker.
pub fn remove(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    branch_remove::remove(&service.store, &stack, text(args, "branch")?)
}
