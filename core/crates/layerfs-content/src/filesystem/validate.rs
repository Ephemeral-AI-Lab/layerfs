//! Checked inputs: membership, identity, topology and retained bindings.
//!
//! These checks run before any promise is made about the result. They are
//! incremental where the input allows it: an update to a valid base validates the
//! affected bindings, identities and effective parents, while a structure that can
//! form a cycle is walked with a declared bounded work limit. Authentication of
//! object bytes is not topology validation and neither is a caller assertion: new
//! identities are rechecked against the base they claim to be absent from, and
//! every final count is derived from checked retained bindings.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::identity::InodeScope;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::input::FilesystemInput;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one operation's validation performed.
///
/// Validation reads the base it is about to change: the parent records, the
/// bindings of the directories it walks and the entries of every directory it
/// inspects for a cycle. Those reads are part of the operation's work and are
/// reported with it, exactly like the merge's own reads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationWork {
    /// Canonical objects read by the checks.
    pub objects_read: u64,
    /// Canonical read waves the checks issued.
    pub read_waves: u64,
    /// Inode records the checks demanded.
    pub inode_demands: u64,
    /// Inode pages the checks read.
    pub inode_pages_read: u64,
    /// Directory pages the checks read.
    pub directory_pages_read: u64,
    /// Directory entries the checks examined.
    pub entries_examined: u64,
    /// Inode pages read, split by the call site that asked for them.
    ///
    /// `inode_pages_read` is one total, and a total cannot say which of the
    /// checks read the table: the grouped prefetch charges one wave per *level*
    /// (`inode/read.rs`), a single-serial descent charges one wave *and* one page
    /// per level, and four different regions can make either call. Each is
    /// charged here as the growth of `inode_pages_read` across its own region, so
    /// the six sites sum to the total exactly. `FilesystemTopology::load` reads
    /// the base root once per operation and charges no counter at all, so it has
    /// no site here rather than a site of zero.
    pub inode_pages_by_site: ValidationReadSites,
}

/// Where one validation's inode pages were read.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationReadSites {
    /// The allocator precondition, one grouped demand per 64 new serials.
    pub allocation: u64,
    /// The batch's grouped prefetch of parents and bound children.
    pub prefetch: u64,
    /// The per-binding loop's own record lookups.
    pub bindings: u64,
    /// The parent-alias pass and its per-parent listings.
    pub aliases: u64,
    /// The effective-tree cycle walk over rebound directories.
    pub cycles: u64,
    /// The build-reachability walk a base-less operation takes instead.
    pub reachability: u64,
}

/// Charges the inode pages read since `before` to one site.
///
/// The site counters are read from the same total the operation already
/// maintains, so an instrument added here cannot invent a read: the six sites sum
/// to `inode_pages_read` or the difference is a read outside every region, which
/// is itself visible in the row.
fn charge_site(
    work: &mut ValidationWork,
    before: u64,
    site: fn(&mut ValidationReadSites) -> &mut u64,
) {
    let delta = work.inode_pages_read.saturating_sub(before);
    let slot = site(&mut work.inode_pages_by_site);
    *slot = slot.saturating_add(delta);
}

/// Entries the effective-tree cycle check may inspect before it refuses.
///
/// This is a declared work limit, not a property of the tree: a legal rename of a
/// directory whose effective subtree is larger than this is refused with the same
/// error a genuine cycle gets, because proving it acyclic would cost more entries
/// than the operation declared it would walk. The figure is part of the
/// operation's resource contract, the entries the walk examines are charged to
/// `ValidationWork::entries_examined` so a caller can see how close it came, and
/// both consequences of its per-walk scope are stated with
/// [`MAXIMUM_WALK_ENTRIES`](crate::filesystem::limits::MAXIMUM_WALK_ENTRIES).
pub const MAXIMUM_CYCLE_CHECK_ENTRIES: usize = crate::filesystem::limits::MAXIMUM_WALK_ENTRIES;
/// Serial demands one allocator-precondition wave may make.
pub const ALLOCATION_CHECK_BATCH: usize = 64;

/// The checked state one operation starts from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemTopology {
    /// Decoded base root, when the caller supplied one.
    pub base: Option<FilesystemRoot>,
    /// Base inode table, when the caller supplied a base root.
    pub table: Option<InodeTable>,
}

