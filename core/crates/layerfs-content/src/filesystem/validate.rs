//! Checked inputs: membership, identity, topology and retained bindings.
//!
//! These checks run before any promise is made about the result. They are
//! incremental where the input allows it: an update to a valid base validates the
//! affected bindings, identities and effective parents, while a structure that can
//! form a cycle is walked with a declared bounded work limit. Authentication of
//! object bytes is not topology validation and neither is a caller assertion: new
//! identities are rechecked against the base they claim to be absent from, and
//! every final count is derived from checked retained bindings.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::DirectoryReadWork;
use crate::filesystem::identity::InodeScope;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::{
    check_binding_input, CompatibilityBindingRows, DirectoryHeader, PreparedBindingRows,
    PreparedRows,
};
use crate::filesystem::state::{
    BindingClaimState, BindingClaims, BindingSiteState, BindingSites, ResidentClaims, SiteScope,
    StateScope, StateSelection, StateTable,
};
use crate::object::inode_leaf::InodeKind;
use crate::object::{AuthenticatedObjects, ObjectId};

mod aliases;
mod binding;
mod cycles;
mod effective;
mod facts;
mod prefetch;
mod site_aliases;

pub use prefetch::ValidationPrefetchWork;

use crate::filesystem::path::PathName;
use aliases::BindingSite;
use binding::{selected, Bindings, Headers};
use facts::ValidationState;

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
    /// Actual bounded initial-demand production and lookup attempts.
    pub prefetch: ValidationPrefetchWork,
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
fn walk_limit<I: PreparedRows + ?Sized>(input: &I) -> usize {
    usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX)
}

/// The declared-total allowance: one ordering slot per serial, the same figure
/// as `FilesystemResources::maximum_touched_serials`.
fn declared_limit<I: PreparedRows + ?Sized>(input: &I) -> usize {
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
pub struct CheckedInput<'a, I: ?Sized = dyn PreparedRows + 'a> {
    /// The validated row source.
    pub input: &'a I,
    /// Checked base state.
    pub topology: FilesystemTopology,
    /// Bindings each child serial gains inside this operation.
    pub additions: BTreeMap<u64, u64>,
}

/// Checked scalar bindings, retaining the exact addressed source.
pub type CheckedBindingInput<'a> = CheckedInput<'a, dyn PreparedBindingRows + 'a>;

/// Checks a legacy whole-directory source through its explicit compatibility view.
pub fn check<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedInput<'a>> {
    let bindings = CompatibilityBindingRows::new(input)?;
    let checked = check_bindings(reader, &bindings, unreachable, work)?;
    Ok(CheckedInput {
        input,
        topology: checked.topology,
        additions: checked.additions,
    })
}

/// Checked source and topology without a resident additions result.
pub struct CheckedTopologyInput<'a> {
    /// Exact addressed scalar source.
    pub input: &'a dyn PreparedBindingRows,
    /// Checked base state.
    pub topology: FilesystemTopology,
}

/// Checks scalar inputs using explicitly selected resident legacy claims/results.
pub fn check_bindings<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedBindingInput<'a>> {
    let mut additions = BTreeMap::new();
    let checked = check_resident(reader, input, unreachable, work, Some(&mut additions))?;
    Ok(CheckedInput {
        input,
        topology: checked.topology,
        additions,
    })
}

pub(crate) fn check_resident_topology<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<CheckedTopologyInput<'a>> {
    check_resident(reader, input, unreachable, work, None)
}

fn check_resident<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    additions: Option<&mut BTreeMap<u64, u64>>,
) -> ContentResult<CheckedTopologyInput<'a>> {
    check_binding_input(input)?;
    let declared = claim_shape(input)?;
    let mut selector = blake3::Hasher::new();
    selector.update(b"layerfs/binding-claims/resident-selection/v1\0");
    selector.update(input.scope().object().as_bytes());
    selector.update(&input.root_serial().to_be_bytes());
    let mut selection = StateSelection::issue(*selector.finalize().as_bytes())?;
    let mut binding = blake3::Hasher::new();
    binding.update(b"layerfs/binding-claims/resident-owner/v1\0");
    binding.update(selection.selector());
    binding.update(&selection.token().to_be_bytes());
    selection.bind_owner(*binding.finalize().as_bytes())?;
    let scope = StateScope::new(selection, 1, StateTable::BindingClaims)?;
    let mut state = ResidentClaims::new(scope.clone(), declared)?;
    let mut claims = BindingClaims::new(&mut state, scope, declared)?;
    let checked = check_semantics(reader, input, unreachable, work, &mut claims, additions)?;
    claims.finish()?;
    Ok(checked)
}

