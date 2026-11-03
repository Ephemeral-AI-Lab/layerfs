//! `fs.cp`.
use crate::methods::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

/// `cp(from, to, { branch })`: one Commit copying a path, recursively.
pub fn cp(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "cp")])
}
