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

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::update::apply_bindings;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::inode::update::apply_inode_values;
use crate::filesystem::objects::{
    FilesystemObjects, FilesystemPhases, ObjectWork, MAXIMUM_READ_DEMANDS,
};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer, ReferenceWork};
use crate::filesystem::references::release::{release_zero_count, ReleaseWork};
use crate::filesystem::references::OperationReducer;
use crate::filesystem::root::{profile_id, FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::view::{OperationInput, ResidentInput, StreamedInput};
use crate::filesystem::rows::{check_operation_input, PreparedDirectoryStreams, PreparedRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::sorted::SortedWork;
use crate::filesystem::state::{DroppedParents, InitialRows, RebuiltRoots, SerialState};
use crate::filesystem::validate::{self, ValidationWork};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, FinalizedObject, ObjectRole};
use crate::IndexedConstructionBacking;

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

/// Builds a new filesystem from strictly sorted final bindings.
pub fn build_filesystem(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(
        objects,
        &ResidentInput::new(input),
        backing,
        &FilesystemPhases::disabled(),
    )
}

/// Builds a new filesystem while recording the caller's coarse phase scopes.
pub fn build_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, &ResidentInput::new(input), backing, phases)
}

/// Applies one complete update to a checked immutable base root.
pub fn update_filesystem(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(
        objects,
        &ResidentInput::new(input),
        backing,
        &FilesystemPhases::disabled(),
    )
}

/// Applies one complete update while recording the caller's coarse phase scopes.
pub fn update_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, &ResidentInput::new(input), backing, phases)
}

/// Builds from stable streamed directory headers/changes without resident rows.
pub fn build_filesystem_streamed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    build_filesystem_streamed_timed(objects, input, backing, &FilesystemPhases::disabled())
}

/// Streamed initial construction with the same coarse phase scopes.
pub fn build_filesystem_streamed_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, &StreamedInput::new(input), backing, phases)
}

/// Applies stable streamed changes through the existing canonical driver.
pub fn update_filesystem_streamed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    update_filesystem_streamed_timed(objects, input, backing, &FilesystemPhases::disabled())
}

/// Streamed update with the same coarse phase scopes; no alternate validator.
pub fn update_filesystem_streamed_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, &StreamedInput::new(input), backing, phases)
}

/// Builds with serial state in the caller's distinct filesystem attempt scope.
/// The record port is the existing neutral protocol; no scope is acquired or
/// released here. Remaining validation/reducer/resource limits still apply.
pub fn build_filesystem_streamed_backed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    records: &mut dyn IndexedConstructionBacking,
    ordering: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    build_filesystem_streamed_backed_timed(
        objects,
        input,
        records,
        ordering,
        &FilesystemPhases::disabled(),
    )
}

/// Backed serial-state initial construction with the existing phase scopes.
pub fn build_filesystem_streamed_backed_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    records: &mut dyn IndexedConstructionBacking,
    ordering: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run_state(
        objects,
        &StreamedInput::new(input),
        ordering,
        Some(records),
        phases,
    )
}

/// Updates through the canonical driver with backed serial construction state.
/// The caller retains the first original record failure and fences actual
/// consumers before releasing its explicit operation/construction scope.
pub fn update_filesystem_streamed_backed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    records: &mut dyn IndexedConstructionBacking,
    ordering: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    update_filesystem_streamed_backed_timed(
        objects,
        input,
        records,
        ordering,
        &FilesystemPhases::disabled(),
    )
}

/// Backed serial-state update with the existing coarse phase scopes.
pub fn update_filesystem_streamed_backed_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,
    records: &mut dyn IndexedConstructionBacking,
    ordering: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run_state(
        objects,
        &StreamedInput::new(input),
        ordering,
        Some(records),
        phases,
    )
}

fn run(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn OperationInput,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    run_state(objects, input, backing, None, phases)
}

fn run_state<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn OperationInput,
    backing: Option<&'b mut dyn OrderingBacking>,
    state_backing: Option<&mut dyn IndexedConstructionBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let state = SerialState::new(state_backing);
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
            &state,
            phases,
            &mut cleanup_attempted,
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
            Err(error)
        }
    }
}

