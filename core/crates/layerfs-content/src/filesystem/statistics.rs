//! Logical byte accounting over canonical inode values, independent of encoding.
use super::{
    inode::read::{rows_after, InodeReadWork},
    sorted::format::CompactInodes,
    FilesystemRootId,
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{
    compare::{
        sorted::{difference, DifferenceWork},
        tree::ChangedDirectories,
        TreeTable,
    },
    AuthenticatedObjects, ContentError, ContentResult, FileView, ObjectId,
};
use layerfs_telemetry::timer::Timing;

fn bytes(reader: &dyn AuthenticatedObjects, value: Option<&InodeValue>) -> ContentResult<i128> {
    let Some(value) = value.filter(|value| value.kind == InodeKind::RegularFile) else {
        return Ok(0);
    };
    let (length, _) = Timing::disabled("filesystem.logical", |timing| {
        FileView::open(reader, value.content_root, timing.child("length"))
            .map(|view| view.logical_len())
    });
    Ok(i128::from(length?) * i128::from(value.namespace_ref_count))
}

/// Counts an initial root in bounded inode-leaf windows; reads file roots, not payload streams.
pub fn logical_size(
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
) -> ContentResult<u64> {
    let tree = TreeTable::open(reader, root)?;
    let mut after = None;
    let mut sum = 0i128;
    loop {
        let rows = rows_after(reader, tree.table, after, &mut InodeReadWork::default())?;
        if rows.is_empty() {
            break;
        }
        for (serial, value) in rows {
            after = Some(serial);
            sum += bytes(reader, Some(&value))?;
        }
    }
    u64::try_from(sum).map_err(|_| ContentError::InvalidRecord("logical byte overflow"))
}

/// Changed file roots one length demand carries: a bounded resident window.
pub const LENGTH_WINDOW: usize = 1024;

/// Updates known logical size by page-skipping inode differences, including
/// hardlink count changes, with file lengths from `lengths`, which answers a
/// window of at most [`LENGTH_WINDOW`] regular-file content roots in order.
///
/// The lengths of the regular files the changed rows name are asked a window
/// at a time, so a Store answers them from its catalogue in one demand instead
/// of one payload read per file.
pub fn logical_size_after_with(
    reader: &dyn AuthenticatedObjects,
    before: FilesystemRootId,
    after: FilesystemRootId,
    known: u64,
    lengths: &mut dyn FnMut(&[ObjectId]) -> ContentResult<Vec<u64>>,
) -> ContentResult<u64> {
    logical_size_and_directories(reader, before, after, known, lengths, None)
}

/// [`logical_size_after_with`], noting in `directories` every directory
/// whose listing the change rewrote, from the same difference walk (#810
/// F4): a Commit's binding hints then need no second walk over the two
/// inode tables ([`crate::compare::tree::directory_bindings`]).
pub fn logical_size_and_directories(
    reader: &dyn AuthenticatedObjects,
    before: FilesystemRootId,
    after: FilesystemRootId,
    known: u64,
    lengths: &mut dyn FnMut(&[ObjectId]) -> ContentResult<Vec<u64>>,
    mut directories: Option<&mut ChangedDirectories>,
) -> ContentResult<u64> {
    if let Some(directories) = directories.as_deref_mut() {
        *directories = ChangedDirectories::between(before, after);
    }
    if before == after {
        return Ok(known);
    }
    let left = TreeTable::open(reader, before)?;
    let right = TreeTable::open(reader, after)?;
    let mut sum = i128::from(known);
    let mut window = Window::default();
    difference::<CompactInodes>(
        reader,
        Some(left.table.root),
        Some(right.table.root),
        &mut DifferenceWork::default(),
        |serial, before, after| {
            if let Some(directories) = directories.as_mut() {
                directories.note(*serial, before, after);
            }
            for (value, sign) in [(after, 1), (before, -1)] {
                if let Some(value) = value.filter(|value| value.kind == InodeKind::RegularFile) {
                    window.push(value, sign);
                    if window.roots.len() == LENGTH_WINDOW {
                        sum += window.drain(lengths)?;
                    }
                }
            }
            Ok(())
        },
    )?;
    sum += window.drain(lengths)?;
    u64::try_from(sum).map_err(|_| ContentError::InvalidRecord("logical byte overflow"))
}

/// Pending file roots and each one's signed link count.
#[derive(Default)]
struct Window {
    roots: Vec<ObjectId>,
    weights: Vec<i128>,
}
impl Window {
    fn push(&mut self, value: &InodeValue, sign: i128) {
        self.roots.push(value.content_root);
        self.weights
            .push(sign * i128::from(value.namespace_ref_count));
    }
    fn drain(
        &mut self,
        lengths: &mut dyn FnMut(&[ObjectId]) -> ContentResult<Vec<u64>>,
    ) -> ContentResult<i128> {
        if self.roots.is_empty() {
            return Ok(0);
        }
        let answered = lengths(&self.roots)?;
        if answered.len() != self.roots.len() {
            return Err(ContentError::InvalidRecord("file length cardinality"));
        }
        let sum = answered
            .iter()
            .zip(&self.weights)
            .map(|(length, weight)| i128::from(*length) * weight)
            .sum();
        self.roots.clear();
        self.weights.clear();
        Ok(sum)
    }
}
