//! `fs.read_bytes`.
use crate::base::args::{count, path, snapshot, Answer};
use layerfs_bridge::control::SdkValue;
use layerfs_store_service::{
    read::{reads, window},
    StoreService,
};

/// `read_bytes(path, { branch, commit?, offset?, length? })`: one bounded
/// window of a file's raw bytes at a Branch, Commit or Layer.
pub fn read_bytes(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = path(args, "path")?;
    reads::read_bytes(
        &reader,
        &snap,
        &path,
        count(args, "offset", 0),
        count(args, "length", window::BYTES as u64),
    )
}