/// Checks all scalar semantics, verifies the exact claim seal and retires it.
/// No additions population is built; DirectoryRoots may start after success.
pub fn check_with_claims<'a, S: BindingClaimState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scope: &StateScope,
) -> ContentResult<CheckedTopologyInput<'a>> {
    let outcome = (|| {
        check_binding_input(input)?;
        let declared = claim_shape(input)?;
        let mut claims = BindingClaims::new(state, scope.clone(), declared)?;
        let checked = check_semantics(reader, input, unreachable, work, &mut claims, None)?;
        claims.finish()?;
        Ok(checked)
    })();
    if outcome.is_err() {
        let _ = state.claim_abandon(scope);
    }
    outcome
}

/// Validates source-bound exclusive sites, facts, exact final seal and retirement.
/// All errors terminalize this selected attempt once without native cleanup.
pub fn check_with_sites<'a, S: BindingSiteState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scope: &SiteScope,
) -> ContentResult<CheckedTopologyInput<'a>> {
    select_site_input(input, scope)?;
    check_sites_selected(reader, input, unreachable, work, state, scope)
}

pub(crate) fn select_site_input(
    input: &dyn PreparedBindingRows,
    scope: &SiteScope,
) -> ContentResult<()> {
    // An unavailable/foreign source has not selected this owner. Preserve that
    // error without consuming another live source's attempt.
    if input.binding_source_id()? != scope.source_id() {
        return Err(ContentError::InvalidOrderingRecord("site input source"));
    }
    Ok(())
}

pub(crate) fn check_sites_selected<'a, S: BindingSiteState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scope: &SiteScope,
) -> ContentResult<CheckedTopologyInput<'a>> {
    let outcome = (|| {
        check_binding_input(input)?;
        let declared = claim_shape(input)?;
        let mut sites = BindingSites::new(state, scope.clone(), declared)?;
        let (topology, mut facts) = check_binding_semantics(
            reader,
            input,
            work,
            |child, header, ordinal, has_base, _name| {
                sites.claim(
                    child,
                    header,
                    ordinal,
                    has_base,
                    !unreachable.contains_key(&header.parent()),
                )
            },
        )?;
        let mut sites = sites.close_membership()?;
        let members = sites.members().clone();
        let active_stored = sites.active_stored();
        let aliases_before = work.inode_pages_read;
        let outcome = site_aliases::check(
            site_aliases::SiteAliasInput {
                reader,
                input,
                topology: &topology,
                members: &members,
                active_stored,
                unreachable,
            },
            work,
            &mut facts,
            sites.state(),
        );
        charge_site(work, aliases_before, |sites| &mut sites.aliases);
        outcome?;
        // Preserve the ordinary alias verdict before root/cycle decisions. A
        // later semantic failure still abandons the already retired attempt.
        sites.finish(input, unreachable, &members)?;
        let checked =
            check_remaining(reader, input, topology, unreachable, work, &mut facts, None)?;
        Ok(checked)
    })();
    if outcome.is_err() {
        let _ = state.site_abandon(scope);
    }
    outcome
}

fn claim_shape(input: &dyn PreparedBindingRows) -> ContentResult<usize> {
    let mut headers = Headers::new(input)?;
    let mut count = 0usize;
    while let Some(header) = headers.next()? {
        count = count
            .checked_add(header.binding_count() as usize)
            .ok_or(ContentError::LengthOverflow)?;
    }
    Ok(count)
}

fn check_semantics<'a, S: BindingClaimState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    claims: &mut BindingClaims<'_, S>,
    mut additions: Option<&mut BTreeMap<u64, u64>>,
) -> ContentResult<CheckedTopologyInput<'a>> {
    let mut sites: BTreeMap<u64, BindingSite> = BTreeMap::new();
    let (topology, mut state) = check_binding_semantics(
        reader,
        input,
        work,
        |child, header, _ordinal, has_base, name| {
            claims.claim(child)?;
            if let Some(additions) = additions.as_deref_mut() {
                additions.insert(child, 1);
            }
            if has_base {
                sites.insert(
                    child,
                    BindingSite {
                        parent: header.parent(),
                        name,
                    },
                );
            }
            Ok(())
        },
    )?;
    let aliases_before = work.inode_pages_read;
    let outcome = aliases::check(
        reader,
        input,
        &topology,
        &sites,
        unreachable,
        work,
        &mut state,
    );
    charge_site(work, aliases_before, |sites| &mut sites.aliases);
    outcome?;
    check_remaining(
        reader,
        input,
        topology,
        unreachable,
        work,
        &mut state,
        additions,
    )
}

