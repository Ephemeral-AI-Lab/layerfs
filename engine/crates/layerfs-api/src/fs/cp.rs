//! `fs.cp`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `cp(from, to, { branch })`: one Commit copying a path, recursively.
pub fn cp(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "cp")])
}
