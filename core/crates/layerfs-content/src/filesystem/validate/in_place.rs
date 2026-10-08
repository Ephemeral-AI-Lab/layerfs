//! Stored directories this operation renames inside their own base parent.
//!
//! Classification records a placement whenever a stored directory is bound
//! under a name its stored parent's base listing does not give it. When that
//! parent is the directory's own base parent, the directory was renamed and
//! did not move: it keeps its base ancestry, so listing its subtree would be
//! base-proportional work for a change that cannot close a cycle.
//!
//! The evidence is the change itself. For each parent that holds a stored
//! directory's placement, the parent's change rows are read once through the
//! input's own cursor; the names they remove are resolved in the parent's base
//! listing, one grouped lookup per window; and a removed base binding whose
//! directory is placed under the same parent marks that placement in place. A
//! scanned parent is recorded, so several renames in one parent cost one scan.
//! The work is the change rows of those parents and one placement point per
//! removed base binding; nothing resident outlives one window of names.
//!
//! A name rebound to another inode in the same row is not a removed name here:
//! a directory displaced that way keeps its placement and its walk.

use super::backed::{TopologyRecords, RECORD_WINDOW};
use super::{charge_directory, charge_inode, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{lookup_many as lookup_names, DirectoryReadWork};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::rows::view::OperationInput;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::PathName;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Marks every stored directory placed back under its own base parent.
pub(super) fn mark_in_place(
    reader: &dyn AuthenticatedObjects,
    input: &dyn OperationInput,
    table: InodeTable,
    records: &mut TopologyRecords<'_, '_>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    let mut after = None;
    loop {
        let window = records.placements_after(after)?;
        let Some((last, _)) = window.last().copied() else {
            return Ok(());
        };
        for (_, placement) in window {
            if !placement.stored || !placement.parent_stored || records.scanned(placement.parent)? {
                continue;
            }
            Scan {
                reader,
                table,
                parent: placement.parent,
                listing: None,
                records,
                work,
            }
            .run(input)?;
            records.mark_scanned(placement.parent)?;
        }
        after = Some(last);
    }
}

/// One pass over the change rows of one stored parent.
struct Scan<'a, 'r, 's, 'b> {
    reader: &'a dyn AuthenticatedObjects,
    table: InodeTable,
    parent: u64,
    /// The parent's base listing root, read when the first removed name needs it.
    listing: Option<ObjectId>,
    records: &'r mut TopologyRecords<'s, 'b>,
    work: &'r mut ValidationWork,
}

impl Scan<'_, '_, '_, '_> {
    fn run(mut self, input: &dyn OperationInput) -> ContentResult<()> {
        self.work.in_place_scans = self.work.in_place_scans.saturating_add(1);
        // A placement is stated in its parent's own header.
        let row = input
            .directory_for(self.parent)?
            .ok_or(ContentError::InvalidRecord("directory parent"))?;
        let mut changes = row.changes()?;
        let mut removed = Vec::with_capacity(RECORD_WINDOW);
        loop {
            let next = changes.next().transpose()?;
            let done = next.is_none();
            if let Some((name, binding)) = next {
                self.work.in_place_rows = self.work.in_place_rows.saturating_add(1);
                if binding.is_none() {
                    removed.push(name);
                }
            }
            if removed.len() == RECORD_WINDOW || (done && !removed.is_empty()) {
                let window = std::mem::replace(&mut removed, Vec::with_capacity(RECORD_WINDOW));
                self.resolve(window)?;
            }
            if done {
                return Ok(());
            }
        }
    }

    /// Resolves one window of removed names in the parent's base listing.
    fn resolve(&mut self, names: Vec<PathName>) -> ContentResult<()> {
        let listing = match self.listing {
            Some(listing) => listing,
            None => {
                let mut inode = InodeReadWork::default();
                let base = lookup_many(self.reader, self.table, &[self.parent], &mut inode)?;
                charge_inode(self.work, inode);
                let listing = base
                    .into_iter()
                    .next()
                    .flatten()
                    .ok_or(ContentError::InvalidRecord("missing base inode"))?
                    .content_root;
                self.listing = Some(listing);
                listing
            }
        };
        let mut directory = DirectoryReadWork::default();
        let base = lookup_names(self.reader, DirectoryRoot(listing), &names, &mut directory)?;
        charge_directory(self.work, directory);
        let mut kept = Vec::new();
        for serial in base.into_iter().flatten() {
            // The base bound this serial here and the change removes that name:
            // a placement of it under this same parent is a rename.
            if let Some(placement) = self.records.placed(serial)? {
                if placement.parent == self.parent && !placement.in_place {
                    kept.push((serial, placement));
                }
            }
        }
        self.work.in_place_directories = self
            .work
            .in_place_directories
            .saturating_add(kept.len() as u64);
        self.records.mark_in_place(kept)
    }
}