fn check_binding_semantics(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    work: &mut ValidationWork,
    mut claim: impl FnMut(u64, &DirectoryHeader, u32, bool, PathName) -> ContentResult<()>,
) -> ContentResult<(FilesystemTopology, ValidationState)> {
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
    // The allocator precondition is checked before anything else: a serial the
    // caller calls new must not already exist in the base it addresses.
    let allocation_before = work.inode_pages_read;
    check_new_identities(reader, input, topology, work)?;
    charge_site(work, allocation_before, |sites| &mut sites.allocation);
    // Count and finish the selected scalar source before the first prefetch grouped base
    // demand. Replay it through one fixed raw64 producer; no demand union lives.
    let mut state = ValidationState::new(walk_limit(input));
    if let Some(table) = topology.table {
        let prefetch_before = work.inode_pages_read;
        let outcome = prefetch::read(reader, input, table, &mut state, declared, work);
        charge_site(work, prefetch_before, |sites| &mut sites.prefetch);
        outcome?;
    }
    let bindings_before = work.inode_pages_read;
    let mut rows = Headers::new(input)?;
    let mut names = 0usize;
    while let Some(header) = rows.next()? {
        names = names.saturating_add(header.binding_count() as usize);
        if names > declared {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared,
                actual: names,
            });
        }
        if header.parent() == input.root_serial() && input.base().is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if header.parent() == input.root_serial() {
            // A root directory update is legal; the root's own count stays zero.
        } else if input.is_new(header.parent())? {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = input
                .value_for(header.parent())?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else if let Some(table) = topology.table {
            let record = state.lookup_one(reader, table, header.parent(), work)?;
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else {
            let value = input
                .value_for(header.parent())?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        }
        let mut bindings = Bindings::new(input, header)?;
        let mut ordinal = 0_u32;
        while let Some((name, binding)) = bindings.next()? {
            let selected_ordinal = ordinal;
            ordinal = ordinal.checked_add(1).ok_or(ContentError::LengthOverflow)?;
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
            // A same-batch duplicate is refused here, before the base-listing
            // alias pass: a directory or a symlink has one binding outside the
            // root and the batch already names it twice.
            claim(child, &header, selected_ordinal, stored.is_some(), name)?;
        }
    }
    drop(rows);
    charge_site(work, bindings_before, |sites| &mut sites.bindings);
    Ok((topology, state))
}

fn check_remaining<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    topology: FilesystemTopology,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
    additions: Option<&mut BTreeMap<u64, u64>>,
) -> ContentResult<CheckedTopologyInput<'a>> {
    // Only the old public result owns this population. The bounded route has
    // neither a regular-file marker nor a second map of exclusive claims.
    if let Some(additions) = additions {
        let mut rows = Headers::new(input)?;
        while let Some(header) = rows.next()? {
            let mut bindings = Bindings::new(input, header)?;
            while let Some((_, binding)) = bindings.next()? {
                if let Some(child) = binding {
                    additions.entry(child).or_insert(0);
                }
            }
        }
    }
    check_root_invariants(input)?;
    if input.base().is_none() {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if update.value.kind == InodeKind::Directory
                && selected(input, update.serial)?.is_none()
            {
                return Err(ContentError::InvalidRecord("directory bindings missing"));
            }
        }
    }
    let checked = CheckedTopologyInput { input, topology };
    cycles::check_effective_cycles(reader, &checked, unreachable, work, state)?;
    Ok(checked)
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

fn check_new_identities<I: PreparedRows + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &I,
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

fn check_root_invariants(input: &dyn PreparedBindingRows) -> ContentResult<()> {
    let root_serial = input.root_serial();
    if input.base().is_none() {
        let value = input
            .value_for(root_serial)?
            .ok_or(ContentError::InvalidRecord("root inode value"))?;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::InvalidRecord("root inode kind"));
        }
    }
    let mut rows = Headers::new(input)?;
    while let Some(header) = rows.next()? {
        let mut bindings = Bindings::new(input, header)?;
        while let Some((_, binding)) = bindings.next()? {
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
