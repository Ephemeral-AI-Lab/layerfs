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
use crate::filesystem::input::FilesystemInput;
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer, ReferenceWork};
use crate::filesystem::references::release::{release_zero_count, ReleaseWork};
use crate::filesystem::root::{profile_id, FilesystemRoot, FilesystemRootId};
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
    input: &FilesystemInput<'_>,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base.is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, &FilesystemPhases::disabled())
}

/// Builds a new filesystem while recording the caller's coarse phase scopes.
pub fn build_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &FilesystemInput<'_>,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base.is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, phases)
}

/// Applies one complete update to a checked immutable base root.
pub fn update_filesystem(
    objects: &mut FilesystemObjects<'_>,
    input: &FilesystemInput<'_>,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base.is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, &FilesystemPhases::disabled())
}

/// Applies one complete update while recording the caller's coarse phase scopes.
pub fn update_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &FilesystemInput<'_>,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base.is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, phases)
}

fn run<'b>(
    objects: &mut FilesystemObjects<'_>,
    input: &FilesystemInput<'_>,
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
    input: &FilesystemInput<'_>,
    backing: Option<&mut (dyn OrderingBacking + 'b)>,
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
) -> ContentResult<FilesystemResult> {
    // A directory this batch leaves with no binding is dead on arrival: its
    // parent already accounted the binding it lost, so there is no final count to
    // hold it, no page worth building, and no subtree to walk.
    let unreachable = unreachable_parents(input);
    let mut validation = ValidationWork::default();
    let checked = phases.phase("validate", || {
        validate::check(objects.reader(), input, &unreachable, &mut validation)
    })?;
    let reader = objects.reader();
    let mut counters = FilesystemUpdateCounters {
        validation,
        ..FilesystemUpdateCounters::default()
    };
    let table = checked.topology.table();
    let base_table = checked.topology.base.map(|root| root.inode_table());
    let mut reducer = ReferenceReducer::new(
        input.resources.maximum_pending_records,
        backing,
        input.resources.merge_buffer_bytes,
        input.resources.ordering_bytes,
    );
    reducer.check_backing_capacity()?;
    register_values(&mut reducer, input, &unreachable)?;
    let mut contents: BTreeMap<u64, ObjectId> = BTreeMap::new();
    phases.phase("directories", || -> ContentResult<()> {
        for update in input.directories {
            if unreachable.contains_key(&update.parent) {
                // Nothing binds this directory in the result, so no page of it
                // is worth building. Its bindings are still this operation's
                // edges and stay accounted: every final binding of a directory
                // this operation allocates is an addition.
                for (_, binding) in &update.changes {
                    let Some(child) = binding else {
                        continue;
                    };
                    reducer.note_retained_binding(*child)?;
                    counters.bindings_added = counters.bindings_added.saturating_add(1);
                }
                continue;
            }
            if update.changes.is_empty() {
                // A directory with no changed name keeps the content root it already
                // has; a directory in a new filesystem is a real empty page.
                let content = if input.new_inodes.contains(&update.parent)
                    || checked.topology.table.is_none()
                {
                    crate::filesystem::directory::update::empty_directory(objects)?.0
                } else {
                    lookup_base(reader, table, update.parent)?
                        .ok_or(ContentError::InvalidRecord("directory parent record"))?
                        .content_root
                };
                contents.insert(update.parent, content);
                continue;
            }
            let base_directory = if input.new_inodes.contains(&update.parent) {
                // A directory this operation allocates has no stored bindings yet.
                None
            } else if checked.topology.table.is_some() {
                let record = lookup_base(reader, table, update.parent)?
                    .ok_or(ContentError::InvalidRecord("directory parent record"))?;
                if record.kind != InodeKind::Directory {
                    return Err(ContentError::InvalidRecord("directory parent kind"));
                }
                Some(DirectoryRoot(record.content_root))
            } else {
                None
            };
            let mut observe = |before: Option<u64>, after: Option<u64>| -> ContentResult<()> {
                if before == after {
                    return Ok(());
                }
                // Additions are accounted before removals, so a move never drops an
                // inode to a spurious zero between its two bindings.
                if let Some(next) = after {
                    reducer.note_retained_binding(next)?;
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
                update
                    .changes
                    .iter()
                    .map(|(name, binding)| Ok((name.clone(), *binding))),
                input.resources.scratch_bytes,
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
            contents.insert(update.parent, root.0);
        }
        Ok(())
    })?;
    // A directory whose bindings this operation merged gets that directory's new
    // root as its content root. Its kind and attribute root come from the caller's
    // typed value when one was supplied, and from the stored record otherwise, so
    // a caller that only changes names does not have to restate its metadata.
    for (serial, content_root) in &contents {
        let value = match input.value_for(*serial) {
            Some(value) => Some(InodeValue {
                content_root: *content_root,
                ..value
            }),
            None => lookup_base(reader, table, *serial)?.map(|base| InodeValue {
                content_root: *content_root,
                ..base
            }),
        };
        let value = value.ok_or(ContentError::InvalidRecord("directory value missing"))?;
        reducer.note_value(*serial, value)?;
    }
    // Every other supplied value keeps the content root the caller named, unless
    // this operation rebuilt that inode's own directory.
    let mut updates = Vec::with_capacity(input.inodes.len());
    for update in input.inodes {
        if contents.contains_key(&update.serial) || unreachable.contains_key(&update.serial) {
            // A directory this batch drops is not part of the result at all: its
            // value is never a final row, so it must not enter the reduction.
            continue;
        }
        updates.push((update.serial, update.value));
    }
    for (serial, value) in updates {
        reducer.note_value(serial, value)?;
    }
    if checked.topology.table.is_some() {
        // Only an update can release descendants: a new filesystem has no base
        // binding to lose, and its root is never released.
        let zero = zero_count_serials(
            reader,
            table,
            &mut reducer,
            input.resources.base_read_batch,
            input.root_serial,
            &unreachable,
            input.resources.maximum_touched_serials(),
        )?;
        counters.base_records_read = counters.base_records_read.saturating_add(zero.1);
        counters.release = release_zero_count(
            reader,
            table,
            &mut reducer,
            &zero.0,
            input.resources.base_read_batch,
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            |reader, serial| {
                lookup_base(reader, table, serial)?
                    .ok_or(ContentError::InvalidRecord("released inode record"))
            },
        )?;
    }
    let mut rows = phases.phase("references", || {
        reducer.finish(
            reader,
            table,
            input.resources.base_read_batch.max(1),
            input.root_serial,
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
        apply_inode_values(objects, base_table, changes, input.resources.scratch_bytes)
    });
    if let Some(error) = source_error {
        return Err(error);
    }
    let (inode_table, inode_work) = built?;
    counters.inodes = inode_work;
    // The final stream is done with: its counters are snapshotted, its reader
    // handle is closed, and only then is the backing's completion checked. A
    // cleanup failure fails the operation before any root object exists, so a
    // successful result always means the ordering resources were released.
    counters.references = rows.work();
    drop(rows);
    *cleanup_attempted = true;
    phases.phase("cleanup", || reducer.release())?;
    let root = match checked.topology.base {
        Some(root) => root.with_inode_table(inode_table),
        None => FilesystemRoot::new(profile_id(), input.scope, input.root_serial, inode_table)?,
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
fn unreachable_parents(input: &FilesystemInput<'_>) -> BTreeMap<u64, ()> {
    let mut bound: BTreeMap<u64, ()> = BTreeMap::new();
    for update in input.directories {
        for (_, binding) in &update.changes {
            if let Some(child) = binding {
                bound.insert(*child, ());
            }
        }
    }
    let mut dead = BTreeMap::new();
    for update in input.directories {
        let parent = update.parent;
        if parent == input.root_serial || bound.contains_key(&parent) {
            continue;
        }
        if !input.new_inodes.contains(&parent) {
            continue;
        }
        // An empty binding list is the "keep the bindings you have" form, and a
        // directory this operation allocates has none to keep.
        dead.insert(parent, ());
    }
    dead
}

fn register_values(
    reducer: &mut ReferenceReducer<'_, '_>,
    input: &FilesystemInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
) -> ContentResult<()> {
    for serial in input.new_inodes {
        if unreachable.contains_key(serial) {
            // The serial is not part of the result, so it is not a final row:
            // registering it would ask the stream for a value it cannot have.
            continue;
        }
        reducer.declare_new(*serial)?;
    }
    for update in input.inodes {
        if unreachable.contains_key(&update.serial) {
            continue;
        }
        reducer.note_value(update.serial, update.value)?;
    }
    Ok(())
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
