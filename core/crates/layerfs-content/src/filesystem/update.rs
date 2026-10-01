//! One complete filesystem operation: validation, directories, references, inodes.
//!
//! The operation consumes checked final-state inputs and produces one canonical
//! filesystem root. Directory bindings are merged by the sorted engine, which
//! reports every original/final binding it actually saw; those edges drive the
//! reference reducer, which derives each final inode value. Inodes whose derived
//! count reached zero are traversed in bounded pages after every addition has been
//! accounted, and the inode table is rebuilt from typed values in one sorted pass.
//! Completion here is this operation's own result; acknowledged persistence is the
//! consumer's.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer, ReferenceWork};
use crate::filesystem::references::release::ReleaseWork;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::sorted::SortedWork;
use crate::filesystem::state::{DirectoryRoots, IndexedState, StateScope};
use crate::filesystem::validate::{self, FilesystemTopology, ValidationWork};
use crate::object::inode_leaf::InodeValue;
use crate::object::AuthenticatedObjects;

/// Work one complete filesystem operation performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FilesystemUpdateCounters {
    /// Canonical object reads and writes.
    pub objects: ObjectWork,
    /// Sorted directory work.
    pub directories: SortedWork,
    /// Sorted inode work.
    pub inodes: SortedWork,
    /// Reference reduction work.
    pub references: ReferenceWork,
    /// Released-descendant traversal work.
    pub release: ReleaseWork,
    /// Directory bindings observed as additions.
    pub bindings_added: u64,
    /// Directory bindings observed as removals.
    pub bindings_removed: u64,
    /// Directory merges this operation performed.
    pub directory_updates: u64,
    /// Base inode records read while deriving final counts.
    pub base_records_read: u64,
    /// Validation work: the reads, pages and entries the checks performed.
    pub validation: ValidationWork,
}

/// One completed filesystem operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemResult {
    /// Identity of the new canonical filesystem root.
    pub root: FilesystemRootId,
    /// The decoded new root.
    pub value: FilesystemRoot,
    /// Work performed.
    pub counters: FilesystemUpdateCounters,
}

mod canonical;
mod canonical_body;
mod compatibility;
mod construction;
mod graph;
mod namespace;
mod reduction;
mod sites;
use crate::filesystem::state::{EligibilityAuthority, ParentCalls};

pub use canonical::{
    build_filesystem_binding_rows_with_canonical_state,
    update_filesystem_binding_rows_with_canonical_state,
};
pub use canonical_body::canonical_directory_working_bytes;
pub use compatibility::{
    build_filesystem, build_filesystem_binding_rows_with_state, build_filesystem_timed,
    build_filesystem_with_state, build_filesystem_with_state_timed, update_filesystem,
    update_filesystem_binding_rows_with_state, update_filesystem_timed,
    update_filesystem_with_state, update_filesystem_with_state_timed,
};
pub use construction::{
    build_filesystem_binding_rows_with_construction_state,
    update_filesystem_binding_rows_with_construction_state,
};
pub use graph::{
    build_filesystem_binding_rows_with_alias_graph_state,
    build_filesystem_binding_rows_with_graph_state,
    update_filesystem_binding_rows_with_alias_graph_state,
    update_filesystem_binding_rows_with_graph_state,
};
pub use namespace::{
    build_filesystem_binding_rows_with_namespace_state,
    update_filesystem_binding_rows_with_namespace_state,
};
pub use reduction::canonical_reduction_working_bytes;
pub use sites::{
    build_filesystem_binding_rows_with_site_state, update_filesystem_binding_rows_with_site_state,
};

fn run_binding_state<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&'b mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    input.resources().check()?;
    let mut contents = DirectoryRoots::new(state, scope.clone(), input.directory_rows())?;
    let mut backing = backing;
    // The flag is set by the body the moment the checked completion runs, so the
    // failure path below never releases the same backing twice and never depends
    // on recognising a particular error label.
    let mut cleanup_attempted = false;
    let outcome = {
        let borrowed: Option<&mut (dyn OrderingBacking + 'b)> = backing.as_deref_mut();
        run_body(
            objects,
            input,
            borrowed,
            phases,
            &mut cleanup_attempted,
            &mut contents,
            state,
        )
    };
    match outcome {
        Ok(result) => Ok(result),
        Err(error) => {
            // One attempted operation: known-owned ordering resources are released
            // here, once, and the original failure is what the caller sees. A
            // cleanup that also fails is visible through the backing, never by
            // replacing the operation's own error.
            if !cleanup_attempted {
                if let Some(backing) = backing {
                    let _ = backing.release();
                }
            }
            let _ = contents.release(state);
            Err(error)
        }
    }
}

fn run_body<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut (dyn OrderingBacking + 'b)>,
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
    contents: &mut DirectoryRoots,
    state: &mut dyn IndexedState,
) -> ContentResult<FilesystemResult> {
    // A directory this batch leaves with no binding is dead on arrival: its
    // parent already accounted the binding it lost, so there is no final count to
    // hold it, no page worth building, and no subtree to walk.
    let unreachable = unreachable_parents(input)?;
    let mut validation = ValidationWork::default();
    let checked = phases.phase("validate", || {
        validate::check_resident_topology(objects.reader(), input, &unreachable, &mut validation)
    })?;
    let validated = ValidatedInput {
        topology: checked.topology,
        validation,
        unreachable: EligibilityAuthority::Legacy(unreachable),
    };
    run_canonical_body(
        objects,
        input,
        backing,
        phases,
        cleanup_attempted,
        contents,
        validated,
        state,
        ParentCalls::compatibility(),
    )
}

