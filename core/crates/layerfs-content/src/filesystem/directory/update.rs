//! Sorted initial and final directory bindings, with semantic edge observation.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::path::PathName;
use crate::filesystem::sorted::finish::{
    apply_directory_changes as apply_sorted, emit_empty_directory, DirectoryRoot,
};
use crate::filesystem::sorted::page::MAXIMUM_SCRATCH_BYTES;
use crate::filesystem::sorted::SortedWork;

pub use crate::filesystem::sorted::finish::DirectoryRoot as DirectoryRootHandle;

/// Emits the canonical empty directory this profile stores for a real empty result.
pub fn empty_directory(objects: &mut FilesystemObjects<'_>) -> ContentResult<DirectoryRoot> {
    emit_empty_directory(objects)
}

/// Applies strictly sorted unique final bindings to an optional base directory.
///
/// `observe` reports each original/final inode serial the merge actually saw.
pub fn apply_bindings(
    objects: &mut FilesystemObjects<'_>,
    base: Option<DirectoryRoot>,
    changes: impl Iterator<Item = ContentResult<(PathName, Option<u64>)>>,
    scratch_limit: usize,
    observe: &mut dyn FnMut(Option<u64>, Option<u64>) -> ContentResult<()>,
) -> ContentResult<(DirectoryRoot, SortedWork)> {
    apply_sorted(objects, base, changes, scratch_limit, observe)
}

/// Builds a complete directory from strictly sorted unique bindings.
pub fn build_directory(
    objects: &mut FilesystemObjects<'_>,
    entries: impl Iterator<Item = ContentResult<(PathName, u64)>>,
) -> ContentResult<(DirectoryRoot, SortedWork)> {
    apply_bindings(
        objects,
        None,
        entries.map(|row| row.map(|(name, serial)| (name, Some(serial)))),
        MAXIMUM_SCRATCH_BYTES,
        &mut |_, _| Ok(()),
    )
}

/// Checks one requested binding change for order and duplicate keys.
pub fn check_change_order(changes: &[(PathName, Option<u64>)]) -> ContentResult<()> {
    if changes.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(ContentError::NonCanonicalOrdering);
    }
    Ok(())
}
