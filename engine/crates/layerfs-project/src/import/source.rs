//! Host source observations; symbolic links are stored without traversal.
use crate::{ProjectError, SourceRefusal};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use std::{fs::{self, Metadata}, path::Path};
use super::backing::face;

pub(super) fn read_link(path: &Path, scanned: &Metadata) -> Result<Vec<u8>, ProjectError> {
    // read_link never follows the target, including dangling, external or cyclic
    // links. A replaced/changed source is refused without a second read/retry.
    let target = fs::read_link(path)?;
    let after = fs::symlink_metadata(path)?;
    let host = super::host_id::of_path(path);
    let (a, s) = (face(&after, host), face(scanned, host));
    if !after.file_type().is_symlink()
        || a.dev != s.dev
        || a.ino != s.ino
        || after.len() != scanned.len()
        || a.mode != s.mode
        || a.mtime != s.mtime
        || a.mtime_nsec != s.mtime_nsec
        || a.ctime != s.ctime
        || a.ctime_nsec != s.ctime_nsec
    {
        return Err(ProjectError::refused(SourceRefusal::Changed, path));
    }
    // Keep the canonical target bound/grammar: no normalization or UTF-8 cast.
    let target = SymlinkTarget::new(super::bytes::as_bytes(target.as_os_str()))
        .map_err(ProjectError::Content)?;
    Ok(target.as_bytes().to_vec())
}

/// Refuses a backing whose files are the source or lie inside it.
///
/// The placement is resolved through its links first, then each ancestor's
/// host identity is compared with the source's, so a symlinked or otherwise
/// aliased spelling of a directory inside the source is recognised. Nothing is
/// begun or changed: a backing inside the source would both alter the source
/// while it is read and be acquired as part of it.
pub(super) fn outside_source(
    source: &Path,
    scanned: &Metadata,
    placement: &Path,
) -> Result<(), ProjectError> {
    let source_host = super::host_id::of_path(source);
    let mut ancestor = fs::canonicalize(placement)?;
    loop {
        let metadata = fs::symlink_metadata(&ancestor)?;
        let (m, s) = (
            face(&metadata, super::host_id::of_path(&ancestor)),
            face(scanned, source_host),
        );
        if m.dev == s.dev && m.ino == s.ino {
            return Err(ProjectError::BackingInsideSource);
        }
        if !ancestor.pop() {
            return Ok(());
        }
    }
}