fn run_body<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn OperationInput,
    backing: Option<&mut (dyn OrderingBacking + 'b)>,
    state: &SerialState<'_>,
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
) -> ContentResult<FilesystemResult> {
    // A directory this batch leaves with no binding is dead on arrival: its
    // parent already accounted the binding it lost, so there is no final count to
    // hold it, no page worth building, and no subtree to walk.
    if state.backed() {
        check_operation_input(input)?;
    }
    state.begin(input)?;
    unreachable_parents(input, state)?;
    let dropped = state.dropped_view(input);
    let mut validation = ValidationWork::default();
    let checked = phases.phase("validate", || {
        validate::check_operation(objects.reader(), input, &dropped, &mut validation)
    })?;
    let reader = objects.reader();
    let mut counters = FilesystemUpdateCounters {
        validation,
        ..FilesystemUpdateCounters::default()
    };
    let table = checked.topology.table();
    let base_table = checked.topology.base.map(|root| root.inode_table());
    // A build already supplies sorted new serials and every final typed value.
    // One count per declared serial avoids ordering runs and their lookups.
    let resources = input.resources();
    let initial = base_table.is_none();
    if initial {
        if !state.backed() && input.new_rows() > resources.maximum_touched_serials() {
            return Err(ContentError::ObjectLimitExceeded {
                limit: resources.maximum_touched_serials(),
                actual: input.new_rows(),
            });
        }
        state.initial_counts(input)?;
    }
    let mut reducer = OperationReducer::new(state, input, &dropped, backing);
    if !initial {
        reducer.check_backing_capacity()?;
        register_values(&mut reducer, input, &dropped)?;
    }
    let batch = resources.base_read_batch.min(MAXIMUM_READ_DEMANDS);
    let mut contents = RebuiltRoots::new(state);
    let mut retained_parents = Vec::new();
    let mut retained_bases = Vec::new();
    phases.phase("directories", || -> ContentResult<()> {
        // Validation already proved parent ordering and uniqueness. Only one
        // final batch is retained for reuse after every binding effect is known.
        // One declared batch of rows is resident at a time: the window is the
        // caller's own base-read batch, so the resident cost of reading the
        // prepared namespace does not grow with the number of rows in it.
        let mut waves = input.directories()?;
        loop {
            let mut updates = Vec::with_capacity(batch);
            while updates.len() < batch {
                match waves.next_row()? {
                    Some(row) => updates.push(row),
                    None => break,
                }
            }
            if updates.is_empty() {
                break;
            }
            let mut parents = Vec::with_capacity(updates.len());
            for update in &updates {
                if checked.topology.table.is_some()
                    && !dropped.contains(update.header.parent)?
                    && !input.is_new(update.header.parent)?
                {
                    parents.push(update.header.parent);
                }
            }
            let bases = lookup_many(reader, table, &parents, &mut InodeReadWork::default())?;
            for update in &updates {
                if dropped.contains(update.header.parent)? {
                    // Nothing binds this directory in the result, so no page of it
                    // is worth building. Its bindings are still this operation's
                    // edges and stay accounted: every final binding of a directory
                    // this operation allocates is an addition.
                    for change in update.changes()? {
                        let (_, binding) = change?;
                        let Some(child) = binding else {
                            continue;
                        };
                        note_retained_binding(
                            &mut reducer,
                            initial.then_some(state),
                            input,
                            child,
                        )?;
                        counters.bindings_added = counters.bindings_added.saturating_add(1);
                    }
                    continue;
                }
                let base = parents
                    .binary_search(&update.header.parent)
                    .ok()
                    .and_then(|index| bases[index]);
                let content_root = if update.header.change_rows == 0 {
                    // An unchanged directory retains its root; a new directory
                    // needs one actual empty page.
                    if input.is_new(update.header.parent)? || checked.topology.table.is_none() {
                        crate::filesystem::directory::update::empty_directory(objects)?.0
                    } else {
                        base.ok_or(ContentError::InvalidRecord("directory parent record"))?
                            .content_root
                    }
                } else {
                    let base_directory = if input.is_new(update.header.parent)? {
                        None
                    } else if checked.topology.table.is_some() {
                        let record =
                            base.ok_or(ContentError::InvalidRecord("directory parent record"))?;
                        if record.kind != InodeKind::Directory {
                            return Err(ContentError::InvalidRecord("directory parent kind"));
                        }
                        Some(DirectoryRoot(record.content_root))
                    } else {
                        None
                    };
                    let mut observe = |before: Option<u64>,
                                       after: Option<u64>|
                     -> ContentResult<()> {
                        if before == after {
                            return Ok(());
                        }
                        // Additions precede removals, so a move never drops an
                        // inode to a spurious zero between its two bindings.
                        if let Some(next) = after {
                            note_retained_binding(
                                &mut reducer,
                                initial.then_some(state),
                                input,
                                next,
                            )?;
                            counters.bindings_added = counters.bindings_added.saturating_add(1);
                        }
                        if let Some(previous) = before {
                            reducer.note_removed_binding(previous)?;
                            counters.bindings_removed = counters.bindings_removed.saturating_add(1);
                        }
                        Ok(())
                    };
                    let (root, work) = apply_bindings(
                        objects,
                        base_directory,
                        update.changes()?,
                        resources.scratch_bytes,
                        &mut observe,
                    )?;
                    counters.directories.pages_read = counters
                        .directories
                        .pages_read
                        .saturating_add(work.pages_read);
                    counters.directories.pages_created = counters
                        .directories
                        .pages_created
                        .saturating_add(work.pages_created);
                    counters.directories.pages_reused = counters
                        .directories
                        .pages_reused
                        .saturating_add(work.pages_reused);
                    counters.directories.change_keys = counters
                        .directories
                        .change_keys
                        .saturating_add(work.change_keys);
                    counters.directories.untouched_subtrees = counters
                        .directories
                        .untouched_subtrees
                        .saturating_add(work.untouched_subtrees);
                    counters.directories.peak_scratch_bytes = counters
                        .directories
                        .peak_scratch_bytes
                        .max(work.peak_scratch_bytes);
                    counters.directory_updates = counters.directory_updates.saturating_add(1);
                    root.0
                };
                contents.insert(update.header.parent, content_root)?;
            }
            if !parents.is_empty() {
                retained_parents = parents;
                retained_bases = bases;
            }
        }
        Ok(())
    })?;
    // Keep the reducer's original all-effects-before-values insertion order:
    // interleaving values with later effects can increase spill quota demands.
    // Reuse the final parent batch; earlier omitted values are read in bounded
    // groups rather than retaining a record for every directory in the input.
    if !initial {
        let mut contents_iter = contents.rows(input)?;
        loop {
            let wave = contents_iter
                .by_ref()
                .take(batch)
                .collect::<ContentResult<Vec<_>>>()?;
            if wave.is_empty() {
                break;
            }
            let mut missing = Vec::with_capacity(wave.len());
            for (serial, _) in &wave {
                if input.value_for(*serial)?.is_none()
                    && retained_parents.binary_search(serial).is_err()
                {
                    missing.push(*serial);
                }
            }
            let bases = lookup_many(reader, table, &missing, &mut InodeReadWork::default())?;
            for (serial, content_root) in &wave {
                let value = match input.value_for(*serial)? {
                    Some(value) => Some(value),
                    None => retained_parents
                        .binary_search(serial)
                        .ok()
                        .and_then(|index| retained_bases[index])
                        .or_else(|| {
                            missing
                                .binary_search(serial)
                                .ok()
                                .and_then(|index| bases[index])
                        }),
                };
                let value = value.ok_or(ContentError::InvalidRecord("directory value missing"))?;
                reducer.note_value(
                    *serial,
                    InodeValue {
                        content_root: *content_root,
                        ..value
                    },
                )?;
            }
        }
    }
    // Every other supplied value keeps the content root the caller named, unless
    // this operation rebuilt that inode's own directory.
    if !initial {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if contents.get(update.serial)?.is_some() || dropped.contains(update.serial)? {
                // A directory this batch drops is not part of the result at all: its
                // value is never a final row, so it must not enter the reduction.
                continue;
            }
            if state.backed() && input.directory_for(update.serial)?.is_some() {
                // A sealed header declared the rebuilt root already inserted
                // above. Losing it cannot turn this supplied value into a
                // metadata-only update or overwrite the constructed root.
                return Err(ContentError::InvalidRecord(
                    "filesystem rebuilt root missing",
                ));
            }
            reducer.note_value(update.serial, update.value)?;
        }
    }
    if checked.topology.table.is_some() {
        // Only an update can release descendants: a new filesystem has no base
        // binding to lose, and its root is never released.
        if state.backed() {
            let (release, reads) = reducer.release_indexed(
                objects,
                table,
                resources.base_read_batch,
                input.root_serial(),
            )?;
            counters.release = release;
            counters.base_records_read = counters.base_records_read.saturating_add(reads);
        } else {
            let memory = reducer.memory().expect("resident reducer");
            let zero = zero_count_serials(
                reader,
                table,
                memory,
                resources.base_read_batch,
                input.root_serial(),
                &dropped,
                resources.maximum_touched_serials(),
            )?;
            counters.base_records_read = counters.base_records_read.saturating_add(zero.1);
            counters.release = release_zero_count(
                reader,
                table,
                memory,
                &zero.0,
                resources.base_read_batch,
                64,
                crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                |reader, serial| {
                    lookup_base(reader, table, serial)?
                        .ok_or(ContentError::InvalidRecord("released inode record"))
                },
            )?;
        }
    }
    let (inode_table, inode_work) = if initial {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if input.new_position(update.serial)?.is_none() {
                return Err(ContentError::InvalidRecord("effect inode record"));
            }
        }
        drop(values);
        let rows = InitialRows::new(input, state, &contents)?;
        let built = phases.phase("inodes", || {
            apply_inode_values(objects, None, rows, resources.scratch_bytes)
        })?;
        counters.references.final_values = input
            .new_rows()
            .checked_sub(state.dropped_count())
            .ok_or(ContentError::InvalidRecord("filesystem dropped count"))?
            as u64;
        built
    } else {
        let mut rows = phases.phase("references", || {
            reducer.finish(
                reader,
                table,
                resources.base_read_batch.max(1),
                input.root_serial(),
            )
        })?;
        let changes = std::iter::from_fn(|| match rows.next_change() {
            Ok(Some(change)) => Some(Ok((change.serial, change.value))),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        });
        let built = phases.phase("inodes", || {
            apply_inode_values(objects, base_table, changes, resources.scratch_bytes)
        })?;
        counters.references = rows.work();
        drop(rows);
        built
    };
    counters.inodes = inode_work;
    // The final stream is done with: its counters are snapshotted, its reader
    // handle is closed, and only then is the backing's completion checked. A
    // cleanup failure fails the operation before any root object exists, so a
    // successful result always means the ordering resources were released.
    *cleanup_attempted = true;
    phases.phase("cleanup", || reducer.release())?;
    let root = match checked.topology.base {
        Some(root) => root.with_inode_table(inode_table),
        None => FilesystemRoot::new(
            profile_id(),
            input.scope(),
            input.root_serial(),
            inode_table,
        )?,
    };
    let id = phases.phase("root.encode", || {
        let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode()?)?
            .with_references(vec![inode_table]);
        objects.emit(object)
    })?;
    counters.objects = objects.work();
    Ok(FilesystemResult {
        root: FilesystemRootId(id),
        value: root,
        counters,
    })
}

