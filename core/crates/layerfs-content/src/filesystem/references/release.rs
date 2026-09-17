//! Paged release of a directory that really reached zero references.
//!
//! Removing a directory's last binding is not the end of the work: every entry it
//! held loses one reference too, recursively. The traversal is paged - one bounded
//! listing page at a time and one bounded base-record wave per page - and it stops
//! at an inode that still has an alias elsewhere, because that alias keeps the
//! inode alive. Unrelated subtrees are never read.

use std::collections::{BTreeMap, VecDeque};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::path::PathName;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::AuthenticatedObjects;

/// Work one release traversal performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReleaseWork {
    /// Directory listing pages read.
    pub pages: u64,
    /// Entries inspected.
    pub entries: u64,
    /// Base records read.
    pub base_records: u64,
    /// Inodes released.
    pub released: u64,
    /// Directories whose entries were traversed.
    pub traversed_directories: u64,
    /// Largest simultaneous cursor depth.
    pub peak_depth: usize,
}

/// One directory whose entries still need one decrement each.
struct Cursor {
    root: DirectoryRoot,
    after: Option<PathName>,
    finished: bool,
}

/// Releases the descendants of every inode that reached zero references.
///
/// `starting` lists the serials whose reference count the caller already reduced
/// to zero, in ascending order. A released inode that is a directory has its
/// entries decremented in bounded pages; a child that reaches zero and is itself a
/// directory is traversed in turn.
#[allow(clippy::too_many_arguments)]
pub fn release_zero_count(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    reducer: &mut ReferenceReducer<'_, '_>,
    starting: &[u64],
    base_batch: usize,
    page_entries: usize,
    page_bytes: usize,
    mut base_record: impl FnMut(&dyn AuthenticatedObjects, u64) -> ContentResult<InodeValue>,
) -> ContentResult<ReleaseWork> {
    let mut work = ReleaseWork::default();
    let mut queue: VecDeque<u64> = VecDeque::new();
    let mut cursors: Vec<Cursor> = Vec::new();
    let mut pending: VecDeque<u64> = starting.iter().copied().collect();
    // Base records the frontier already owns. The starting serials are read in one
    // bounded wave per `base_batch` instead of one read each, and a child that
    // reaches zero during a page walk keeps the record that page already read for
    // it. Without this the frontier asked for one record per demand, so a released
    // subtree paid a separate read for every directory it walked into - twice for
    // the ones a page had just supplied.
    let mut prefetched: BTreeMap<u64, InodeValue> = BTreeMap::new();
    for wave in starting.chunks(base_batch.max(1)) {
        let bases = lookup_many(reader, table, wave, &mut InodeReadWork::default())?;
        work.base_records = work.base_records.saturating_add(wave.len() as u64);
        for (serial, base) in wave.iter().zip(bases) {
            if let Some(base) = base {
                prefetched.insert(*serial, base);
            }
        }
    }
    loop {
        if let Some(serial) = pending.pop_front() {
            let (kind, content_root) = match reducer.state(serial)? {
                Some(PendingState::New { value, .. }) => {
                    let value = value.ok_or(ContentError::InvalidRecord("released new inode"))?;
                    (value.kind, value.content_root)
                }
                Some(PendingState::Existing { value, .. }) => {
                    let base = frontier_base(
                        reader,
                        serial,
                        &mut prefetched,
                        &mut work,
                        &mut base_record,
                    )?;
                    (
                        value.map_or(base.kind, |value| value.kind),
                        value.map_or(base.content_root, |value| value.content_root),
                    )
                }
                None => {
                    let base = frontier_base(
                        reader,
                        serial,
                        &mut prefetched,
                        &mut work,
                        &mut base_record,
                    )?;
                    (base.kind, base.content_root)
                }
            };
            if kind == InodeKind::Directory {
                cursors.push(Cursor {
                    root: DirectoryRoot(content_root),
                    after: None,
                    finished: false,
                });
            }
            continue;
        }
        let depth = cursors.len();
        let Some(cursor) = cursors.last_mut() else {
            break;
        };
        if cursor.finished {
            cursors.pop();
            continue;
        }
        let page = list_after(
            reader,
            cursor.root,
            cursor.after.as_ref(),
            page_entries,
            page_bytes,
            &mut DirectoryReadWork::default(),
        )?;
        work.pages = work.pages.saturating_add(1);
        cursor.finished = page.continuation.is_none();
        cursor.after = page.continuation;
        work.entries = work.entries.saturating_add(page.entries.len() as u64);
        work.peak_depth = work.peak_depth.max(depth);
        if page.entries.is_empty() {
            cursor.finished = true;
            continue;
        }
        let serials = page
            .entries
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        // One bounded authenticated wave supplies every child record this page
        // needs; the counts themselves come from the reducer's newest state.
        let bases = lookup_many(reader, table, &serials, &mut InodeReadWork::default())?;
        work.base_records = work.base_records.saturating_add(serials.len() as u64);
        for (index, serial) in serials.iter().enumerate() {
            let base = bases[index].ok_or(ContentError::InvalidRecord("released child"))?;
            reducer.note_removed_binding(*serial)?;
            work.released = work.released.saturating_add(1);
            let state = reducer.state(*serial)?;
            let (kind, count) = match state {
                Some(PendingState::New { value, count }) => {
                    (value.map_or(base.kind, |value| value.kind), count)
                }
                Some(PendingState::Existing { value, delta }) => {
                    let count = i128::from(base.namespace_ref_count) + i128::from(delta);
                    (
                        value.map_or(base.kind, |value| value.kind),
                        u64::try_from(count.max(0)).unwrap_or(0),
                    )
                }
                None => (base.kind, base.namespace_ref_count),
            };
            if count == 0 && kind == InodeKind::Directory {
                // The page wave already read this child's record; the frontier
                // keeps it rather than asking for it again.
                prefetched.insert(*serial, base);
                queue.push_back(*serial);
            }
        }
        while let Some(serial) = queue.pop_front() {
            pending.push_back(serial);
        }
        if work.released > 0 {
            work.traversed_directories = work.traversed_directories.saturating_add(1);
        }
    }
    Ok(work)
}

/// One frontier base record: the prefetched copy when there is one, else one read.
fn frontier_base(
    reader: &dyn AuthenticatedObjects,
    serial: u64,
    prefetched: &mut BTreeMap<u64, InodeValue>,
    work: &mut ReleaseWork,
    base_record: &mut impl FnMut(&dyn AuthenticatedObjects, u64) -> ContentResult<InodeValue>,
) -> ContentResult<InodeValue> {
    match prefetched.remove(&serial) {
        Some(base) => Ok(base),
        None => {
            work.base_records = work.base_records.saturating_add(1);
            base_record(reader, serial)
        }
    }
}
