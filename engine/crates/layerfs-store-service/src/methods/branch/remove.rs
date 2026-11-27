//! `branch.remove`.
use crate::methods::args::{project, text, Answer};
use crate::{history::removal, StoreService};
use layerfs_protocol::sdk::SdkValue;

/// `remove({ project, branch })`: hide one Branch by its Store marker.
pub fn remove(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    removal::remove(&service.store, &stack, text(args, "branch")?)
}
