//! Checked inputs: membership, identity, topology and retained bindings.
//!
//! These checks run before any promise is made about the result. They are
//! incremental: an update to a valid base validates the affected bindings,
//! identities and effective parents, and reads a base directory only where a
//! stored directory left its base parent and a placement could land beneath
//! it. Authentication
//! of object bytes is not topology validation and neither is a caller
//! assertion: new identities are rechecked against the base they claim to be
//! absent from, and every final count is derived from checked retained bindings.
//!
//! One algorithm serves every route; only its containers differ. Without a
//! backing the input is resident by declaration, so the batch's base records
//! are one grouped demand and the topology evidence lives in maps held under the
//! caller's ordering budget. With backed serial state the same classification
//! runs in bounded windows and the same evidence is indexed records, so nothing
//! resident grows with the base or with the whole change and no total is
//! refused.
//!
//! A directory or symlink bound twice is not decided here in general: the
//! reducer derives every touched inode's final count and refuses a non-file
//! count above one. Only a directory placed twice inside one batch is refused
//! before any object is offered.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::DirectoryReadWork;
use crate::filesystem::identity::InodeScope;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::view::{OperationInput, ResidentInput};
use crate::filesystem::rows::{check_operation_input, PreparedRows};
use crate::filesystem::state::SerialState;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

mod backed;
mod cycles;
mod entries;
mod in_place;
mod incremental;

/// Work one operation's validation performed.
///
/// Validation reads the base it is about to change: the parent records, the
/// base names of the stored directories it binds and the entries of every
/// directory a territory walk lists. Those reads are part of the operation's
/// work and are reported with it, exactly like the merge's own reads.
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
    /// Base directory entries a territory walk read.
    pub entries_examined: u64,
    /// Inode pages read, split by the call site that asked for them.
    ///
    /// `inode_pages_read` is one total, and a total cannot say which of the
    /// checks read the table: a grouped demand charges one wave per *level*
    /// (`inode/read.rs`), a single-serial descent charges one wave *and* one page
    /// per level, and several regions can make either call. Each is charged here
    /// as the growth of `inode_pages_read` across its own region, so the sites
    /// sum to the total exactly. `FilesystemTopology::load` reads the base root
    /// once per operation and charges no counter at all, so it has no site here
    /// rather than a site of zero.
    pub inode_pages_by_site: ValidationReadSites,
    /// Directory placements classification recorded.
    pub placements: u64,
    /// Upward steps the rooted proof took, over both of its walks.
    pub ancestry_steps: u64,
    /// Directories a territory walk listed, each moved directory included.
    pub territory_directories: u64,
    /// Effective entries a territory walk examined.
    pub territory_entries: u64,
    /// Most rows one classification window held: headers and bound names.
    /// Without a backing the whole input is one window.
    pub peak_window_rows: u64,
    /// Stored parents whose change rows the in-place pass read, each once.
    pub in_place_scans: u64,
    /// Change rows the in-place pass read over those parents.
    pub in_place_rows: u64,
    /// Placed stored directories found renamed inside their own base parent.
    pub in_place_directories: u64,
}

/// Where one validation's inode pages were read.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationReadSites {
    /// The allocator precondition, one grouped demand per 64 new serials.
    pub allocation: u64,
    /// The grouped demand of each classification window.
    pub prefetch: u64,
    /// The classification's own record lookups.
    pub bindings: u64,
    /// Always zero: the derived count replaced the parent-alias pass.
    pub aliases: u64,
    /// The territory walks of an update's moved directories.
    pub cycles: u64,
    /// Always zero: a base-less operation's proof reads no inode page.
    pub reachability: u64,
}

/// Charges the inode pages read since `before` to one site.
///
/// The site counters are read from the same total the operation already
/// maintains, so an instrument added here cannot invent a read: the sites sum
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

/// Resident validation state derived from the caller's ordering-memory budget.
///
/// Two allowances come out of the same declared bytes, at the two rates that
/// budget already charges, and both apply only when no backing holds the state.
/// Declared totals - the row counts, the changed names and the grouped demand -
/// are bounded by the per-serial allowance the count array and the
/// touched-serial collection use, because they bound the same per-serial
/// quantities and an operation whose count array fits its budget must not refuse
/// in validation first. Resident containers - the base-record memo and each
/// topology map - keep the conservative per-entry rate, because those hold real
/// decoded records. Neither bounds the work a walk examines.
fn resident_limit(input: &dyn OperationInput) -> usize {
    usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX)
}

