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
use crate::filesystem::directory::update::apply_bindings;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::inode::update::apply_inode_values;
use crate::filesystem::objects::{
    FilesystemObjects, FilesystemPhases, ObjectWork, MAXIMUM_READ_DEMANDS,
};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer, ReferenceWork};
use crate::filesystem::references::release::{release_zero_count, ReleaseWork};
use crate::filesystem::root::{profile_id, FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::view::{OperationInput, ResidentInput, StreamedInput};
use crate::filesystem::rows::{PreparedDirectoryStreams, PreparedRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::sorted::SortedWork;
use crate::filesystem::validate::{self, ValidationWork};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole};

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

fn run<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn OperationInput,
    backing: Option<&'b mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let mut backing = backing;
    // The flag is set by the body the moment the checked completion runs, so the
    // failure path below never releases the same backing twice and never depends
    // on recognising a particular error label.
    let mut cleanup_attempted = false;
    let outcome = {
        let borrowed: Option<&mut (dyn OrderingBacking + 'b)> = backing.as_deref_mut();
        run_body(objects, input, borrowed, phases, &mut cleanup_attempted)
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
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
) -> ContentResult<FilesystemResult> {
    // A directory this batch leaves with no binding is dead on arrival: its
    // parent already accounted the binding it lost, so there is no final count to
    // hold it, no page worth building, and no subtree to walk.
    let unreachable = unreachable_parents(input)?;
    let mut validation = ValidationWork::default();
    let checked = phases.phase("validate", || {
        validate::check_operation(objects.reader(), input, &unreachable, &mut validation)
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
    let mut initial_counts = if base_table.is_none() {
        if input.new_rows() > resources.maximum_touched_serials() {
            return Err(ContentError::ObjectLimitExceeded {
                limit: resources.maximum_touched_serials(),
                actual: input.new_rows(),
            });
        }
        Some(vec![0_u64; input.new_rows()])
    } else {
        None
    };
    let mut reducer = ReferenceReducer::new(
        resources.maximum_pending_records,
        backing,
        resources.merge_buffer_bytes,
        resources.ordering_bytes,
    );
    if initial_counts.is_none() {
        reducer.check_backing_capacity()?;
        register_values(&mut reducer, input, &unreachable)?;
    }
    let batch = resources.base_read_batch.min(MAXIMUM_READ_DEMANDS);
    let mut contents: BTreeMap<u64, ObjectId> = BTreeMap::new();
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
                    && !unreachable.contains_key(&update.header.parent)
                    && !input.is_new(update.header.parent)?
                {
                    parents.push(update.header.parent);
                }
            }
            let bases = lookup_many(reader, table, &parents, &mut InodeReadWork::default())?;
            for update in &updates {
                if unreachable.contains_key(&update.header.parent) {
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
                            initial_counts.as_deref_mut(),
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
                                initial_counts.as_deref_mut(),
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
                contents.insert(update.header.parent, content_root);
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
    if initial_counts.is_none() {
        let mut contents_iter = contents.iter();
        loop {
            let wave = contents_iter.by_ref().take(batch).collect::<Vec<_>>();
            if wave.is_empty() {
                break;
            }
            let mut missing = Vec::with_capacity(wave.len());
            for (serial, _) in &wave {
                if input.value_for(**serial)?.is_none()
                    && retained_parents.binary_search(serial).is_err()
                {
                    missing.push(**serial);
                }
            }
            let bases = lookup_many(reader, table, &missing, &mut InodeReadWork::default())?;
            for (serial, content_root) in wave {
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
    if initial_counts.is_none() {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if contents.contains_key(&update.serial) || unreachable.contains_key(&update.serial) {
                // A directory this batch drops is not part of the result at all: its
                // value is never a final row, so it must not enter the reduction.
                continue;
            }
            reducer.note_value(update.serial, update.value)?;
        }
    }
    if checked.topology.table.is_some() {
        // Only an update can release descendants: a new filesystem has no base
        // binding to lose, and its root is never released.
        let zero = zero_count_serials(
            reader,
            table,
            &mut reducer,
            resources.base_read_batch,
            input.root_serial(),
            &unreachable,
            resources.maximum_touched_serials(),
        )?;
        counters.base_records_read = counters.base_records_read.saturating_add(zero.1);
        counters.release = release_zero_count(
            reader,
            table,
            &mut reducer,
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
    let (inode_table, inode_work) = if let Some(counts) = initial_counts {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if input.new_position(update.serial)?.is_none() {
                return Err(ContentError::InvalidRecord("effect inode record"));
            }
        }
        drop(values);
        let root_serial = input.root_serial();
        let new_count = input.new_rows();
        let mut serials = input.new_inodes()?;
        let mut rows: Vec<(u64, Option<InodeValue>)> = Vec::with_capacity(new_count);
        for count in counts {
            let Some(serial) = serials.next_row()? else {
                return Err(ContentError::InvalidRecord("new inode row count"));
            };
            if unreachable.contains_key(&serial) {
                continue;
            }
            if count == 0 && serial != root_serial {
                return Err(ContentError::InvalidRecord("new inode without binding"));
            }
            let value = input
                .value_for(serial)?
                .ok_or(ContentError::InvalidRecord("new inode value"))?;
            rows.push((
                serial,
                Some(InodeValue {
                    namespace_ref_count: count,
                    content_root: contents.get(&serial).copied().unwrap_or(value.content_root),
                    ..value
                }),
            ));
        }
        drop(serials);
        let changes = rows.into_iter().map(Ok);
        let built = phases.phase("inodes", || {
            apply_inode_values(objects, None, changes, resources.scratch_bytes)
        })?;
        counters.references.final_values = (new_count - unreachable.len()) as u64;
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
        let mut source_error: Option<ContentError> = None;
        let changes = std::iter::from_fn(|| match rows.next_change() {
            Ok(Some(change)) => Some(Ok((change.serial, change.value))),
            Ok(None) => None,
            Err(error) => {
                source_error = Some(error);
                None
            }
        });
        let built = phases.phase("inodes", || {
            apply_inode_values(objects, base_table, changes, resources.scratch_bytes)
        });
        if let Some(error) = source_error {
            return Err(error);
        }
        let built = built?;
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
fn unreachable_parents(input: &dyn OperationInput) -> ContentResult<BTreeMap<u64, ()>> {
    let limit = usize::try_from(input.resources().ordering_bytes / 1024).unwrap_or(usize::MAX);
    let root = input.root_serial();
    // The retained membership is the targeted set itself: one entry per
    // declared-new parent other than the root, because only such a parent can
    // end the operation with no binding at all. Each entry is charged to the
    // ordering ceiling as the scratch it is; unrelated child bindings are never
    // retained, so a wide directory does not multiply this charge.
    let mut parents: BTreeMap<u64, bool> = BTreeMap::new();
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        let parent = update.header.parent;
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
    let mut rows = input.directories()?;
    while let Some(update) = rows.next_row()? {
        for change in update.changes()? {
            let (_, binding) = change?;
            if let Some(child) = binding {
                if let Some(bound) = parents.get_mut(&child) {
                    *bound = true;
                }
            }
        }
    }
    drop(rows);
    Ok(parents
        .into_iter()
        .filter(|(_, bound)| !*bound)
        .map(|(parent, _)| (parent, ()))
        .collect())
}

fn register_values(
    reducer: &mut ReferenceReducer<'_, '_>,
    input: &dyn OperationInput,
    unreachable: &BTreeMap<u64, ()>,
) -> ContentResult<()> {
    let mut serials = input.new_inodes()?;
    while let Some(serial) = serials.next_row()? {
        if unreachable.contains_key(&serial) {
            // The serial is not part of the result, so it is not a final row:
            // registering it would ask the stream for a value it cannot have.
            continue;
        }
        reducer.declare_new(serial)?;
    }
    drop(serials);
    let mut values = input.inodes()?;
    while let Some(update) = values.next_row()? {
        if unreachable.contains_key(&update.serial) {
            continue;
        }
        reducer.note_value(update.serial, update.value)?;
    }
    Ok(())
}

fn note_retained_binding(
    reducer: &mut ReferenceReducer<'_, '_>,
    initial_counts: Option<&mut [u64]>,
    input: &dyn OperationInput,
    serial: u64,
) -> ContentResult<()> {
    if let Some(counts) = initial_counts {
        let index = input
            .new_position(serial)?
            .ok_or(ContentError::InvalidRecord("effect inode record"))?;
        counts[index] = counts[index]
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
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
    unreachable: &BTreeMap<u64, ()>,
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
                    let base_count = base.map_or(0, |value| value.namespace_ref_count as i128);
                    u64::try_from((base_count + i128::from(*delta)).max(0)).unwrap_or(0)
                }
            };
            // A directory this batch drops is not a released inode: it was never
            // part of the result, so there is nothing to traverse.
            if count == 0 && *serial != root_serial && !unreachable.contains_key(serial) {
                zero.push(*serial);
            }
        }
    }
    Ok((zero, reads))
}
