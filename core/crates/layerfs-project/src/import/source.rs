//! Native source observations; symbolic links are stored without traversal.
use crate::ProjectError;
use layerfs_content::filesystem::symlink::SymlinkTarget;
use std::{
    fs::{self, Metadata},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};

pub(super) fn read_link(path: &Path, scanned: &Metadata) -> Result<Vec<u8>, ProjectError> {
    // read_link never follows the target, including dangling, external or cyclic
    // links. A replaced/changed source is refused without a second read/retry.
    let target = fs::read_link(path)?;
    let after = fs::symlink_metadata(path)?;
    if !after.file_type().is_symlink()
        || after.dev() != scanned.dev()
        || after.ino() != scanned.ino()
        || after.len() != scanned.len()
        || after.mode() != scanned.mode()
        || after.mtime() != scanned.mtime()
        || after.mtime_nsec() != scanned.mtime_nsec()
        || after.ctime() != scanned.ctime()
        || after.ctime_nsec() != scanned.ctime_nsec()
    {
        return Err(ProjectError::InvalidInput);
    }
    // Keep the canonical target bound/grammar: no normalization or UTF-8 cast.
    let target = SymlinkTarget::new(target.as_os_str().as_bytes().to_vec())
        .map_err(ProjectError::Content)?;
    Ok(target.as_bytes().to_vec())
}

/// Refuses a backing whose files are the source or lie inside it.
///
/// The placement is resolved through its links first, then each ancestor's
/// native identity is compared with the source's, so a symlinked or otherwise
/// aliased spelling of a directory inside the source is recognised. Nothing is
/// begun or changed: a backing inside the source would both alter the source
/// while it is read and be acquired as part of it.
pub(super) fn outside_source(source: &Metadata, placement: &Path) -> Result<(), ProjectError> {
    let mut ancestor = fs::canonicalize(placement)?;
    loop {
        let metadata = fs::symlink_metadata(&ancestor)?;
        if metadata.dev() == source.dev() && metadata.ino() == source.ino() {
            return Err(ProjectError::BackingInsideSource);
        }
        if !ancestor.pop() {
            return Ok(());
        }
    }
}