/// The declared-total allowance: one ordering slot per serial, the same figure
/// as `FilesystemResources::maximum_touched_serials`.
fn declared_limit(input: &dyn OperationInput) -> usize {
    input.resources().maximum_touched_serials()
}
/// Serial demands one allocator-precondition wave may make.
pub const ALLOCATION_CHECK_BATCH: usize = 64;
/// Rows one backed classification window holds: one per header and one per
/// bound name. One window's names, at most this many of at most 255 bytes, are
/// all the classification keeps resident on the backed route.
pub const CLASSIFICATION_WINDOW_ROWS: usize = 64;
/// Base records the memo holds on the backed route: one window's parents and
/// children, never a figure derived from the change.
const BACKED_MEMO_RECORDS: usize = 2 * CLASSIFICATION_WINDOW_ROWS;

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
}

/// Checks membership, identity use and effective topology before any mutation.
///
/// No dropped set is supplied: a directory this operation allocates and nothing
/// binds is recognised by its missing placement.
pub fn check<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedRows,
    work: &mut ValidationWork,
) -> ContentResult<CheckedInput<'a>> {
    let view = ResidentInput::new(input);
    let topology = check_operation(reader, &view, &SerialState::new(None), work)?;
    Ok(CheckedInput { input, topology })
}

/// Checks one operation and returns the base state it was checked against.
pub(crate) fn check_operation(
    reader: &dyn AuthenticatedObjects,
    input: &dyn OperationInput,
    serial: &SerialState<'_>,
    work: &mut ValidationWork,
) -> ContentResult<FilesystemTopology> {
    check_operation_input(input)?;
    // A backed operation holds no container sized by these totals, so it
    // declares no ceiling on them.
    let resident = !serial.backed();
    let declared = declared_limit(input);
    let rows = input
        .directory_rows()
        .max(input.inode_rows())
        .max(input.new_rows());
    if resident && rows > declared {
        return Err(ContentError::ObjectLimitExceeded {
            limit: declared,
            actual: rows,
        });
    }
    let topology =
        FilesystemTopology::load(reader, input.base(), input.scope(), input.root_serial())?;
    // The allocator precondition is checked before anything else: a serial the
    // caller calls new must not already exist in the base it addresses.
    let allocation_before = work.inode_pages_read;
    check_new_identities(reader, input, topology, work)?;
    charge_site(work, allocation_before, |sites| &mut sites.allocation);
    let mut state = ValidationState::new(if resident {
        resident_limit(input)
    } else {
        BACKED_MEMO_RECORDS
    });
    let classification_before = work.inode_pages_read;
    let mut prefetched = 0;
    if let (true, Some(table)) = (resident, topology.table) {
        // The input is resident, so every serial classification will demand is
        // known before it runs: the parents it must classify and every child it
        // binds. One grouped demand answers them all, and the windows below then
        // read the memo with one descent instead of one per window.
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
        state.prefetch(reader, table, demanded, work)?;
        prefetched = work.inode_pages_read.saturating_sub(classification_before);
    }
    let mut records = backed::TopologyRecords::new(serial, resident_limit(input));
    let mut classifier =
        incremental::Classifier::new(reader, input, topology, &mut state, &mut records, work);
    let mut rows = input.directories()?;
    let mut names = 0usize;
    while let Some(update) = rows.next_row()? {
        names =
            names.saturating_add(usize::try_from(update.header.change_rows).unwrap_or(usize::MAX));
        if resident && names > declared {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared,
                actual: names,
            });
        }
        classifier.row(incremental::WindowRow {
            parent: update.header.parent,
            bound: None,
        })?;
        for change in update.changes()? {
            let (name, binding) = change?;
            if let Some(child) = binding {
                classifier.row(incremental::WindowRow {
                    parent: update.header.parent,
                    bound: Some((name, child)),
                })?;
            }
        }
    }
    drop(rows);
    classifier.finish()?;
    let (classified, window_pages) = (classifier.rows, classifier.prefetched_pages);
    drop(classifier);
    if resident {
        // One grouped demand covered every row, whatever the processing chunks.
        work.peak_window_rows = classified;
    }
    let prefetched = prefetched.saturating_add(window_pages);
    let bindings = work
        .inode_pages_read
        .saturating_sub(classification_before)
        .saturating_sub(prefetched);
    work.inode_pages_by_site.prefetch =
        work.inode_pages_by_site.prefetch.saturating_add(prefetched);
    work.inode_pages_by_site.bindings = work.inode_pages_by_site.bindings.saturating_add(bindings);
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
    cycles::check_effective_tree(reader, input, &topology, &mut records, work)?;
    Ok(topology)
}

/// Memoizes authenticated base records and absence within a bounded window:
/// the resident allowance without a backing, one classification window's
/// records with one. Eviction changes physical reads, never the logical demand
/// charge.
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
            .filter(|serial| !self.records.contains_key(serial) && !self.absent.contains(serial))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() {
            return Ok(());
        }
        // A demand that fits is kept whole: room is made once, before its
        // answers arrive, so the window that asked reads every one of them back.
        if self.records.len() + self.absent.len() + missing.len() > self.limit {
            self.records.clear();
            self.absent.clear();
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
                // classification buying it again.
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
