//! `fs.write`.
use crate::methods::args::{commit, single, Answer};
use crate::StoreService;
use layerfs_bridge::control::SdkValue;

/// `write(path, content, { branch, mode? })`: one Commit writing a file; `mode`
/// sets its permission bits, else a new file is 0o644 and an existing one keeps
/// its mode.
pub fn write(service: &StoreService, args: &SdkValue) -> Answer {
    commit(service, args, &[single(args, "write")])
}
