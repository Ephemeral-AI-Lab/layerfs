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
use crate::filesystem::objects::{FilesystemObjects, ObjectWork};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::reduce::{PendingState, ReferenceReducer, ReferenceWork};
use crate::filesystem::references::release::{release_zero_count, ReleaseWork};
use crate::filesystem::root::{profile_id, FilesystemRoot, FilesystemRootId};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::sorted::SortedWork;
use crate::filesystem::validate;
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
    run(objects, input, backing)
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
    run(objects, input, backing)
}

fn run(
    objects: &mut FilesystemObjects<'_>,
    input: &FilesystemInput<'_>,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    let checked = validate::check(objects.reader(), input)?;
    let reader = objects.reader();
    let mut counters = FilesystemUpdateCounters::default();
    let table = checked.topology.table();
    let base_table = checked.topology.base.map(|root| root.inode_table());
    let mut reducer = ReferenceReducer::new(
        input.resources.maximum_pending_records,
        backing,
        input.resources.merge_buffer_bytes,
    );
    register_values(&mut reducer, input)?;
    let mut contents: BTreeMap<u64, ObjectId> = BTreeMap::new();
    for update in input.directories {
        if update.changes.is_empty() {
            // A directory with no changed name keeps the content root it already
            // has; a directory in a new filesystem is a real empty page.
            let content = match checked.topology.table {
                Some(_) => {
                    lookup_base(reader, table, update.parent)?
                        .ok_or(ContentError::InvalidRecord("directory parent record"))?
                        .content_root
                }
                None => crate::filesystem::directory::update::empty_directory(objects)?.0,
            };
            contents.insert(update.parent, content);
            continue;
        }
        let base_directory = match checked.topology.table {
            Some(_) => {
                let record = lookup_base(reader, table, update.parent)?
                    .ok_or(ContentError::InvalidRecord("directory parent record"))?;
                if record.kind != InodeKind::Directory {
                    return Err(ContentError::InvalidRecord("directory parent kind"));
                }
                Some(DirectoryRoot(record.content_root))
            }
            None => None,
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
    // A supplied typed value keeps the stored content root unless this operation
    // rebuilt that inode's directory.
    let mut updates = Vec::with_capacity(input.inodes.len());
    for update in input.inodes {
        let content_root = contents
            .get(&update.serial)
            .copied()
            .unwrap_or(update.value.content_root);
        updates.push((
            update.serial,
            InodeValue {
                content_root,
                ..update.value
            },
        ));
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
    let mut rows = reducer.finish(
        reader,
        table,
        input.resources.base_read_batch.max(1),
        input.root_serial,
    )?;
    let mut source_error: Option<ContentError> = None;
    let changes = std::iter::from_fn(|| match rows.next_change() {
        Ok(Some(change)) => Some(Ok((change.serial, change.value))),
        Ok(None) => None,
        Err(error) => {
            source_error = Some(error);
            None
        }
    });
    let built = apply_inode_values(objects, base_table, changes, input.resources.scratch_bytes);
    if let Some(error) = source_error {
        return Err(error);
    }
    let (inode_table, inode_work) = built?;
    counters.inodes = inode_work;
    let references = rows.work();
    counters.references = ReferenceWork {
        runs: references.runs,
        ..references
    };
    let root = match checked.topology.base {
        Some(root) => root.with_inode_table(inode_table),
        None => FilesystemRoot::new(profile_id(), input.scope, input.root_serial, inode_table)?,
    };
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode()?)?
        .with_references(vec![inode_table]);
    let id = objects.emit(object)?;
    counters.objects = objects.work();
    Ok(FilesystemResult {
        root: FilesystemRootId(id),
        value: root,
        counters,
    })
}

fn register_values(
    reducer: &mut ReferenceReducer<'_>,
    input: &FilesystemInput<'_>,
) -> ContentResult<()> {
    for serial in input.new_inodes {
        reducer.declare_new(*serial)?;
    }
    for update in input.inodes {
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
    reducer: &mut ReferenceReducer<'_>,
    base_batch: usize,
    root_serial: u64,
) -> ContentResult<(Vec<u64>, u64)> {
    let touched = reducer.touched_serials(base_batch)?;
    let mut zero = Vec::new();
    let mut reads = 0_u64;
    for wave in touched.chunks(base_batch.max(1)) {
        let bases = lookup_many(reader, table, wave, &mut InodeReadWork::default())?;
        reads = reads.saturating_add(wave.len() as u64);
        for (index, serial) in wave.iter().enumerate() {
            let base = bases[index];
            let count = match reducer.state(*serial)? {
                Some(PendingState::New { count, .. }) => count,
                Some(PendingState::Existing { delta, .. }) => {
                    let base_count = base.map_or(0, |value| value.namespace_ref_count as i128);
                    u64::try_from((base_count + i128::from(delta)).max(0)).unwrap_or(0)
                }
                None => base.map_or(0, |value| value.namespace_ref_count),
            };
            if count == 0 && *serial != root_serial {
                zero.push(*serial);
            }
        }
    }
    Ok((zero, reads))
}
