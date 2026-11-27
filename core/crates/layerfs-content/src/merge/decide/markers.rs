//! Unresolved marker blocks in named files of one root.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::text::has_markers;
use crate::compare::text::{as_text, MAXIMUM_TEXT_BYTES};
use crate::compare::tree::TreeTable;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::name::path::PathName;
use crate::filesystem::FilesystemRootId;
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;

/// The named paths that are bounded text files holding a complete marker
/// block in `root`, in the given order. Absent and other paths are skipped.
pub fn paths_with_markers(
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
    paths: &[String],
    timing: &TimingScope<'_, Active>,
) -> ContentResult<Vec<String>> {
    let tree = TreeTable::open(reader, root)?;
    let mut out = Vec::new();
    'paths: for path in paths {
        let mut serial = tree.root_serial();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            match tree.lookup(reader, serial, &PathName::new(name)?)? {
                Some(child) => serial = child,
                None => continue 'paths,
            }
        }
        let Some(value) = tree.value(reader, serial)? else {
            continue;
        };
        if value.kind != InodeKind::RegularFile {
            continue;
        }
        let mut bytes = Vec::new();
        match crate::file::read_all_bounded(
            reader,
            value.content_root,
            MAXIMUM_TEXT_BYTES,
            &mut bytes,
            timing.child("markers.read"),
        ) {
            Err(ContentError::BoundedCapacityExceeded { .. }) => continue,
            other => {
                other?;
            }
        }
        if as_text(&bytes).is_some_and(has_markers) {
            out.push(path.clone());
        }
    }
    Ok(out)
}