/// Directory parents this operation leaves with no binding at all.
///
/// A serial this operation allocates and this operation never binds cannot end
/// with a final count, so there is neither a parent to hold it nor a page worth
/// building: its bindings are accounted by the walk that dropped it. Only a
/// declared-new parent can be in that state - an existing directory that is not
/// rebound keeps the record it already has.
fn unreachable_parents(input: &dyn OperationInput, state: &SerialState<'_>) -> ContentResult<()> {
    let limit = usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX);
    let root = input.root_serial();
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        let parent = update.header.parent;
        if parent == root
            || (!state.backed() && state.parent(parent)?.is_some())
            || !input.is_new(parent)?
        {
            continue;
        }
        state.declare_parent(parent, limit)?;
    }
    drop(rows);
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        for change in update.changes()? {
            let (_, binding) = change?;
            if let Some(child) = binding {
                state.hold_parent(input, child)?;
            }
        }
    }
    drop(rows);
    state.finish_parents();
    Ok(())
}

fn register_values(
    reducer: &mut OperationReducer<'_, '_, '_, '_>,
    input: &dyn OperationInput,
    unreachable: &dyn DroppedParents,
) -> ContentResult<()> {
    let mut serials = input.new_inodes()?;
    while let Some(serial) = serials.next_row()? {
        if unreachable.contains(serial)? {
            // The serial is not part of the result, so it is not a final row:
            // registering it would ask the stream for a value it cannot have.
            continue;
        }
        reducer.declare_new(serial)?;
    }
    drop(serials);
    let mut values = input.inodes()?;
    while let Some(update) = values.next_row()? {
        if unreachable.contains(update.serial)? {
            continue;
        }
        reducer.note_value(update.serial, update.value)?;
    }
    Ok(())
}

