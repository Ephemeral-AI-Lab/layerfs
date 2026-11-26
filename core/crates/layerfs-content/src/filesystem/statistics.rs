//! Logical byte accounting over canonical inode values, independent of encoding.
use super::{
    inode::read::{rows_after, InodeReadWork},
    sorted::format::CompactInodes,
    state::{self, SerialState},
    FilesystemRootId,
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{
    compare::{
        sorted::{difference, DifferenceWork},
        tree::ChangedDirectories,
        TreeTable,
    },
    AuthenticatedObjects, ContentError, ContentResult, FileView, IndexedConstructionBacking,
    ObjectId,
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
                        sum = add_bytes(sum, window.drain(lengths)?)?;
                    }
                }
            }
            Ok(())
        },
    )?;
    sum = add_bytes(sum, window.drain(lengths)?)?;
    u64::try_from(sum).map_err(|_| ContentError::InvalidRecord("logical byte overflow"))
}

/// A successful construction's logical-count input, bound to its exact roots.
/// Complete length windows live in the still-held construction scope. The
/// final partial window and the existing bounded directory hints stay local.
#[derive(Debug)]
pub struct LogicalChanges {
    before: FilesystemRootId,
    after: FilesystemRootId,
    directories: ChangedDirectories,
    full_windows: u64,
    partial: Window,
}

impl LogicalChanges {
    /// Whether this result measures exactly the roots chosen for publication.
    pub fn matches(&self, before: FilesystemRootId, after: FilesystemRootId) -> bool {
        (self.before, self.after) == (before, after)
    }

    /// Resolves lengths through the authoritative reader after Save completion.
    /// The caller still owns the same construction scope that wrote the windows.
    pub fn count(
        mut self,
        records: &mut dyn IndexedConstructionBacking,
        known: u64,
        lengths: &mut dyn FnMut(&[ObjectId]) -> ContentResult<Vec<u64>>,
    ) -> ContentResult<(u64, ChangedDirectories)> {
        if self.before == self.after {
            return Ok((known, self.directories));
        }
        let mut sum = i128::from(known);
        let mut window = Window::default();
        for serial in 1..=self.full_windows {
            let bytes = records
                .get(state::key(state::LOGICAL_LENGTHS, serial))?
                .ok_or(ContentError::InvalidRecord("logical length window missing"))?;
            window.fill(&bytes)?;
            drop(bytes);
            sum = add_bytes(sum, window.drain(lengths)?)?;
        }
        sum = add_bytes(sum, self.partial.drain(lengths)?)?;
        let counted = u64::try_from(sum).map_err(|_| logical_overflow())?;
        Ok((counted, self.directories))
    }
}

/// Provisional counts and hints observed by the very leaf merge that writes the
/// inode table. They become usable only when that update returns its root.
#[derive(Default)]
pub(super) struct LogicalTransitions {
    directories: ChangedDirectories,
    full_windows: u64,
    partial: Window,
}

impl LogicalTransitions {
    pub fn note(
        &mut self,
        state: &SerialState<'_>,
        serial: u64,
        before: Option<InodeValue>,
        after: Option<InodeValue>,
    ) -> ContentResult<()> {
        if before == after {
            return Ok(());
        }
        self.directories
            .note(serial, before.as_ref(), after.as_ref());
        for (value, sign) in [(after, 1), (before, -1)] {
            if let Some(value) = value.filter(|value| value.kind == InodeKind::RegularFile) {
                self.partial.push(&value, sign);
                if self.partial.roots.len() == LENGTH_WINDOW {
                    let serial = self
                        .full_windows
                        .checked_add(1)
                        .ok_or(ContentError::LengthOverflow)?;
                    state.apply(vec![state::change(
                        state::LOGICAL_LENGTHS,
                        serial,
                        None,
                        self.partial.encode(),
                    )])?;
                    self.full_windows = serial;
                    self.partial.clear();
                }
            }
        }
        Ok(())
    }

    pub fn finish(mut self, before: FilesystemRootId, after: FilesystemRootId) -> LogicalChanges {
        self.directories.bind_roots(before, after);
        LogicalChanges {
            before,
            after,
            directories: self.directories,
            full_windows: self.full_windows,
            partial: self.partial,
        }
    }
}

fn logical_overflow() -> ContentError {
    ContentError::InvalidRecord("logical byte overflow")
}

fn add_bytes(left: i128, right: i128) -> ContentResult<i128> {
    left.checked_add(right).ok_or_else(logical_overflow)
}

const WEIGHTED_ROOT_BYTES: usize = crate::object::DIGEST_BYTES + std::mem::size_of::<i128>();
const WINDOW_BYTES: usize = LENGTH_WINDOW * WEIGHTED_ROOT_BYTES;
const _: () =
    assert!(WINDOW_BYTES + std::mem::size_of::<crate::ConstructionRecordChange>() <= 65_536);

/// Pending file roots and each one's signed link count.
#[derive(Debug, Default)]
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
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(WINDOW_BYTES);
        for (root, weight) in self.roots.iter().zip(&self.weights) {
            bytes.extend_from_slice(root.as_bytes());
            bytes.extend_from_slice(&weight.to_be_bytes());
        }
        bytes
    }
    fn fill(&mut self, bytes: &[u8]) -> ContentResult<()> {
        if bytes.len() != WINDOW_BYTES {
            return Err(ContentError::InvalidRecord("logical length window size"));
        }
        for item in bytes.chunks_exact(WEIGHTED_ROOT_BYTES) {
            let (root, weight) = item.split_at(crate::object::DIGEST_BYTES);
            let weight =
                i128::from_be_bytes(weight.try_into().map_err(|_| ContentError::UnexpectedEof)?);
            if weight.unsigned_abs() > u128::from(u64::MAX) {
                return Err(ContentError::InvalidRecord("logical length weight"));
            }
            self.roots.push(ObjectId::from_bytes(root)?);
            self.weights.push(weight);
        }
        Ok(())
    }
    fn clear(&mut self) {
        self.roots.clear();
        self.weights.clear();
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
            .try_fold(0, |sum, (length, weight)| {
                let weighted = i128::from(*length)
                    .checked_mul(*weight)
                    .ok_or_else(logical_overflow)?;
                add_bytes(sum, weighted)
            })?;
        self.clear();
        Ok(sum)
    }
}
