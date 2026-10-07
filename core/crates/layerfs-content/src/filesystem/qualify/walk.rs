//! The qualification pass: every inode row once, every reachable binding once,
//! then the count closure that leaves no inode unbound or unreachable.
use std::collections::BTreeMap;

use super::proof::{QualificationWork, QualifiedRoot, RootContext};
use super::records::{Records, Row};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::list_after;
use crate::filesystem::identity::MAXIMUM_INODE_SERIAL;
use crate::filesystem::inode::read::{rows_after, InodeTable};
use crate::filesystem::limits::MAXIMUM_NAME_BYTES;
use crate::filesystem::path::PathName;
use crate::filesystem::root::FilesystemRoot;
use crate::filesystem::sorted::format::NAME_LENGTH_BYTES;
use crate::filesystem::sorted::DirectoryRoot;
use crate::object::inode_leaf::InodeKind;
use crate::object::{AuthenticatedObjects, ObjectId};
use crate::IndexedConstructionBacking;

/// Bindings counted per listing window and per guarded record batch.
const WINDOW: usize = 64;
const WINDOW_BYTES: usize = WINDOW * (NAME_LENGTH_BYTES + 8 + MAXIMUM_NAME_BYTES);

/// Qualifies the whole namespace of one immutable root.
///
/// The pass is explicit and paid: it reads every inode-table page and every
/// directory page of the tree, and one record per inode and per directory in
/// `records`, a fresh scope the caller acquired for this pass and releases
/// afterwards. Resident state is one page window and two queue cursors.
///
/// It refuses a root of another scope, profile or serial; an invalid inode
/// row; a binding to a serial the table does not hold; any binding to the root
/// directory; a directory bound more than once, which is every alias and every
/// cycle reachable from the root; an inode bound more often than it declares;
/// and, by the final count closure, an inode bound less often than it declares,
/// which is every unreachable inode and every island.
///
/// File mappings, symlink targets and attribute trees are not walked. Their
/// owning readers check them under bounded demand reads.
/// `work` is reset at entry and reports only this attempted pass.
pub fn qualify_root(
    reader: &dyn AuthenticatedObjects,
    records: &mut dyn IndexedConstructionBacking,
    context: &RootContext,
    work: &mut QualificationWork,
) -> ContentResult<QualifiedRoot> {
    *work = QualificationWork::default();
    let root = FilesystemRoot::decode(&reader.read_canonical(context.root.0)?)?;
    context.check(root)?;
    let root_serial = root.root_inode().serial();
    let mut pass = Pass {
        reader,
        records: Records::begin(records, context.root, root, work)?,
        root_serial,
    };
    let (declared, directories, listing) = pass.record_inodes(root)?;
    let mut next = Some(listing);
    while let Some(directory) = next {
        pass.bind_directory(directory)?;
        next = pass.records.next_directory()?.map(|(_, listing)| listing);
    }
    let work = pass.records.work();
    // No inode is bound more often than it declares, so equal totals bind
    // every inode exactly as declared: none is unreachable.
    if work.bindings != declared {
        return Err(ContentError::InvalidRecord("qualified inode binding count"));
    }
    if work.directories != directories {
        return Err(ContentError::InvalidRecord("qualified directory count"));
    }
    Ok(QualifiedRoot {
        id: context.root,
        root,
        inodes: work.inodes,
        directories,
        bindings: work.bindings,
    })
}

struct Pass<'a, 'r, 'w> {
    reader: &'a dyn AuthenticatedObjects,
    records: Records<'r, 'w>,
    root_serial: u64,
}

impl Pass<'_, '_, '_> {
    /// Records every inode row in serial order. Returns the declared binding
    /// total, the directory count and the root directory's listing root.
    fn record_inodes(&mut self, root: FilesystemRoot) -> ContentResult<(u64, u64, ObjectId)> {
        let table = InodeTable {
            root: root.inode_table(),
            root_serial: self.root_serial,
        };
        let (mut declared, mut directories, mut listing) = (0_u64, 0_u64, None);
        let mut after = None;
        loop {
            let rows = rows_after(self.reader, table, after, &mut self.records.work().inode)?;
            let Some((last, _)) = rows.last().copied() else {
                break;
            };
            let mut changes = Vec::with_capacity(rows.len());
            for (serial, value) in rows {
                if serial == 0 || serial > MAXIMUM_INODE_SERIAL {
                    return Err(ContentError::InvalidRecord("inode serial"));
                }
                value.validate(serial == self.root_serial)?;
                declared = declared
                    .checked_add(value.namespace_ref_count)
                    .ok_or(ContentError::LengthOverflow)?;
                if value.kind == InodeKind::Directory {
                    directories += 1;
                }
                if serial == self.root_serial {
                    listing = Some(value.content_root);
                }
                changes.push(Records::insert(serial, &Row::new(value)));
            }
            let work = self.records.work();
            work.inodes = work.inodes.saturating_add(changes.len() as u64);
            self.records.apply(changes)?;
            after = Some(last);
        }
        let listing =
            listing.ok_or(ContentError::InvalidRecord("missing filesystem root inode"))?;
        Ok((declared, directories, listing))
    }

    /// Counts every binding of one directory in bounded name-ordered windows.
    fn bind_directory(&mut self, listing: ObjectId) -> ContentResult<()> {
        let mut after: Option<PathName> = None;
        loop {
            let page = list_after(
                self.reader,
                DirectoryRoot(listing),
                after.as_ref(),
                WINDOW,
                WINDOW_BYTES,
                &mut self.records.work().directory,
            )?;
            let mut targets: BTreeMap<u64, u64> = BTreeMap::new();
            let mut previous = after.as_ref();
            for (name, serial) in &page.entries {
                if previous.is_some_and(|previous| previous >= name) {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                previous = Some(name);
                *targets.entry(*serial).or_default() += 1;
            }
            let work = self.records.work();
            work.bindings = work
                .bindings
                .checked_add(page.entries.len() as u64)
                .ok_or(ContentError::LengthOverflow)?;
            self.bind(targets)?;
            match page.continuation {
                Some(name) => after = Some(name),
                None => break,
            }
        }
        let work = self.records.work();
        work.directories = work.directories.saturating_add(1);
        Ok(())
    }

    /// Applies one window's bindings to their targets in one guarded batch and
    /// queues each directory at its first and only binding.
    fn bind(&mut self, targets: BTreeMap<u64, u64>) -> ContentResult<()> {
        if targets.is_empty() {
            return Ok(());
        }
        let mut changes = Vec::with_capacity(2 * targets.len());
        let mut queued = Vec::new();
        for (serial, added) in targets {
            if serial == self.root_serial {
                return Err(ContentError::InvalidRecord(
                    "qualified binding names the root directory",
                ));
            }
            let (mut row, old) = self
                .records
                .row(serial)?
                .ok_or(ContentError::InvalidRecord(
                    "qualified binding names a missing inode",
                ))?;
            row.bound = row
                .bound
                .checked_add(added)
                .ok_or(ContentError::LengthOverflow)?;
            if row.kind == InodeKind::Directory {
                if row.bound > 1 {
                    return Err(ContentError::InvalidRecord(
                        "qualified directory has more than one binding",
                    ));
                }
                queued.push(serial);
            }
            if row.bound > row.declared {
                return Err(ContentError::InvalidRecord(
                    "qualified binding count exceeds the inode",
                ));
            }
            changes.push(Records::replace(serial, old, &row));
        }
        self.records.apply_with_queue(changes, &queued)
    }
}