impl FilesystemTopology {
    /// Loads and checks the base root the caller named, if any.
    pub fn load(
        reader: &dyn AuthenticatedObjects,
        base: Option<FilesystemRootId>,
        scope: InodeScope,
        root_serial: u64,
    ) -> ContentResult<Self> {
        let Some(base) = base else {
            return Ok(Self {
                base: None,
                table: None,
            });
        };
        let canonical = reader.read_canonical(base.0)?;
        let root = FilesystemRoot::decode(&canonical)?;
        if root.scope() != scope {
            return Err(ContentError::ScopeMismatch {
                what: "filesystem root scope",
            });
        }
        if root.root_inode().serial() != root_serial {
            return Err(ContentError::InvalidRecord("filesystem root serial"));
        }
        Ok(Self {
            base: Some(root),
            table: Some(InodeTable {
                root: root.inode_table(),
                root_serial,
            }),
        })
    }

    /// The table a lookup should use for base records.
    pub fn table(self) -> InodeTable {
        self.table.unwrap_or(InodeTable {
            root: ObjectId::from_bytes(&[0; 32]).expect("zero identity"),
            root_serial: 0,
        })
    }
}

/// One checked operation input, bound to the base it addresses.
pub struct CheckedInput<'a> {
    /// The validated request.
    pub input: &'a FilesystemInput<'a>,
    /// Checked base state.
    pub topology: FilesystemTopology,
    /// Bindings each child serial gains inside this operation.
    pub additions: BTreeMap<u64, u64>,
}

/// Checks membership, identity use and effective topology before any mutation.
pub fn check<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a FilesystemInput<'a>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedInput<'a>> {
    check_inner(reader, input, unreachable, work, true)
}

/// The internal fresh build does not consume zero-count `additions` rows.
pub(crate) fn check_for_fresh_run<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a FilesystemInput<'a>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedInput<'a>> {
    check_inner(reader, input, unreachable, work, false)
}

