//! Host source observations; symbolic links are stored without traversal.
use super::host::{os_to_bytes, HostStat};
use crate::{ProjectError, SourceRefusal};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use std::{
    fs::{self, Metadata},
    path::Path,
};

pub(super) fn read_link(path: &Path, scanned: &Metadata) -> Result<Vec<u8>, ProjectError> {
    // read_link never follows the target, including dangling, external or cyclic
    // links. A replaced/changed source is refused without a second read/retry.
    let target = fs::read_link(path)?;
    let after = fs::symlink_metadata(path)?;
    if !after.file_type().is_symlink()
        || HostStat::of(path, &after)? != HostStat::of(path, scanned)?
    {
        return Err(ProjectError::refused(SourceRefusal::Changed, path));
    }
    // Keep the canonical target bound/grammar: no normalization or UTF-8 cast.
    let target =
        SymlinkTarget::new(os_to_bytes(target.as_os_str())).map_err(ProjectError::Content)?;
    Ok(target.as_bytes().to_vec())
}

/// Refuses a backing whose files are the source or lie inside it.
///
/// The placement is resolved through its links first, then each ancestor's
/// host identity is compared with the source's, so a symlinked or otherwise
/// aliased spelling of a directory inside the source is recognised. Nothing
/// is begun or changed: a backing inside the source would both alter the source
/// while it is read and be acquired as part of it.
///
/// Windows carries no path-independent identity here, so the check is
/// containment of the resolved placement inside the resolved source: a
/// spelling that resolves into the source is refused whatever named it,
/// while a spelling that does not resolve to the source's own path (a subst
/// drive, an unresolvable alias) is not recognised — a recorded limitation
/// of this route.
pub(super) fn outside_source(
    source: &Path,
    metadata: &Metadata,
    placement: &Path,
) -> Result<(), ProjectError> {
    #[cfg(unix)]
    let mut ancestor = fs::canonicalize(placement)?;
    #[cfg(unix)]
    {
        let _ = source;
        use std::os::unix::fs::MetadataExt;
        loop {
            let entry = fs::symlink_metadata(&ancestor)?;
            if entry.dev() == metadata.dev() && entry.ino() == metadata.ino() {
                return Err(ProjectError::BackingInsideSource);
            }
            if !ancestor.pop() {
                return Ok(());
            }
        }
    }
    #[cfg(not(unix))]
    {
        // Linux's algorithm on the audited Windows identity: canonicalize
        // the placement once (junctions and symlinks in the spelling
        // resolve), then compare every ancestor's volume-serial + file
        // index with the source's. The identity walk sees through drive
        // aliases a DOS-path comparison misses — a subst drive keeps its
        // letter under GetFinalPathNameByHandle's DOS volume name, but its
        // directories carry the real volume's device and index.
        let source_resolved = fs::canonicalize(source)?;
        let source_identity = super::host::HostStat::of(&source_resolved, metadata)?;
        let mut ancestor = fs::canonicalize(placement)?;
        loop {
            let entry = super::host::HostStat::of(&ancestor, &fs::symlink_metadata(&ancestor)?)?;
            if entry.dev == source_identity.dev && entry.ino == source_identity.ino {
                return Err(ProjectError::BackingInsideSource);
            }
            if !ancestor.pop() {
                return Ok(());
            }
        }
    }
}
