//! `fs.mkdir`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `mkdir(path, { branch, mode? })`: one Commit creating a directory and its
/// missing parents; `mode` applies to `path` when it is created.
pub fn mkdir(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mkdir")])
}
