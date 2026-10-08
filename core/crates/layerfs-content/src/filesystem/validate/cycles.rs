//! Effective-tree proof from placement evidence: territory and rooted walks.
//!
//! Classification recorded one placement for every directory binding this
//! operation states that the base does not already have. A placed stored
//! directory whose placement names its own base parent was renamed in place
//! (`in_place.rs`): it keeps its base parent, so everywhere below it is treated
//! as a stored directory with no placement, and "unplaced" includes it. Two
//! passes turn the records into the proof that no directory of the result sits
//! on a cycle.
//!
//! **Territory.** A stored directory placed away from its base parent has
//! moved, and the stored, unplaced directories still under it through their
//! bindings in their base parents moved with it: they are its territory. The
//! pass lists each moved directory's effective subtree once, entering only
//! stored unplaced directories, and marks every listed directory that has a
//! header with the moved directory that owns it. It runs only when a stored
//! directory moved *and* some placement that is not in place names a stored,
//! unplaced parent other than the root; otherwise no placement can land inside
//! a territory and no base page is listed at all.
//!
//! **Rooted proof.** From every placed directory the walk goes upward: a
//! directory placed away from its base parent, or allocated, continues at the
//! parent its placement names, and a stored unplaced directory continues at the
//! moved directory owning its territory. The walk is proven when it reaches the
//! root, a stored unplaced directory outside every territory (its base
//! position, whose base ancestry this operation left intact) or a directory an
//! earlier walk already proved. A second walk over the same path marks it
//! rooted, so later walks stop there and the whole pass is linear in the
//! placements.
//!
//! **Why this is sound.** The base is a tree by induction, and the reducer
//! refuses any directory whose derived count is not one, so in an accepted
//! result every directory has exactly one binding. A directory that keeps its
//! base parent, under whatever name, keeps its base ancestry, and a cycle
//! cannot consist of such directories alone: it holds a directory placed away
//! from its base parent or allocated, and every placed directory is a walk's
//! start. Following the cycle upward from it, each such member's only binding
//! is its placement, and each stored unplaced member is reached from a
//! placement stated in it - so it has a header - and keeps its binding in its
//! base parent, whose ancestors inside the cycle lead through bindings in base
//! parents to a moved member: it lies in that member's territory and is marked,
//! because such a cycle contains both a moved stored directory and a placement,
//! not in place, under a stored, unplaced, non-root parent, which is exactly
//! when the territory pass runs. The walk therefore never leaves the cycle, and
//! a simple upward path holds each placed directory at most once with at most
//! one territory step between two of them: a walk longer than twice the
//! placement count plus two has repeated a directory and is refused.
//!
//! **Why it terminates before the alias verdict.** The reducer's verdict comes
//! after this proof, so a stored directory may still be both in its base
//! position and placed. Nothing here depends on that being resolved: the
//! territory pass enters only stored unplaced directories, and such a
//! directory's effective binding in a listed parent is a binding in its base
//! parent - its base binding, or the one placement that replaced a base
//! binding the same header displaces by removing the name or giving it to
//! another inode - so the pass follows edges of the base tree and lists each
//! directory at most once; the rooted walk is bounded by its step count
//! whatever the records say. An aliased directory can make the proof report a
//! cycle one binding too early or accept a batch the reducer then refuses; it
//! cannot make it loop or accept a cycle.
//!
//! **Dropped ends.** A walk that ends at a directory this operation allocates
//! and nothing binds reaches neither the root nor a base position, so it is
//! refused as unreachable whatever started it: a fresh directory, or a stored
//! one bound or moved there. Nothing is recorded for such a walk. Files and
//! symlinks are not topology evidence and are left to their derived counts.

