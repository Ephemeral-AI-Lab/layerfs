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
pub(super) fn outside_source(source: &Metadata, placement: &Path) -> Result<(), ProjectError> {
    #[cfg(unix)]
    let mut ancestor = fs::canonicalize(placement)?;
    #[cfg(not(unix))]
    let ancestor = fs::canonicalize(placement)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
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
    #[cfg(windows)]
    {
        // Windows std carries no path-independent identity here, and the
        // entry's own identity probe is per-path; an aliased spelling of
        // the source is not recognised by this check (a recorded
        // limitation).
        let _ = (source, &ancestor);
        Ok(())
    }
}
