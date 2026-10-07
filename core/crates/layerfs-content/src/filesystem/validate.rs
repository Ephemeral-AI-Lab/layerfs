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
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::view::{DirectoryRow, OperationInput, ResidentInput};
use crate::filesystem::rows::{check_operation_input, DirectoryChangeLookup, PreparedRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::state::DroppedParents;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

mod cycles;
mod entries;

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

/// Bounded validation work derived from the caller's ordering-memory budget.
///
/// Two allowances come out of the same declared bytes, at the two rates that
/// budget already charges. Declared totals — the row counts, the changed names
/// and the grouped prefetch demand — are bounded by the per-serial allowance
/// the count array and the touched-serial collection use, because they bound
/// the same per-serial quantities and an operation whose count array fits its
/// budget must not refuse in validation first. Resident state — the base-record
/// memo and the entries an effective-tree walk examines — keeps the
/// conservative per-entry rate, because those hold real decoded records and
/// examined work.
fn walk_limit(input: &dyn OperationInput) -> usize {
    usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX)
}

/// The declared-total allowance: one ordering slot per serial, the same figure
/// as `FilesystemResources::maximum_touched_serials`.
fn declared_limit(input: &dyn OperationInput) -> usize {
    input.resources().maximum_touched_serials()
}
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
    /// The validated row source.
    pub input: &'a dyn PreparedRows,
    /// Checked base state.
    pub topology: FilesystemTopology,
    /// Bindings each child serial gains inside this operation.
    pub additions: BTreeMap<u64, u64>,
}

/// Checks membership, identity use and effective topology before any mutation.
pub fn check<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedInput<'a>> {
    let view = ResidentInput::new(input);
    let checked = check_operation(reader, &view, unreachable, work)?;
    Ok(CheckedInput {
        input,
        topology: checked.topology,
        additions: checked.additions,
    })
}

pub(crate) struct CheckedOperationInput<'a> {
    pub input: &'a dyn OperationInput,
    pub topology: FilesystemTopology,
    pub additions: BTreeMap<u64, u64>,
}

