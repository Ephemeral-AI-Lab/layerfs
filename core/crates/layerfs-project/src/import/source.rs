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
        // Both sides are canonicalized, so junctions and symlinks in the
        // placement's spelling resolve before the comparison.
        let source = fs::canonicalize(source)?;
        let ancestor = fs::canonicalize(placement)?;
        if ancestor == source || ancestor.starts_with(&source) {
            return Err(ProjectError::BackingInsideSource);
        }
        let _ = metadata;
        Ok(())
    }
}