fn check_inner<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a FilesystemInput<'a>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    include_zero_additions: bool,
) -> ContentResult<CheckedInput<'a>> {
    input.check()?;
    let topology = FilesystemTopology::load(reader, input.base, input.scope, input.root_serial)?;
    let mut additions: BTreeMap<u64, u64> = BTreeMap::new();
    // Children with a stored record that this batch binds exactly once, by the
    // parent that binds them: the final pass decides whether that binding is the
    // one the base already has or a second parent.
    let mut by_parent: BTreeMap<u64, Vec<Vec<u8>>> = BTreeMap::new();
    // The allocator precondition is checked before anything else: a serial the
    // caller calls new must not already exist in the base it addresses.
    let allocation_before = work.inode_pages_read;
    check_new_identities(reader, input, topology, work)?;
    charge_site(work, allocation_before, |sites| &mut sites.allocation);
    // Every serial this loop will demand is known before it runs: the parents it
    // must classify and every child it binds. One grouped demand answers them all,
    // and the loop below then reads the memo — the same verdicts in the same
    // order, with one descent instead of one per binding.
    let mut state = ValidationState::new();
    if let Some(table) = topology.table {
        let mut demanded: Vec<u64> = Vec::new();
        for update in input.directories {
            if update.parent != input.root_serial && !input.new_inodes.contains(&update.parent) {
                demanded.push(update.parent);
            }
            for (_, binding) in &update.changes {
                if let Some(child) = binding {
                    if *child != input.root_serial {
                        demanded.push(*child);
                    }
                }
            }
        }
        let prefetch_before = work.inode_pages_read;
        state.prefetch(reader, table, demanded, work)?;
        charge_site(work, prefetch_before, |sites| &mut sites.prefetch);
    }
    let bindings_before = work.inode_pages_read;
    for update in input.directories {
        if update.parent == input.root_serial && input.base.is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if update.parent == input.root_serial {
            // A root directory update is legal; the root's own count stays zero.
        } else if input.new_inodes.contains(&update.parent) {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = input
                .value_for(update.parent)
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else if let Some(table) = topology.table {
            let record = state.lookup_one(reader, table, update.parent, work)?;
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else {
            let value = input
                .value_for(update.parent)
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        }
        for (name, binding) in &update.changes {
            let _ = name;
            let Some(child) = binding else {
                continue;
            };
            if *child == input.root_serial {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
            // The kind comes from the stored record for an existing inode and
            // from the caller's typed value for one this operation allocates.
            let stored = match topology.table {
                Some(table) => state.lookup_optional(reader, table, *child, work)?,
                None => None,
            };
            let previous = stored.or_else(|| input.value_for(*child));
            let previous = previous.ok_or(ContentError::InvalidRecord("binding kind"))?;
            // Only a regular file may carry several bindings, so a file that
            // already has one keeps gaining them.
            let kind = stored.map_or(previous.kind, |record| record.kind);
            if kind == InodeKind::RegularFile {
                continue;
            }
            // A same-batch duplicate is refused here, before any base record is
            // consulted: a directory or a symlink has one binding outside the
            // root and the batch already names it twice.
            let added = additions.entry(*child).or_insert(0);
            *added = added.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            if *added > 1 {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
            // The batch names it once; a stored record may already own that one
            // binding, and the base name decides whether this is the same
            // binding or a second parent. The decision needs a base listing, so
            // it is deferred to one final pass over the children that have a
            // stored record.
            if stored.is_some() {
                by_parent
                    .entry(update.parent)
                    .or_default()
                    .push(name.as_bytes().to_vec());
            }
        }
    }
    charge_site(work, bindings_before, |sites| &mut sites.bindings);
    let aliases_before = work.inode_pages_read;
    check_parent_aliases(
        reader,
        input,
        &topology,
        &by_parent,
        unreachable,
        work,
        &mut state,
    )?;
    charge_site(work, aliases_before, |sites| &mut sites.aliases);
    if include_zero_additions {
        for update in input.directories {
            for (_, binding) in &update.changes {
                if let Some(child) = binding {
                    let _ = additions.entry(*child).or_insert(0);
                }
            }
        }
    }
    check_root_invariants(input)?;
    if input.base.is_none() {
        for update in input.inodes {
            if update.value.kind == InodeKind::Directory
                && input.update_for(update.serial).is_none()
            {
                return Err(ContentError::InvalidRecord("directory bindings missing"));
            }
        }
    }
    let checked = CheckedInput {
        input,
        topology,
        additions,
    };
    check_effective_cycles(reader, &checked, unreachable, work, &mut state)?;
    Ok(checked)
}

/// Refuses a stored non-file inode that keeps its base binding and gains another.
///
/// A directory or a symlink has exactly one parent. A batch that names one
/// again may only restate the binding the base already has, or move it after
/// unbinding the old name in the same final-state batch. One listing per
/// candidate parent answers "which name does the base bind it under", bounded by
/// the same page ceiling as every other directory read.
fn check_parent_aliases(
    reader: &dyn AuthenticatedObjects,
    input: &FilesystemInput<'_>,
    topology: &FilesystemTopology,
    by_parent: &BTreeMap<u64, Vec<Vec<u8>>>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    // A directory the same batch drops is not part of the result, so the bindings
    // it states are not bindings the result has to satisfy.
    let by_parent: BTreeMap<u64, Vec<Vec<u8>>> = by_parent
        .iter()
        .filter(|(parent, _)| !unreachable.contains_key(parent))
        .map(|(&parent, names)| (parent, names.clone()))
        .collect();
    if by_parent.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    // The candidate children this batch binds under a new name.
    let mut candidates: BTreeMap<u64, ()> = BTreeMap::new();
    for (parent, names) in &by_parent {
        for name in names {
            if let Some(child) = single_binding(input, parent, name) {
                candidates.insert(child, ());
            }
        }
    }
    let Some(root) = topology.base else {
        return Ok(());
    };
    // The one binding a non-file child has may sit in a directory this batch
    // never names, so the walk covers the base tree once, bounded by the same
    // entry ceiling as every other whole-tree check.
    let mut bound: BTreeMap<u64, Vec<(Vec<u8>, u64)>> = BTreeMap::new();
    let mut pending = vec![root.root_inode().serial()];
    let mut visited = 0_usize;
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    while let Some(parent) = pending.pop() {
        if !seen.insert(parent) {
            continue;
        }
        let record = state.lookup_one(reader, table, parent, work)?;
        let mut after = None;
        loop {
            let mut directory = DirectoryReadWork::default();
            let page = list_after(
                reader,
                DirectoryRoot(record.content_root),
                after.as_ref(),
                64,
                crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                &mut directory,
            )?;
            charge_directory(work, directory);
            visited = visited.saturating_add(page.entries.len());
            work.entries_examined = work
                .entries_examined
                .saturating_add(page.entries.len() as u64);
            if visited > MAXIMUM_CYCLE_CHECK_ENTRIES {
                return Err(ContentError::InvalidRecord("cycle check work limit"));
            }
            for (key, serial) in page.entries {
                // Only a name this batch leaves alone is still bound by the
                // base; a name it restates or drops is the caller's own edit. The
                // binding it states is this operation's edge either way, so the
                // walk follows it and it counts against the same ceiling.
                let restated = by_parent.get(&parent).is_some_and(|names| {
                    names.iter().any(|name| name.as_slice() == key.as_bytes())
                });
                if restated {
                    for name in by_parent.get(&parent).into_iter().flatten() {
                        let Some(child) = single_binding(input, &parent, name) else {
                            continue;
                        };
                        visited = visited.saturating_add(1);
                        if visited > MAXIMUM_CYCLE_CHECK_ENTRIES {
                            return Err(ContentError::InvalidRecord("cycle check work limit"));
                        }
                        work.entries_examined = work.entries_examined.saturating_add(1);
                        if state
                            .lookup_optional(reader, table, child, work)?
                            .is_some_and(|value| value.kind == InodeKind::Directory)
                        {
                            pending.push(child);
                        }
                    }
                    continue;
                }
                if state
                    .lookup_optional(reader, table, serial, work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    pending.push(serial);
                }
                if candidates.contains_key(&serial) {
                    bound
                        .entry(serial)
                        .or_default()
                        .push((key.as_bytes().to_vec(), parent));
                }
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
    }
    for (parent, names) in &by_parent {
        for name in names {
            let Some(child) = single_binding(input, parent, name) else {
                continue;
            };
            let Some(base) = bound.get(&child) else {
                continue;
            };
            let legal = base.iter().any(|(base_name, base_parent)| {
                // The batch restates the binding the base has, so nothing moves.
                (base_parent == parent && base_name.as_slice() == name.as_slice())
                    || !base_binding_survives(input, *base_parent, base_name, child)
            });
            if !legal {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
        }
    }
    Ok(())
}

/// The child one `(parent, name)` pair in this batch binds.
fn single_binding(input: &FilesystemInput<'_>, parent: &u64, name: &[u8]) -> Option<u64> {
    input.update_for(*parent).and_then(|update| {
        update
            .changes
            .iter()
            .find(|(changed, _)| changed.as_bytes() == name)
            .and_then(|(_, binding)| *binding)
    })
}

/// True when the base still binds `child` under `base_name` in `base_parent`.
fn base_binding_survives(
    input: &FilesystemInput<'_>,
    base_parent: u64,
    base_name: &[u8],
    child: u64,
) -> bool {
    match input.update_for(base_parent) {
        Some(update) => match update
            .changes
            .iter()
            .find(|(changed, _)| changed.as_bytes() == base_name)
        {
            Some((_, binding)) => *binding == Some(child),
            None => true,
        },
        None => true,
    }
}

/// The base records one validation reads, memoized across its three walks.
///
/// Validation demands base inode records in three passes — the decision loop, the
/// alias walk and the effective-cycle walk — and the passes overlap: a serial the
/// first pass read is demanded again by the second and third. The memo answers a
/// repeated demand **without a read while still charging the demand**, so
/// `inode_demands` (the work-limit charge) is unchanged and only the I/O moves.
///
/// **Absence is memoized too, and the demand charge is what stays bit-identical.**
/// This row's batches bind serials they are allocating, so those serials are
/// absent from the base by construction; measured on this row, 27,436 of
/// validation's 27,662 inode-page reads were absent-serial descents, and half of
/// them were the identical descent bought twice — once by the binding loop and
/// once by the cycle walk. Remembering absence removes the repeated read and keeps
/// the *charge*: an absent memo hit charges one demand exactly as the descent it
/// replaces charged one (`lookup_many` charges a demand per serial it answers at a
/// leaf), so `inode_demands` and `objects_read` are unchanged while the pages and
/// waves fall.
///
/// **The memo is sound because the base is immutable for its lifetime.** It lives
/// for one `check` call, and `FilesystemTopology::load` binds one base root that
/// every site reads through the same `InodeTable`, so a serial absent once is
/// absent for the whole call. The memo is bounded by the serials the operation
/// actually demands — one entry per demanded serial, in one of the two maps —
/// which is the same bound the lazy path's own demand set has.
struct ValidationState {
    records: BTreeMap<u64, InodeValue>,
    absent: BTreeSet<u64>,
}

impl ValidationState {
    fn new() -> Self {
        Self {
            records: BTreeMap::new(),
            absent: BTreeSet::new(),
        }
    }

    /// Reads every not-yet-known serial in one grouped demand.
    ///
    /// The batch pays the pages and waves it reads; the **demand** charge is paid
    /// where the demand is made ([`Self::lookup_optional`]), so `inode_demands`
    /// stays the number of logical demands rather than the batch size.
    fn prefetch(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: impl IntoIterator<Item = u64>,
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        let mut missing: Vec<u64> = serials
            .into_iter()
            .filter(|serial| !self.records.contains_key(serial))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() {
            return Ok(());
        }
        let mut inode = InodeReadWork::default();
        let found = lookup_many(reader, table, &missing, &mut inode)?;
        work.read_waves = work.read_waves.saturating_add(inode.read_waves);
        work.inode_pages_read = work.inode_pages_read.saturating_add(inode.pages_read);
        for (serial, value) in missing.into_iter().zip(found) {
            match value {
                Some(value) => {
                    self.records.insert(serial, value);
                }
                // The grouped demand has already paid for this answer, in the same
                // waves the found serials came back in; recording it is what stops
                // the binding loop and the cycle walk buying it again.
                None => {
                    self.absent.insert(serial);
                }
            }
        }
        Ok(())
    }

    /// One base record, answered from the memo when it is known.
    fn lookup_optional(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<InodeValue>> {
        if let Some(value) = self.records.get(&serial) {
            // A memoized record was found by an earlier demand, so its charge is
            // the same one `charge_inode` makes for a found serial: one demand.
            work.objects_read = work.objects_read.saturating_add(1);
            work.inode_demands = work.inode_demands.saturating_add(1);
            return Ok(Some(*value));
        }
        if self.absent.contains(&serial) {
            // The same charge a descent would have made for a serial the base does
            // not hold (`lookup_many` charges one demand per serial answered at a
            // leaf, absence included), so the physical read is what this removes
            // and the accounting is what it keeps.
            work.objects_read = work.objects_read.saturating_add(1);
            work.inode_demands = work.inode_demands.saturating_add(1);
            return Ok(None);
        }
        let mut inode = InodeReadWork::default();
        let found = lookup_many(reader, table, &[serial], &mut inode)?;
        charge_inode(work, inode);
        let value = found.into_iter().next().flatten();
        match value {
            Some(value) => {
                self.records.insert(serial, value);
            }
            None => {
                self.absent.insert(serial);
            }
        }
        Ok(value)
    }

    /// One base record that must exist.
    fn lookup_one(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<InodeValue> {
        self.lookup_optional(reader, table, serial, work)?
            .ok_or(ContentError::InvalidRecord("missing base inode"))
    }
}

/// Charges one inode lookup's work.
fn charge_inode(work: &mut ValidationWork, inode: InodeReadWork) {
    work.objects_read = work.objects_read.saturating_add(inode.demands);
    work.read_waves = work.read_waves.saturating_add(inode.read_waves);
    work.inode_demands = work.inode_demands.saturating_add(inode.demands);
    work.inode_pages_read = work.inode_pages_read.saturating_add(inode.pages_read);
}

/// Charges one directory listing's work.
fn charge_directory(work: &mut ValidationWork, directory: DirectoryReadWork) {
    work.objects_read = work.objects_read.saturating_add(directory.pages_read);
    work.read_waves = work.read_waves.saturating_add(directory.read_waves);
    work.directory_pages_read = work
        .directory_pages_read
        .saturating_add(directory.pages_read);
}

fn check_new_identities(
    reader: &dyn AuthenticatedObjects,
    input: &FilesystemInput<'_>,
    topology: FilesystemTopology,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    if input.new_inodes.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    for wave in input.new_inodes.chunks(ALLOCATION_CHECK_BATCH) {
        let mut inode = InodeReadWork::default();
        let found = lookup_many(reader, table, wave, &mut inode)?;
        charge_inode(work, inode);
        if found.iter().any(Option::is_some) {
            return Err(ContentError::InvalidRecord("reused inode serial"));
        }
    }
    Ok(())
}

fn check_root_invariants(input: &FilesystemInput<'_>) -> ContentResult<()> {
    if input.base.is_none() {
        let value = input
            .value_for(input.root_serial)
            .ok_or(ContentError::InvalidRecord("root inode value"))?;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::InvalidRecord("root inode kind"));
        }
    }
    for update in input.directories {
        for (_, binding) in &update.changes {
            if *binding == Some(input.root_serial) {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
        }
    }
    if input.base.is_some() && input.new_inodes.contains(&input.root_serial) {
        return Err(ContentError::InvalidRecord("root inode allocation"));
    }
    Ok(())
}

/// Walks a rebound directory's effective subtree looking for its new parent.
///
/// A cycle can only be formed by binding a directory below itself, so only the
/// directories this operation rebinds need the walk. The work is bounded by
/// [`MAXIMUM_CYCLE_CHECK_ENTRIES`]; exceeding it is an explicit refusal, never a
/// claim that the tree was proven acyclic. A directory this operation allocates
/// has no stored page yet, so the walk of it follows this operation's own
/// bindings instead.
fn check_effective_cycles(
    reader: &dyn AuthenticatedObjects,
    checked: &CheckedInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    if checked.input.base.is_none() {
        let before = work.inode_pages_read;
        let result = check_build_reachability(reader, checked, unreachable, work);
        charge_site(work, before, |sites| &mut sites.reachability);
        return result;
    }
    let cycles_before = work.inode_pages_read;
    let table = checked.topology.table;
    for update in checked.input.directories {
        for (_, binding) in &update.changes {
            let Some(child) = binding else {
                continue;
            };
            let stored = match table {
                Some(table) => state.lookup_optional(reader, table, *child, work)?,
                None => None,
            };
            let kind = match stored {
                Some(record) => record.kind,
                None => match checked.input.value_for(*child) {
                    Some(value) => value.kind,
                    None => continue,
                },
            };
            if kind != InodeKind::Directory {
                continue;
            }
            // A serial with no stored record has no base page to list: the walk
            // of it follows this operation's own bindings alone.
            let seed = stored.map(|record| record.content_root);
            // The walk follows the *effective* bindings: a directory this
            // operation rebinds still holds every name it already had plus the
            // changes, so a cycle formed by two changes is visible here.
            let mut pending = vec![(seed, *child)];
            let mut visited = 0_usize;
            let mut seen: BTreeSet<u64> = BTreeSet::new();
            while let Some((base_root, serial)) = pending.pop() {
                if !seen.insert(serial) {
                    continue;
                }
                let changes = checked
                    .input
                    .update_for(serial)
                    .map(|update| update.changes.as_slice())
                    .unwrap_or(&[]);
                let entries = match base_root {
                    Some(content_root) => {
                        effective_entries(reader, content_root, changes, &mut visited, work)?
                    }
                    None => effective_entries_without_base(changes),
                };
                for (_, entry_serial) in entries {
                    if entry_serial == update.parent || entry_serial == *child {
                        return Err(ContentError::InvalidRecord("effective tree cycle"));
                    }
                    let entering = match table {
                        Some(table) => state.lookup_optional(reader, table, entry_serial, work)?,
                        None => None,
                    };
                    if let Some(entry) = entering {
                        if entry.kind == InodeKind::Directory {
                            pending.push((Some(entry.content_root), entry_serial));
                        }
                    }
                }
            }
        }
    }
    charge_site(work, cycles_before, |sites| &mut sites.cycles);
    Ok(())
}

/// Proves that a build's stated bindings form a tree rooted at the root.
///
/// A build has no stored page to list, so its obligation is the one the root
/// implies: every directory this operation allocates is reachable from the root
/// exactly once, which is what makes a disconnected cycle - two directories
/// binding each other with nothing binding either of them - a refusal. The walk
/// is bounded by the caller's supplied bindings, with no independent count cap.
fn check_build_reachability(
    _reader: &dyn AuthenticatedObjects,
    checked: &CheckedInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    // A build states its own bindings: the walk reads no base object, and the
    // entries it counts are the ones the caller supplied. A directory the same
    // batch drops is not part of the result, so walking it would charge work the
    // operation does not do - and could refuse a build for a subtree it never
    // builds.
    // The root is reached by definition: it is the walk's own starting point.
    let declared: BTreeSet<u64> = checked
        .input
        .new_inodes
        .iter()
        .filter(|serial| **serial != checked.input.root_serial)
        .filter(|serial| !unreachable.contains_key(serial))
        .filter(|serial| {
            checked
                .input
                .value_for(**serial)
                .is_some_and(|value| value.kind == InodeKind::Directory)
        })
        .copied()
        .collect();
    let mut edges: BTreeMap<u64, u32> = declared.iter().map(|serial| (*serial, 0)).collect();
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    let mut pending = vec![checked.input.root_serial];
    while let Some(serial) = pending.pop() {
        if !seen.insert(serial) {
            continue;
        }
        if unreachable.contains_key(&serial) {
            continue;
        }
        for child in checked
            .input
            .update_for(serial)
            .into_iter()
            .flat_map(|update| update.changes.iter().filter_map(|(_, binding)| *binding))
        {
            work.entries_examined = work.entries_examined.saturating_add(1);
            // A binding is an edge into the child. Only a directory has to be
            // reached exactly once: a regular file may be bound several times.
            if let Some(count) = edges.get_mut(&child) {
                *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                if *count > 1 {
                    return Err(ContentError::InvalidRecord("multiple parents"));
                }
                pending.push(child);
            }
        }
    }
    // Every directory this operation allocates is held by a binding the root
    // reaches; one that is not is either disconnected or inside a cycle.
    for serial in declared {
        if edges.get(&serial).copied().unwrap_or(0) == 0 {
            return Err(ContentError::InvalidRecord("effective tree cycle"));
        }
    }
    Ok(())
}

/// This operation's own bindings for a directory that has no stored page yet.
fn effective_entries_without_base(
    changes: &[(crate::filesystem::path::PathName, Option<u64>)],
) -> Vec<(crate::filesystem::path::PathName, u64)> {
    changes
        .iter()
        .filter_map(|(name, binding)| binding.map(|serial| (name.clone(), serial)))
        .collect()
}

/// Base entries of one directory with this operation's changes applied.
fn effective_entries(
    reader: &dyn AuthenticatedObjects,
    content_root: ObjectId,
    changes: &[(crate::filesystem::path::PathName, Option<u64>)],
    visited: &mut usize,
    work: &mut ValidationWork,
) -> ContentResult<Vec<(crate::filesystem::path::PathName, u64)>> {
    let mut base = Vec::new();
    let mut after = None;
    loop {
        let mut directory = DirectoryReadWork::default();
        let page = list_after(
            reader,
            DirectoryRoot(content_root),
            after.as_ref(),
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            &mut directory,
        )?;
        charge_directory(work, directory);
        base.extend(page.entries.iter().cloned());
        *visited = visited.saturating_add(page.entries.len());
        work.entries_examined = work
            .entries_examined
            .saturating_add(page.entries.len() as u64);
        if *visited > MAXIMUM_CYCLE_CHECK_ENTRIES {
            return Err(ContentError::InvalidRecord("cycle check work limit"));
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let mut merged = Vec::with_capacity(base.len() + changes.len());
    let mut index = 0_usize;
    for (name, serial) in base {
        while index < changes.len() && changes[index].0 < name {
            if let Some(binding) = changes[index].1 {
                merged.push((changes[index].0.clone(), binding));
            }
            index += 1;
        }
        if index < changes.len() && changes[index].0 == name {
            if let Some(binding) = changes[index].1 {
                merged.push((name, binding));
            }
            index += 1;
        } else {
            merged.push((name, serial));
        }
    }
    while index < changes.len() {
        if let Some(binding) = changes[index].1 {
            merged.push((changes[index].0.clone(), binding));
        }
        index += 1;
    }
    Ok(merged)
}
