//! `fs.rm`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `rm(path, { branch })`: one Commit removing a path, recursively.
pub fn rm(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "rm")])
}
