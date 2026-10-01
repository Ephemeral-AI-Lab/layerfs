//! One shared canonical directory/reference/inode algorithm with explicit selected reduction.
use super::reduction::{note_reduced_binding, register_reduced_values, FinalSource, Reduction};
use super::{FilesystemResult, FilesystemUpdateCounters, ValidatedInput};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::update::apply_bindings;
use crate::filesystem::inode::update::apply_inode_values;
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases, MAXIMUM_READ_DEMANDS};
use crate::filesystem::references::count_reduce::BaseAnswers;
use crate::filesystem::root::{profile_id, FilesystemRoot, FilesystemRootId};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::state::{
    DirectoryRoots, IndexedState, PageLimit, ParentCalls, STATE_MAX_PAGE_BYTES,
    STATE_MAX_PAGE_RECORDS,
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{FinalizedObject, ObjectRole};
#[allow(clippy::too_many_arguments)]
pub(super) fn run_selected_body<S: IndexedState + ?Sized, R: Reduction<S>>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    phases: &FilesystemPhases<'_>,
    cleanup_attempted: &mut bool,
    contents: &mut DirectoryRoots,
    validated: ValidatedInput,
    state: &mut S,
    parents: ParentCalls<S>,
    reducer: &mut R,
) -> ContentResult<FilesystemResult> {
    let ValidatedInput {
        topology,
        validation,
        unreachable,
    } = validated;
    let reader = objects.reader();
    let mut counters = FilesystemUpdateCounters {
        validation,
        ..FilesystemUpdateCounters::default()
    };
    let table = topology.table();
    let base_table = topology.base.map(|root| root.inode_table());
    // A build already supplies sorted new serials and every final typed value.
    // One count per declared serial avoids ordering runs and their lookups.
    let resources = input.resources();
    let mut initial_counts = reducer.initial_counts(input)?;
    if initial_counts.is_none() {
        reducer.check_capacity()?;
        register_reduced_values(reducer, input, &unreachable, state, parents)?;
    }
    let batch = resources.base_read_batch.min(MAXIMUM_READ_DEMANDS);
    let mut retained_parent_credit = None;
    let mut retained_parents = Vec::new();
    let mut retained_bases = BaseAnswers::compatibility(Vec::new());
    phases.phase("directories", || -> ContentResult<()> {
        // Validation already proved parent ordering and uniqueness. Only one
        // final batch is retained for reuse after every binding effect is known.
        // Retain only one admitted batch of scalar headers. Each directory's
        // names are consumed separately through its exact selected cursor.
        let mut waves = input.directory_headers()?;
        let mut remaining = input.directory_rows();
        loop {
            let capacity = batch.min(remaining);
            if capacity == 0 {
                if waves.next_header()?.is_some() {
                    return Err(ContentError::InvalidRecord("directory row count"));
                }
                break;
            }
            let _updates_credit =
                reducer.working(state, canonical_directory_working_bytes(capacity))?;
            let mut updates = Vec::with_capacity(capacity);
            while updates.len() < capacity {
                match waves.next_header()? {
                    Some(row) => updates.push(row),
                    None => break,
                }
            }
            if updates.len() != capacity {
                return Err(ContentError::InvalidRecord("directory row count"));
            }
            remaining -= updates.len();
            let mut parent_credit = reducer.working(
                state,
                std::mem::size_of::<Vec<u64>>() + updates.len() * std::mem::size_of::<u64>(),
            )?;
            let mut parent_serials = Vec::with_capacity(updates.len());
            for update in &updates {
                if topology.table.is_some()
                    && !unreachable.contains(state, update.parent(), parents)?
                    && !input.is_new(update.parent())?
                {
                    parent_serials.push(update.parent());
                }
            }
            let bases = reducer.base_values(state, reader, table, &parent_serials)?;
            for update in &updates {
                if unreachable.contains(state, update.parent(), parents)? {
                    // Nothing binds this directory in the result, so no page of it
                    // is worth building. Its bindings are still this operation's
                    // edges and stay accounted: every final binding of a directory
                    // this operation allocates is an addition.
                    let mut bindings = input.bindings(update)?;
                    while let Some((_, binding)) = bindings.next_binding()? {
                        let Some(child) = binding else {
                            continue;
                        };
                        note_reduced_binding(
                            reducer,
                            initial_counts.as_deref_mut(),
                            input,
                            child,
                            state,
                        )?;
                        counters.bindings_added = counters.bindings_added.saturating_add(1);
                    }
                    if !bindings.finish()?.matches(update) {
                        return Err(ContentError::InvalidRecord("directory completion"));
                    }
                    continue;
                }
                let base = parent_serials
                    .binary_search(&update.parent())
                    .ok()
                    .and_then(|index| bases[index]);
                let mut bindings = input.bindings(update)?;
                let content_root = if update.binding_count() == 0 {
                    if bindings.next_binding()?.is_some() {
                        return Err(ContentError::InvalidRecord("directory binding count"));
                    }
                    // An unchanged directory retains its root; a new directory
                    // needs one actual empty page.
                    if input.is_new(update.parent())? || topology.table.is_none() {
                        crate::filesystem::directory::update::empty_directory(objects)?.0
                    } else {
                        base.ok_or(ContentError::InvalidRecord("directory parent record"))?
                            .content_root
                    }
                } else {
                    let base_directory = if input.is_new(update.parent())? {
                        None
                    } else if topology.table.is_some() {
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
                            note_reduced_binding(
                                reducer,
                                initial_counts.as_deref_mut(),
                                input,
                                next,
                                state,
                            )?;
                            counters.bindings_added = counters.bindings_added.saturating_add(1);
                        }
                        if let Some(previous) = before {
                            reducer.removed(state, previous)?;
                            counters.bindings_removed = counters.bindings_removed.saturating_add(1);
                        }
                        Ok(())
                    };
                    let (root, work) = apply_bindings(
                        objects,
                        base_directory,
                        std::iter::from_fn(|| bindings.next_binding().transpose()),
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
                if !bindings.finish()?.matches(update) {
                    return Err(ContentError::InvalidRecord("directory completion"));
                }
                contents.append(state, update.parent(), content_root)?;
            }
            if !parent_serials.is_empty() {
                retained_parents = parent_serials;
                retained_parent_credit = parent_credit.take();
                retained_bases = bases;
            }
        }
        Ok(())
    })?;
    let _retained_parent_credit = &retained_parent_credit;
    contents.seal(state)?;
    if initial_counts.is_some() {
        // Fresh construction consumes roots by exact serial. First establish the
        // complete selected page/EOF transcript without retaining its population.
        phases.phase("state.verify", || {
            let _scan = reducer.working(
                state,
                crate::filesystem::state::directory_root_cursor_working_bytes(),
            )?;
            let mut cursor = contents.scan()?;
            let limit = PageLimit::new(batch.min(STATE_MAX_PAGE_RECORDS), STATE_MAX_PAGE_BYTES)?;
            loop {
                let _page = reducer.working(state, cursor.page_working_bytes(limit))?;
                if cursor.next_page(state, limit, _page.is_some())?.is_none() {
                    break;
                }
            }
            Ok(())
        })?;
    }
    // Keep the reducer's original all-effects-before-values insertion order:
    // interleaving values with later effects can increase spill quota demands.
    // Reuse the final parent batch; earlier omitted values are read in bounded
    // groups rather than retaining a record for every directory in the input.
    if initial_counts.is_none() {
        let _scan = reducer.working(
            state,
            crate::filesystem::state::directory_root_cursor_working_bytes(),
        )?;
        let mut cursor = contents.scan()?;
        let limit = PageLimit::new(batch.min(STATE_MAX_PAGE_RECORDS), STATE_MAX_PAGE_BYTES)?;
        loop {
            let _page_credit = reducer.working(state, cursor.page_working_bytes(limit))?;
            let Some(page) = cursor.next_page(state, limit, _page_credit.is_some())? else {
                break;
            };
            let wave = page.into_records();
            if wave.is_empty() {
                break;
            }
            let _missing_credit = reducer.working(
                state,
                std::mem::size_of::<Vec<u64>>() + wave.len() * std::mem::size_of::<u64>(),
            )?;
            let mut missing = Vec::with_capacity(wave.len());
            for record in &wave {
                let serial = record.key().serial();
                if input.value_for(serial)?.is_none()
                    && retained_parents.binary_search(&serial).is_err()
                {
                    missing.push(serial);
                }
            }
            let bases = reducer.base_values(state, reader, table, &missing)?;
            for record in wave {
                let serial = record.key().serial();
                let value = match input.value_for(serial)? {
                    Some(value) => Some(value),
                    None => retained_parents
                        .binary_search(&serial)
                        .ok()
                        .and_then(|index| retained_bases[index])
                        .or_else(|| {
                            missing
                                .binary_search(&serial)
                                .ok()
                                .and_then(|index| bases[index])
                        }),
                };
                let value = value.ok_or(ContentError::InvalidRecord("directory value missing"))?;
                reducer.value(
                    state,
                    serial,
                    InodeValue {
                        content_root: record.root(),
                        ..value
                    },
                )?;
            }
        }
    }
    // The root scan consumed the final parent answers. End their actual owners
    // before zero scanning, descendant release and the final inode consumer.
    drop(retained_parents);
    drop(retained_parent_credit);
    drop(retained_bases);
    // Every other supplied value keeps the content root the caller named, unless
    // this operation rebuilt that inode's own directory.
    if initial_counts.is_none() {
        let mut values = input.inodes()?;
        while let Some(update) = values.next_row()? {
            if contents.get(state, update.serial)?.is_some()
                || unreachable.contains(state, update.serial, parents)?
            {
                // A directory this batch drops is not part of the result at all: its
                // value is never a final row, so it must not enter the reduction.
                continue;
            }
            reducer.value(state, update.serial, update.value)?;
        }
    }
    if topology.table.is_some() {
        // Only an update can release descendants: a new filesystem has no base
        // binding to lose, and its root is never released.
        let zero = reducer.zeros(
            state,
            reader,
            table,
            resources.base_read_batch,
            input.root_serial(),
            &unreachable,
            resources.maximum_touched_serials(),
            parents,
        )?;
        counters.base_records_read = counters.base_records_read.saturating_add(zero.1);
        counters.release = reducer.release_descendants(
            state,
            reader,
            table,
            &zero.0,
            resources.base_read_batch,
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
            if unreachable.contains(state, serial, parents)? {
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
                    content_root: contents.get(state, serial)?.unwrap_or(value.content_root),
                    ..value
                }),
            ));
        }
        drop(serials);
        let changes = rows.into_iter().map(Ok);
        let built = phases.phase("inodes", || {
            apply_inode_values(objects, None, changes, resources.scratch_bytes)
        })?;
        counters.references.final_values = (new_count - unreachable.len()?) as u64;
        built
    } else {
        let mut rows = phases.phase("references", || {
            reducer.finish(
                state,
                reader,
                table,
                resources.base_read_batch.max(1),
                input.root_serial(),
            )
        })?;
        let mut source_error: Option<ContentError> = None;
        let changes = std::iter::from_fn(|| match rows.next_change(state) {
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
    phases.phase("cleanup", || reducer.release(state))?;
    phases.phase("parent.complete", || unreachable.retire(state, parents))?;
    phases.phase("state.complete", || contents.release(state))?;
    let root = match topology.base {
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

/// Actual requested directory header/control window before allocation; zero rows allocate zero.
pub const fn canonical_directory_working_bytes(records: usize) -> usize {
    std::mem::size_of::<Vec<crate::filesystem::rows::DirectoryHeader>>()
        + records * std::mem::size_of::<crate::filesystem::rows::DirectoryHeader>()
}
