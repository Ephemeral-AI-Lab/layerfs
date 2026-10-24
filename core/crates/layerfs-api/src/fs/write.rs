//! `fs.write`.
use crate::base::args::{commit, single, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::StoreService;

/// `write(path, content, { branch, mode? })`: one Commit writing a file; `mode`
/// sets its permission bits, else a new file is 0o644 and an existing one keeps
/// its mode.
pub fn write(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "write")])
}