fn note_retained_binding(
    reducer: &mut OperationReducer<'_, '_, '_, '_>,
    initial_counts: Option<&SerialState<'_>>,
    input: &dyn OperationInput,
    serial: u64,
) -> ContentResult<()> {
    if let Some(counts) = initial_counts {
        let index = input
            .new_position(serial)?
            .ok_or(ContentError::InvalidRecord("effect inode record"))?;
        if index >= input.new_rows() {
            return Err(ContentError::InvalidRecord("new inode position"));
        }
        counts.note_binding(serial, index)?;
        Ok(())
    } else {
        reducer.note_retained_binding(serial)
    }
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
fn zero_count_serials(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    reducer: &mut ReferenceReducer<'_, '_>,
    base_batch: usize,
    root_serial: u64,
    unreachable: &dyn DroppedParents,
    maximum_serials: usize,
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
                    let base_count = base.map_or(0, |value| value.namespace_ref_count);
                    crate::filesystem::references::derived_count(base_count, *delta)?
                }
            };
            // A directory this batch drops is not a released inode: it was never
            // part of the result, so there is nothing to traverse.
            if count == 0 && *serial != root_serial && !unreachable.contains(*serial)? {
                zero.push(*serial);
            }
        }
    }
    Ok((zero, reads))
}