struct ValidatedInput {
    topology: FilesystemTopology,
    validation: ValidationWork,
    unreachable: EligibilityAuthority,
}

#[allow(clippy::too_many_arguments)]
fn run_canonical_body<'b, S: IndexedState + ?Sized>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut (dyn OrderingBacking + 'b)>,
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
    contents: &mut DirectoryRoots,
    validated: ValidatedInput,
    state: &mut S,
    parents: ParentCalls<S>,
) -> ContentResult<FilesystemResult> {
    let resources = input.resources();
    let mut reducer = ReferenceReducer::new(
        resources.maximum_pending_records,
        backing,
        resources.merge_buffer_bytes,
        resources.ordering_bytes,
    );
    canonical_body::run_selected_body(
        objects,
        input,
        phases,
        cleanup_attempted,
        contents,
        validated,
        state,
        parents,
        &mut reducer,
    )
}

/// Directory parents this operation leaves with no binding at all.
///
/// A serial this operation allocates and this operation never binds cannot end
/// with a final count, so there is neither a parent to hold it nor a page worth
/// building: its bindings are accounted by the walk that dropped it. Only a
/// declared-new parent can be in that state - an existing directory that is not
/// rebound keeps the record it already has.
fn unreachable_parents(input: &dyn PreparedBindingRows) -> ContentResult<BTreeMap<u64, ()>> {
    let limit = usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX);
    let root = input.root_serial();
    // The retained membership is the targeted set itself: one entry per
    // declared-new parent other than the root, because only such a parent can
    // end the operation with no binding at all. Each entry is charged to the
    // ordering ceiling as the scratch it is; unrelated child bindings are never
    // retained, so a wide directory does not multiply this charge.
    let mut parents: BTreeMap<u64, bool> = BTreeMap::new();
    let mut rows = input.directory_headers()?;
    while let Some(update) = rows.next_header()? {
        let parent = update.parent();
        if parent == root || parents.contains_key(&parent) {
            continue;
        }
        if !input.is_new(parent)? {
            continue;
        }
        parents.insert(parent, false);
        if parents.len() > limit {
            return Err(ContentError::ObjectLimitExceeded {
                limit,
                actual: parents.len(),
            });
        }
    }
    drop(rows);
    // One binding pass marks the retained parents some row binds. The join
    // holds no second set: a child outside the targeted set costs one map
    // probe and nothing more, so the charge above is the whole working set.
    // An empty binding list is the "keep the bindings you have" form, and a
    // directory this operation allocates has none to keep.
    let mut rows = input.directory_headers()?;
    while let Some(update) = rows.next_header()? {
        let mut bindings = input.bindings(&update)?;
        while let Some((_, binding)) = bindings.next_binding()? {
            if let Some(child) = binding {
                if let Some(bound) = parents.get_mut(&child) {
                    *bound = true;
                }
            }
        }
        if !bindings.finish()?.matches(&update) {
            return Err(ContentError::InvalidRecord("directory completion"));
        }
    }
    drop(rows);
    Ok(parents
        .into_iter()
        .filter(|(_, bound)| !*bound)
        .map(|(parent, _)| (parent, ()))
        .collect())
}

fn lookup_base(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serial: u64,
) -> ContentResult<Option<InodeValue>> {
    Ok(
        lookup_many(reader, table, &[serial], &mut InodeReadWork::default())?
            .into_iter()
            .next()
            .flatten(),
    )
}

/// Serials whose derived final count is zero, with the base records read.
#[allow(clippy::too_many_arguments)]
fn zero_count_serials<S: ?Sized>(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    reducer: &mut ReferenceReducer<'_, '_>,
    base_batch: usize,
    root_serial: u64,
    unreachable: &EligibilityAuthority,
    maximum_serials: usize,
    state: &mut S,
    parents: ParentCalls<S>,
) -> ContentResult<(Vec<u64>, u64)> {
    let touched = reducer.touched_serials(base_batch)?;
    if touched.len() > maximum_serials {
        // The collection is one `u64` per touched inode and belongs to the same
        // declared ordering budget as the rows themselves, so it is refused
        // rather than allocated past the caller's ceiling.
        return Err(ContentError::ObjectLimitExceeded {
            limit: maximum_serials,
            actual: touched.len(),
        });
    }
    reducer.note_serials_scanned(touched.len() as u64);
    let mut zero = Vec::new();
    let mut reads = 0_u64;
    for wave in touched.chunks(base_batch.max(1)) {
        let serials = wave.iter().map(|(serial, _)| *serial).collect::<Vec<_>>();
        let bases = lookup_many(reader, table, &serials, &mut InodeReadWork::default())?;
        reads = reads.saturating_add(wave.len() as u64);
        for (index, (serial, carried)) in wave.iter().enumerate() {
            let base = bases[index];
            // The state travels with the serial: `touched_serials` already read
            // every row, so re-asking the reducer here would re-find each one.
            let count = match carried {
                PendingState::New { count, .. } => *count,
                PendingState::Existing { delta, .. } => {
                    let base_count = base.map_or(0, |value| value.namespace_ref_count as i128);
                    u64::try_from((base_count + i128::from(*delta)).max(0)).unwrap_or(0)
                }
            };
            // A directory this batch drops is not a released inode: it was never
            // part of the result, so there is nothing to traverse.
            if count == 0
                && *serial != root_serial
                && !unreachable.contains(state, *serial, parents)?
            {
                zero.push(*serial);
            }
        }
    }
    Ok((zero, reads))
}