use super::backed::{Placement, TopologyRecords, RECORD_WINDOW};
use super::entries::EffectiveEntries;
use super::in_place::mark_in_place;
use super::{charge_inode, charge_site, FilesystemTopology, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::rows::view::OperationInput;
use crate::object::inode_leaf::InodeKind;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Proves the effective tree this operation's placements produce.
pub(super) fn check_effective_tree(
    reader: &dyn AuthenticatedObjects,
    input: &dyn OperationInput,
    topology: &FilesystemTopology,
    records: &mut TopologyRecords<'_, '_>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    if records.placements() == 0 {
        return Ok(());
    }
    let before = work.inode_pages_read;
    let result = Proof {
        reader,
        input,
        table: topology.table,
        root: input.root_serial(),
        records,
        work,
        territories: false,
    }
    .run();
    // A base-less operation reads no inode page here; the site keeps its name.
    if topology.base.is_some() {
        charge_site(work, before, |sites| &mut sites.cycles);
    } else {
        charge_site(work, before, |sites| &mut sites.reachability);
    }
    result
}

/// One upward step of a rooted walk.
enum Step {
    /// Continue at this directory.
    Up(u64),
    /// The root, a base position or a directory already proven.
    Rooted,
    /// A directory this operation allocates and nothing binds.
    Dropped,
}

struct Proof<'a, 'r, 's, 'b> {
    reader: &'a dyn AuthenticatedObjects,
    input: &'a dyn OperationInput,
    table: Option<InodeTable>,
    root: u64,
    records: &'r mut TopologyRecords<'s, 'b>,
    work: &'r mut ValidationWork,
    territories: bool,
}

impl Proof<'_, '_, '_, '_> {
    fn run(mut self) -> ContentResult<()> {
        // No territory is possible unless a stored directory is placed and some
        // placement names a stored parent other than the root; only then is it
        // worth learning which of the placed stored directories never moved.
        if self.records.stored_placed() && self.records.anchored() {
            let table = self
                .table
                .ok_or(ContentError::InvalidRecord("missing base inode"))?;
            mark_in_place(self.reader, self.input, table, self.records, self.work)?;
            if self.records.moved() && self.landing()? {
                self.territories = true;
                self.mark_territories()?;
            }
        }
        let limit = self
            .records
            .placements()
            .saturating_mul(2)
            .saturating_add(2);
        let mut after = None;
        loop {
            let window = self.records.placements_after(after)?;
            let Some((last, _)) = window.last().copied() else {
                return Ok(());
            };
            for (directory, _) in window {
                self.prove(directory, limit)?;
            }
            after = Some(last);
        }
    }

    /// True when this operation placed the directory away from its base parent
    /// or allocated it: an in-place directory counts as unplaced.
    fn displaced(&self, directory: u64) -> ContentResult<bool> {
        Ok(self
            .records
            .placed(directory)?
            .is_some_and(|placement| !placement.in_place))
    }

    /// True when a placement that is not in place names a stored parent other
    /// than the root that kept its own base parent.
    fn landing(&self) -> ContentResult<bool> {
        let mut after = None;
        loop {
            let window = self.records.placements_after(after)?;
            let Some((last, _)) = window.last().copied() else {
                return Ok(false);
            };
            for (_, placement) in window {
                if !placement.in_place
                    && placement.parent_stored
                    && placement.parent != self.root
                    && !self.displaced(placement.parent)?
                {
                    return Ok(true);
                }
            }
            after = Some(last);
        }
    }

    /// Lists every moved stored directory's surviving subtree once.
    fn mark_territories(&mut self) -> ContentResult<()> {
        let table = self
            .table
            .ok_or(ContentError::InvalidRecord("missing base inode"))?;
        let mut after = None;
        loop {
            let window = self.records.placements_after(after)?;
            let Some((last, _)) = window.last().copied() else {
                return Ok(());
            };
            let moved = window
                .into_iter()
                .filter(|(_, placement)| placement.stored && !placement.in_place)
                .map(|(directory, _)| directory)
                .collect::<Vec<_>>();
            let mut inode = InodeReadWork::default();
            let bases = lookup_many(self.reader, table, &moved, &mut inode)?;
            charge_inode(self.work, inode);
            for (directory, base) in moved.into_iter().zip(bases) {
                let base = base.ok_or(ContentError::InvalidRecord("missing base inode"))?;
                self.territory(table, directory, base.content_root)?;
            }
            after = Some(last);
        }
    }

    /// One queue walk over the stored, unplaced directories under `owner`.
    fn territory(&mut self, table: InodeTable, owner: u64, listing: ObjectId) -> ContentResult<()> {
        self.records.push(vec![(owner, listing)])?;
        while let Some((directory, listing)) = self.records.pop()? {
            self.work.territory_directories = self.work.territory_directories.saturating_add(1);
            let (reader, input) = (self.reader, self.input);
            let row = input.directory_for(directory)?;
            if directory != owner && row.is_some() {
                self.records.mark_territory(directory, owner)?;
            }
            // The effective listing drops the names this operation removed, so
            // a directory that left this subtree is not followed out of it.
            let mut entries = EffectiveEntries::new(reader, Some(listing), row.as_ref())?;
            loop {
                let mut page = Vec::with_capacity(RECORD_WINDOW);
                while page.len() < RECORD_WINDOW {
                    match entries.next_entry(self.work)? {
                        Some((_, serial)) => page.push(serial),
                        None => break,
                    }
                }
                if page.is_empty() {
                    break;
                }
                self.work.territory_entries = self
                    .work
                    .territory_entries
                    .saturating_add(page.len() as u64);
                let mut inode = InodeReadWork::default();
                let values = lookup_many(reader, table, &page, &mut inode)?;
                charge_inode(self.work, inode);
                let mut inside = Vec::with_capacity(page.len());
                for (serial, value) in page.into_iter().zip(values) {
                    // No base record is a directory this operation allocates,
                    // and a displaced one moved: both carry their own placement.
                    // A directory renamed in place is still this one's child.
                    let Some(value) = value else {
                        continue;
                    };
                    if value.kind == InodeKind::Directory && !self.displaced(serial)? {
                        inside.push((serial, value.content_root));
                    }
                }
                self.records.push(inside)?;
            }
        }
        Ok(())
    }

    fn step(&self, directory: u64) -> ContentResult<Step> {
        if directory == self.root || self.records.rooted(directory)? {
            return Ok(Step::Rooted);
        }
        if let Some(Placement {
            parent,
            in_place: false,
            ..
        }) = self.records.placed(directory)?
        {
            return Ok(Step::Up(parent));
        }
        if self.table.is_none() || self.input.is_new(directory)? {
            return Ok(Step::Dropped);
        }
        if self.territories {
            if let Some(owner) = self.records.territory(directory)? {
                return Ok(Step::Up(owner));
            }
        }
        Ok(Step::Rooted)
    }

    /// Walks upward from one placed directory, then marks the proven path.
    fn prove(&mut self, start: u64, limit: u64) -> ContentResult<()> {
        let mut directory = start;
        let mut steps = 0_u64;
        loop {
            match self.step(directory)? {
                Step::Rooted => break,
                // Neither the root nor a base position: nothing holds this path.
                Step::Dropped => return Err(ContentError::InvalidRecord("effective tree cycle")),
                Step::Up(parent) => {
                    steps += 1;
                    if steps > limit {
                        return Err(ContentError::InvalidRecord("effective tree cycle"));
                    }
                    directory = parent;
                }
            }
        }
        self.work.ancestry_steps = self.work.ancestry_steps.saturating_add(steps);
        let mut directory = start;
        let mut path = Vec::with_capacity(RECORD_WINDOW);
        while let Step::Up(parent) = self.step(directory)? {
            path.push(directory);
            if path.len() == RECORD_WINDOW {
                let window = std::mem::replace(&mut path, Vec::with_capacity(RECORD_WINDOW));
                self.records.mark_rooted(window)?;
            }
            self.work.ancestry_steps = self.work.ancestry_steps.saturating_add(1);
            directory = parent;
        }
        self.records.mark_rooted(path)
    }
}
