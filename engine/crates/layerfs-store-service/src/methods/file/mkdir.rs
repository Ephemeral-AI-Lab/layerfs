//! `fs.mkdir`.
use crate::methods::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

/// `mkdir(path, { branch, mode? })`: one Commit creating a directory and its
/// missing parents; `mode` applies to `path` when it is created.
pub fn mkdir(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "mkdir")])
}