pub(crate) fn check_operation<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn OperationInput,
    unreachable: &dyn DroppedParents,
    work: &mut ValidationWork,
) -> ContentResult<CheckedOperationInput<'a>> {
    check_operation_input(input)?;
    let declared = declared_limit(input);
    let rows = input
        .directory_rows()
        .max(input.inode_rows())
        .max(input.new_rows());
    if rows > declared {
        return Err(ContentError::ObjectLimitExceeded {
            limit: declared,
            actual: rows,
        });
    }
    let topology =
        FilesystemTopology::load(reader, input.base(), input.scope(), input.root_serial())?;
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
    let mut state = ValidationState::new(walk_limit(input));
    if let Some(table) = topology.table {
        let mut demanded: Vec<u64> = Vec::new();
        let mut rows = input.directories()?;
        let mut names = 0usize;
        while let Some(update) = rows.next_row()? {
            names = names
                .saturating_add(usize::try_from(update.header.change_rows).unwrap_or(usize::MAX));
            if names > declared {
                return Err(ContentError::ObjectLimitExceeded {
                    limit: declared,
                    actual: names,
                });
            }
            if update.header.parent != input.root_serial() && !input.is_new(update.header.parent)? {
                demanded.push(update.header.parent);
            }
            for change in update.changes()? {
                let (_, binding) = change?;
                if let Some(child) = binding {
                    if child != input.root_serial() {
                        demanded.push(child);
                    }
                }
            }
        }
        if demanded.len() > declared.saturating_mul(2) {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared.saturating_mul(2),
                actual: demanded.len(),
            });
        }
        drop(rows);
        let prefetch_before = work.inode_pages_read;
        state.prefetch(reader, table, demanded, work)?;
        charge_site(work, prefetch_before, |sites| &mut sites.prefetch);
    }
    let bindings_before = work.inode_pages_read;
    let mut rows = input.directories()?;
    let mut names = 0usize;
    while let Some(update) = rows.next_row()? {
        names =
            names.saturating_add(usize::try_from(update.header.change_rows).unwrap_or(usize::MAX));
        if names > declared {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared,
                actual: names,
            });
        }
        if update.header.parent == input.root_serial() && input.base().is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if update.header.parent == input.root_serial() {
            // A root directory update is legal; the root's own count stays zero.
        } else if input.is_new(update.header.parent)? {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = input
                .value_for(update.header.parent)?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else if let Some(table) = topology.table {
            let record = state.lookup_one(reader, table, update.header.parent, work)?;
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else {
            let value = input
                .value_for(update.header.parent)?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        }
        for change in update.changes()? {
            let (name, binding) = change?;
            let _ = name;
            let Some(child) = binding else {
                continue;
            };
            if child == input.root_serial() {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
            // The kind comes from the stored record for an existing inode and
            // from the caller's typed value for one this operation allocates.
            let stored = match topology.table {
                Some(table) => state.lookup_optional(reader, table, child, work)?,
                None => None,
            };
            let previous = match stored {
                Some(record) => Some(record),
                None => input.value_for(child)?,
            };
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
            let added = additions.entry(child).or_insert(0);
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
                    .entry(update.header.parent)
                    .or_default()
                    .push(name.as_bytes().to_vec());
            }
        }
    }
    drop(rows);
    charge_site(work, bindings_before, |sites| &mut sites.bindings);
    let aliases_before = work.inode_pages_read;
    let aliases = check_parent_aliases(
        reader,
        input,
        &topology,
        &by_parent,
        unreachable,
        work,
        &mut state,
    );
    input.clear_binding_points();
    aliases?;
    charge_site(work, aliases_before, |sites| &mut sites.aliases);
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        for change in update.changes()? {
            let (_, binding) = change?;
            if let Some(child) = binding {
                let _ = additions.entry(child).or_insert(0);
            }
        }
    }
    drop(rows);
    check_root_invariants(input)?;
    if input.base().is_none() {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if update.value.kind == InodeKind::Directory
                && input.directory_for(update.serial)?.is_none()
            {
                return Err(ContentError::InvalidRecord("directory bindings missing"));
            }
        }
    }
    let checked = CheckedOperationInput {
        input,
        topology,
        additions,
    };
    cycles::check_effective_cycles(reader, &checked, unreachable, work, &mut state)?;
    Ok(checked)
}

/// Refuses a stored non-file inode that keeps its base binding and gains another.
fn check_parent_aliases(
    reader: &dyn AuthenticatedObjects,
    input: &dyn OperationInput,
    topology: &FilesystemTopology,
    by_parent: &BTreeMap<u64, Vec<Vec<u8>>>,
    unreachable: &dyn DroppedParents,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    // A directory the same batch drops is not part of the result, so the bindings
    // it states are not bindings the result has to satisfy.
    let mut retained = BTreeMap::new();
    for (&parent, names) in by_parent {
        if !unreachable.contains(parent)? {
            retained.insert(parent, names.clone());
        }
    }
    let by_parent = retained;
    if by_parent.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    // The candidate children this batch binds under a new name.
    let mut candidates: BTreeMap<u64, ()> = BTreeMap::new();
    for (parent, names) in &by_parent {
        let update = input.directory_for(*parent)?;
        for name in names {
            let child = match &update {
                Some(row) => binding_in(row, name)?,
                None => None,
            };
            if let Some(child) = child {
                candidates.insert(child, ());
            }
        }
    }
    let Some(root) = topology.base else {
        return Ok(());
    };
    input.prepare_binding_points()?;
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
        let update = if by_parent.contains_key(&parent) {
            input.directory_for(parent)?
        } else {
            None
        };
        let restated: BTreeSet<&[u8]> = by_parent
            .get(&parent)
            .into_iter()
            .flatten()
            .map(Vec::as_slice)
            .collect();
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
            if visited > walk_limit(input) {
                return Err(ContentError::InvalidRecord("cycle check work limit"));
            }
            for (key, serial) in page.entries {
                // Only a name this batch leaves alone is still bound by the
                // base; a name it restates or drops is the caller's own edit. The
                // binding it states is this operation's edge either way, so the
                // walk follows it and it counts against the same ceiling.
                if restated.contains(key.as_bytes()) {
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
        // Follow each changed binding once, including names absent from the
        // base. Repeating the whole changed row for every restated base name
        // used to turn an N-name permutation into N² charged visits.
        for name in by_parent.get(&parent).into_iter().flatten() {
            let child = match &update {
                Some(row) => binding_in(row, name)?,
                None => None,
            };
            let Some(child) = child else {
                continue;
            };
            visited = visited.saturating_add(1);
            if visited > walk_limit(input) {
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
    }
    for (parent, names) in &by_parent {
        let update = input.directory_for(*parent)?;
        for name in names {
            let child = match &update {
                Some(row) => binding_in(row, name)?,
                None => None,
            };
            let Some(child) = child else {
                continue;
            };
            let Some(base) = bound.get(&child) else {
                continue;
            };
            let mut legal = false;
            for (base_name, base_parent) in base {
                // The batch restates the binding the base has, so nothing moves.
                if base_parent == parent && base_name.as_slice() == name.as_slice() {
                    legal = true;
                    break;
                }
                if !base_binding_survives(input, *base_parent, base_name, child)? {
                    legal = true;
                    break;
                }
            }
            if !legal {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
        }
    }
    Ok(())
}

/// One exact changed-name point from the selected operation input.
fn binding_in(row: &DirectoryRow<'_>, name: &[u8]) -> ContentResult<Option<u64>> {
    Ok(
        match row.binding_for(&crate::filesystem::PathName::from_bytes(name)?)? {
            DirectoryChangeLookup::Bound(serial) => Some(serial),
            DirectoryChangeLookup::Removed | DirectoryChangeLookup::Unchanged => None,
        },
    )
}

fn base_binding_survives(
    input: &dyn OperationInput,
    base_parent: u64,
    base_name: &[u8],
    child: u64,
) -> ContentResult<bool> {
    Ok(
        match input.binding_change(
            base_parent,
            &crate::filesystem::PathName::from_bytes(base_name)?,
        )? {
            DirectoryChangeLookup::Unchanged => true,
            DirectoryChangeLookup::Removed => false,
            DirectoryChangeLookup::Bound(serial) => serial == child,
        },
    )
}

/// Memoizes authenticated base records and absence within a resource-sized
/// window. Eviction changes physical reads, never the logical demand charge.
struct ValidationState {
    records: BTreeMap<u64, InodeValue>,
    absent: BTreeSet<u64>,
    limit: usize,
}

impl ValidationState {
    fn new(limit: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            absent: BTreeSet::new(),
            limit: limit.max(1),
        }
    }

    fn make_room(&mut self) {
        if self.records.len() + self.absent.len() >= self.limit {
            self.records.clear();
            self.absent.clear();
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
            self.make_room();
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
        self.make_room();
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
    input: &dyn OperationInput,
    topology: FilesystemTopology,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    if input.new_rows() == 0 {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    // One declared wave of serials is read at a time: the batch the allocator
    // precondition already charged, and no more.
    let mut waves = input.new_inodes()?;
    loop {
        let mut wave = Vec::with_capacity(ALLOCATION_CHECK_BATCH);
        while wave.len() < ALLOCATION_CHECK_BATCH {
            match waves.next_row()? {
                Some(serial) => wave.push(serial),
                None => break,
            }
        }
        if wave.is_empty() {
            break;
        }
        let mut inode = InodeReadWork::default();
        let found = lookup_many(reader, table, &wave, &mut inode)?;
        charge_inode(work, inode);
        if found.iter().any(Option::is_some) {
            return Err(ContentError::InvalidRecord("reused inode serial"));
        }
    }
    Ok(())
}

fn check_root_invariants(input: &dyn OperationInput) -> ContentResult<()> {
    let root_serial = input.root_serial();
    if input.base().is_none() {
        let value = input
            .value_for(root_serial)?
            .ok_or(ContentError::InvalidRecord("root inode value"))?;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::InvalidRecord("root inode kind"));
        }
    }
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        for change in update.changes()? {
            let (_, binding) = change?;
            if binding == Some(root_serial) {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
        }
    }
    drop(rows);
    if input.base().is_some() && input.is_new(root_serial)? {
        return Err(ContentError::InvalidRecord("root inode allocation"));
    }
    Ok(())
}
