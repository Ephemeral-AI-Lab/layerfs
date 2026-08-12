//! Complete bounded create operation composing CDC, CAS, and structural COW.
//!
//! This module exists only behind the C3 polymorphism feature. It deliberately
//! exposes no publication authority: the terminal result is a synchronous,
//! consumed storage handoff. One ledger reservation is acquired before the
//! source supplier is invoked and is borrowed by every lower layer.

use crate::cdc::CdcAlgorithmV1;
use crate::content::{create_file_borrowed_v1, ContentBuffersV1, ContentSourceV1};
use crate::cow::{
    build_canonical_directory_borrowed_v1, preflight_canonical_tree_v1, CanonicalTreeChildV1,
    CanonicalTreeEntryV1, DirectoryBuildModeV1, DirectoryLogicalIdentityV1, TreePageSummaryV1,
    MAX_TREE_OBJECT_BYTES,
};
use crate::format::{
    require_strictly_increasing_paths, validate_chunk_refs_per_file,
    validate_chunk_refs_per_version, validate_entry_count, validate_file_mode,
    validate_logical_length, validate_total_object_count, validate_tree_object_count,
    ValidatedComponent, ValidatedPath, MAX_PATH_DEPTH,
};
use crate::identity::{
    derive_file_node_v1, derive_version_v1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{
    MemoryComponentV1, ObservationScopeV1, OperationCountersV1, OperationMemoryPlanV1,
    OptionalU64ObservationV1,
};
use crate::{CoreError, CoreResult};

use crate::lifecycle::{
    admission_traversal_resident_bytes_v1, run_lifecycle_v1, storage_envelope_v1,
    BuiltDirectoryRecordV1, BuiltFileRecordV1, CreateOperationGrantV1, LifecycleControlV1,
    LifecyclePlanV1, OperationBuffersV1, OperationErrorV1, OperationHandoffV1, PreparedCandidateV1,
    SharedOperationControlV1, StorageOperationV1, StorageSessionPortV1, VersionSummaryInputV1,
    MAX_STORAGE_RECORDS_V1,
};
const DEFAULT_METADATA_RESERVATION_BYTES: u64 = 1_048_576;
const DEFAULT_EXPLICIT_DIRECTORY_MODE: u16 = 0o755;
const MANIFEST_BUILD_STACK_CAPACITY_V1: usize = MAX_PATH_DEPTH + 1;

fn closure_traversal_bytes_v1(maximum_objects: u64) -> CoreResult<usize> {
    usize::try_from(maximum_objects)
        .map_err(|_| CoreError::IntegerOverflow)?
        .checked_mul(2)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or(CoreError::IntegerOverflow)
}

fn global_seen_capacity_v1(maximum_objects: u64) -> CoreResult<u32> {
    let required = maximum_objects
        .checked_mul(2)
        .ok_or(CoreError::IntegerOverflow)?
        .max(8);
    let capacity = required
        .checked_next_power_of_two()
        .ok_or(CoreError::IntegerOverflow)?;
    u32::try_from(capacity).map_err(|_| CoreError::CountCap)
}

pub(crate) trait SourceSupplierV1 {
    type Source: ContentSourceV1;

    /// Side-effect-free bound queried only after the root grant is held.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn supply(self) -> CoreResult<Self::Source>;
}

/// One file in a bounded, canonically ordered private C3 tree operation.
/// The source is retained by the caller and is not read until the complete
/// manifest has passed path, type, count, and memory preflight.
pub(crate) struct TreeFileV1<'path, S> {
    path: &'path [u8],
    mode: u16,
    declared_len: u64,
    supplier: Option<S>,
}

impl<'path, S> TreeFileV1<'path, S> {
    pub(crate) const fn new(path: &'path [u8], mode: u16, declared_len: u64, source: S) -> Self {
        Self {
            path,
            mode,
            declared_len,
            supplier: Some(source),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_create_v1<S, C>(
    grant: CreateOperationGrantV1<'_>,
    algorithm: CdcAlgorithmV1,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    supplier: S,
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: SourceSupplierV1,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = grant.into_operation();
    let (component, maximum_records_u32, global_seen_capacity, supplier_resident, storage_resident) =
        operation.run_preparation_free_stage_v1(
            counters,
            control,
            |operation, counters, _control| {
                operation.require_complete_file_kind_v1()?;
                operation.declare_empty_storage_envelope_v1()?;

                let component = ValidatedComponent::new(name)?;
                let maximum_refs = declared_len
                    .checked_add(8_191)
                    .ok_or(CoreError::IntegerOverflow)?
                    / 8_192;
                validate_chunk_refs_per_file(maximum_refs)?;
                let maximum_records = maximum_refs
                    .checked_add(4)
                    .ok_or(CoreError::IntegerOverflow)?;
                if maximum_records > MAX_STORAGE_RECORDS_V1 {
                    return Err(CoreError::CountCap.into());
                }
                let maximum_records_u32 =
                    u32::try_from(maximum_records).map_err(|_| CoreError::IntegerOverflow)?;
                let global_seen_capacity = global_seen_capacity_v1(maximum_records)?;
                let root_shape = preflight_canonical_tree_v1(1)?;
                let required_traversal_bytes = closure_traversal_bytes_v1(maximum_records)?;
                if buffers.traversal_state.len() < required_traversal_bytes
                    || buffers.tree_pages.len()
                        < usize::try_from(root_shape.page_summary_count())
                            .map_err(|_| CoreError::IntegerOverflow)?
                {
                    return Err(CoreError::ResourceRefused.into());
                }

                operation.declare_storage_envelope_v1(storage_envelope_v1(
                    maximum_records,
                    maximum_records,
                    maximum_refs,
                    1,
                    u64::from(root_shape.tree_object_count()),
                    declared_len,
                    global_seen_capacity,
                    false,
                )?)?;

                // The final conservative envelope is live before the first
                // supplier callback. No preparation path exists in this stage.
                let supplier_resident = supplier.resident_memory_bound_bytes()?;
                let storage_resident =
                    operation.storage_resident_plan_v1(false, maximum_records_u32)?;
                let port_resident = supplier_resident
                    .checked_add(storage_resident.total_resident_bytes_v1())
                    .ok_or(CoreError::IntegerOverflow)?;
                let metadata_reservation = port_resident.max(DEFAULT_METADATA_RESERVATION_BYTES);
                let plan = OperationMemoryPlanV1::empty()
                    .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
                    .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
                    .charge(
                        MemoryComponentV1::ComparisonWindow,
                        (2 * COMPARISON_WINDOW_BYTES) as u64,
                    )?
                    .charge(
                        MemoryComponentV1::ObjectScratch,
                        buffers.tree_object.len() as u64,
                    )?
                    .charge(
                        MemoryComponentV1::PageSummaries,
                        admission_traversal_resident_bytes_v1()?
                            .max(core::mem::size_of_val(buffers.tree_pages) as u64),
                    )?
                    .charge(
                        MemoryComponentV1::TraversalState,
                        buffers.traversal_state.len() as u64,
                    )?
                    .charge(MemoryComponentV1::MetadataWindow, metadata_reservation)?
                    .charge(
                        MemoryComponentV1::HashState,
                        IDENTITY_HASHER_BYTES_V1
                            .checked_mul(2)
                            .ok_or(CoreError::IntegerOverflow)?,
                    )?;
                operation.declare_plan_v1(plan)?;
                counters.memory_high_water = counters
                    .memory_high_water
                    .max(operation.memory_high_water_bytes_v1());
                Ok((
                    component,
                    maximum_records_u32,
                    global_seen_capacity,
                    supplier_resident,
                    storage_resident,
                ))
            },
        )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records: maximum_records_u32,
            algorithm,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let mut source = supplier.supply()?;
            if source.resident_memory_bound_bytes()? > supplier_resident {
                return Err(CoreError::ResourceRefused.into());
            }
            let mut cdc_control = SharedOperationControlV1::new(control_cell);
            let (references, sink) = storage.content_parts_v1();
            let file = create_file_borrowed_v1(
                name,
                mode,
                declared_len,
                &mut source,
                sink,
                references,
                ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                &mut cdc_control,
                reservation,
                algorithm,
                counters,
            )?;
            let file_node = derive_file_node_v1(mode, file.logical_file())?;
            let entry = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: file_node,
                    physical: file.physical_file(),
                },
            );
            let tree = build_canonical_directory_borrowed_v1(
                DirectoryBuildModeV1::ImplicitRoot,
                &[entry],
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                buffers.tree_pages,
            )?;
            let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = tree.logical() else {
                return Err(CoreError::TypeDomain.into());
            };
            let version = storage.write_version_v1(
                derive_version_v1(logical_root),
                tree.physical(),
                VersionSummaryInputV1::new(
                    declared_len,
                    declared_len,
                    tree.entry_count(),
                    u32::from(declared_len != 0),
                    file.chunk_count(),
                ),
                counters,
            )?;
            let completed = storage.complete_v1(version)?;
            let reference_spool_bytes = storage.reference_storage_bytes_v1()?;
            Ok(PreparedCandidateV1::new(
                version,
                tree.physical(),
                completed,
                reference_spool_bytes,
            ))
        },
    )
}

#[derive(Clone, Copy, Default)]
struct TreePreflightV1 {
    directory_entry_count: u64,
    tree_object_count: u64,
    directory_count: u64,
    peak_entry_memory: u64,
    maximum_page_summary_count: u32,
}

impl TreePreflightV1 {
    fn add_child(&mut self, child: Self) -> CoreResult<()> {
        self.directory_entry_count = self
            .directory_entry_count
            .checked_add(child.directory_entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.tree_object_count = self
            .tree_object_count
            .checked_add(child.tree_object_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.directory_count = self
            .directory_count
            .checked_add(child.directory_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.peak_entry_memory = self.peak_entry_memory.max(child.peak_entry_memory);
        self.maximum_page_summary_count = self
            .maximum_page_summary_count
            .max(child.maximum_page_summary_count);
        Ok(())
    }
}

fn path_component_at(path: &[u8], prefix_len: usize) -> CoreResult<(&[u8], bool)> {
    let tail = path.get(prefix_len..).ok_or(CoreError::Path)?;
    if tail.is_empty() {
        return Err(CoreError::Path);
    }
    match tail.iter().position(|&byte| byte == b'/') {
        Some(end) => Ok((&tail[..end], true)),
        None => Ok((tail, false)),
    }
}

fn directory_group_end<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<usize> {
    let (component, _) = path_component_at(files[start].path, prefix_len)?;
    let mut cursor = start + 1;
    while cursor < end {
        let (candidate, _) = path_component_at(files[cursor].path, prefix_len)?;
        if candidate != component {
            break;
        }
        cursor += 1;
    }
    Ok(cursor)
}

#[derive(Clone, Copy)]
struct ManifestPreflightFrameV1 {
    start: usize,
    end: usize,
    prefix_len: usize,
    cursor: usize,
    entry_count: u64,
    result: TreePreflightV1,
}

impl ManifestPreflightFrameV1 {
    const fn new(start: usize, end: usize, prefix_len: usize) -> Self {
        Self {
            start,
            end,
            prefix_len,
            cursor: start,
            entry_count: 0,
            result: TreePreflightV1 {
                directory_entry_count: 0,
                tree_object_count: 0,
                directory_count: 0,
                peak_entry_memory: 0,
                maximum_page_summary_count: 0,
            },
        }
    }

    fn finish(mut self) -> CoreResult<TreePreflightV1> {
        if self.cursor != self.end || self.start > self.end {
            return Err(CoreError::Truncated);
        }
        let shape = preflight_canonical_tree_v1(self.entry_count)?;
        self.result.directory_entry_count = self
            .result
            .directory_entry_count
            .checked_add(self.entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.tree_object_count = self
            .result
            .tree_object_count
            .checked_add(u64::from(shape.tree_object_count()))
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.directory_count = self
            .result
            .directory_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let entry_bytes = self
            .entry_count
            .checked_mul(
                u64::try_from(core::mem::size_of::<CanonicalTreeEntryV1<'static>>())
                    .map_err(|_| CoreError::IntegerOverflow)?,
            )
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.peak_entry_memory = entry_bytes
            .checked_add(self.result.peak_entry_memory)
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.maximum_page_summary_count = self
            .result
            .maximum_page_summary_count
            .max(shape.page_summary_count());
        Ok(self.result)
    }
}

fn preflight_manifest_directory_v1<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<TreePreflightV1> {
    // This pass intentionally uses an explicit fixed-capacity stack. A legal
    // 256-component path must not depend on the platform's native call-stack
    // size, and an invalid 257th component has already been rejected by
    // `ValidatedPath` before this function is entered.
    let mut stack = [None::<ManifestPreflightFrameV1>; MAX_PATH_DEPTH.saturating_add(1)];
    stack[0] = Some(ManifestPreflightFrameV1::new(start, end, prefix_len));
    let mut depth = 0_usize;
    loop {
        let frame = stack[depth].ok_or(CoreError::Truncated)?;
        if frame.cursor == frame.end {
            let completed = frame.finish()?;
            stack[depth] = None;
            if depth == 0 {
                return Ok(completed);
            }
            depth -= 1;
            stack[depth]
                .as_mut()
                .ok_or(CoreError::Truncated)?
                .result
                .add_child(completed)?;
            continue;
        }
        if frame.cursor > frame.end {
            return Err(CoreError::Truncated);
        }
        let cursor = frame.cursor;
        let (component, has_descendants) = path_component_at(files[cursor].path, frame.prefix_len)?;
        ValidatedComponent::new(component)?;
        let group_end = directory_group_end(files, cursor, frame.end, frame.prefix_len)?;
        let current = stack[depth].as_mut().ok_or(CoreError::Truncated)?;
        current.entry_count = current
            .entry_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        current.cursor = group_end;
        if has_descendants {
            let child_prefix = frame
                .prefix_len
                .checked_add(component.len())
                .and_then(|value| value.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            let child_depth = depth.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            if child_depth >= stack.len() {
                return Err(CoreError::CountCap);
            }
            stack[child_depth] = Some(ManifestPreflightFrameV1::new(
                cursor,
                group_end,
                child_prefix,
            ));
            depth = child_depth;
        } else if group_end != cursor + 1 {
            return Err(CoreError::Path);
        }
    }
}

struct ManifestBuildFrameV1<'path> {
    end: usize,
    prefix_len: usize,
    cursor: usize,
    mode: DirectoryBuildModeV1,
    component_in_parent: Option<ValidatedComponent<'path>>,
    entries: Vec<CanonicalTreeEntryV1<'path>>,
}

fn manifest_build_stack_resident_bytes_v1() -> CoreResult<u64> {
    let frame_bytes = u64::try_from(core::mem::size_of::<ManifestBuildFrameV1<'static>>())
        .map_err(|_| CoreError::IntegerOverflow)?;
    let capacity =
        u64::try_from(MANIFEST_BUILD_STACK_CAPACITY_V1).map_err(|_| CoreError::IntegerOverflow)?;
    frame_bytes
        .checked_mul(capacity)
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<Vec<ManifestBuildFrameV1<'static>>>() as u64)
        })
        .ok_or(CoreError::IntegerOverflow)
}

fn manifest_directory_entry_count_v1<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<usize> {
    let mut cursor = start;
    let mut entry_count = 0_usize;
    while cursor < end {
        entry_count = entry_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        cursor = directory_group_end(files, cursor, end, prefix_len)?;
    }
    Ok(entry_count)
}

fn new_manifest_build_frame_v1<'path, S>(
    files: &[TreeFileV1<'path, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
    mode: DirectoryBuildModeV1,
    component_in_parent: Option<ValidatedComponent<'path>>,
) -> CoreResult<ManifestBuildFrameV1<'path>> {
    let entry_count = manifest_directory_entry_count_v1(files, start, end, prefix_len)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(entry_count)
        .map_err(|_| CoreError::ResourceRefused)?;
    if entries.capacity() > entry_count {
        return Err(CoreError::ResourceRefused);
    }
    Ok(ManifestBuildFrameV1 {
        end,
        prefix_len,
        cursor: start,
        mode,
        component_in_parent,
        entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_manifest_directory_v1<'path, S, P>(
    files: &[TreeFileV1<'path, S>],
    storage: &mut P,
    start: usize,
    end: usize,
    prefix_len: usize,
    mode: DirectoryBuildModeV1,
    reservation: &crate::limits::OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<crate::cow::CanonicalDirectoryTreeV1>
where
    P: StorageSessionPortV1 + ?Sized,
{
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(MANIFEST_BUILD_STACK_CAPACITY_V1)
        .map_err(|_| CoreError::ResourceRefused)?;
    if frames.capacity() > MANIFEST_BUILD_STACK_CAPACITY_V1 {
        return Err(CoreError::ResourceRefused);
    }
    frames.push(new_manifest_build_frame_v1(
        files, start, end, prefix_len, mode, None,
    )?);
    loop {
        let frame_index = frames.len().checked_sub(1).ok_or(CoreError::Truncated)?;
        let cursor = frames[frame_index].cursor;
        let frame_end = frames[frame_index].end;
        let frame_prefix_len = frames[frame_index].prefix_len;
        if cursor == frame_end {
            let frame = frames.pop().ok_or(CoreError::Truncated)?;
            let directory = build_canonical_directory_borrowed_v1(
                frame.mode,
                &frame.entries,
                storage.tree_sink_v1(),
                reservation,
                counters,
                object_scratch,
                page_scratch,
            )?;
            storage.push_built_directory_v1(BuiltDirectoryRecordV1 {
                physical: directory.physical(),
                entry_count: directory.entry_count(),
            })?;
            let Some(parent_component) = frame.component_in_parent else {
                if frames.is_empty() {
                    return Ok(directory);
                }
                return Err(CoreError::Truncated);
            };
            let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
                return Err(CoreError::TypeDomain);
            };
            frames
                .last_mut()
                .ok_or(CoreError::Truncated)?
                .entries
                .push(CanonicalTreeEntryV1::new(
                    parent_component,
                    CanonicalTreeChildV1::Directory {
                        logical,
                        physical: directory.physical(),
                    },
                ));
            continue;
        }
        if cursor > frame_end {
            return Err(CoreError::Truncated);
        }
        let (component_bytes, has_descendants) =
            path_component_at(files[cursor].path, frame_prefix_len)?;
        let component = ValidatedComponent::new(component_bytes)?;
        let group_end = directory_group_end(files, cursor, frame_end, frame_prefix_len)?;
        frames[frame_index].cursor = group_end;
        if has_descendants {
            let child_prefix = frame_prefix_len
                .checked_add(component_bytes.len())
                .and_then(|value| value.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            if frames.len() >= MAX_PATH_DEPTH.saturating_add(1) {
                return Err(CoreError::CountCap);
            }
            frames.push(new_manifest_build_frame_v1(
                files,
                cursor,
                group_end,
                child_prefix,
                DirectoryBuildModeV1::Explicit(DEFAULT_EXPLICIT_DIRECTORY_MODE),
                Some(component),
            )?);
        } else {
            if group_end != cursor + 1 {
                return Err(CoreError::Path);
            }
            let ordinal = u32::try_from(cursor).map_err(|_| CoreError::IntegerOverflow)?;
            let file = storage.read_built_file_v1(ordinal)?;
            frames[frame_index].entries.push(CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: file.logical,
                    physical: file.physical,
                },
            ));
        }
    }
}

/// Build one complete, bounded, canonically ordered candidate root containing
/// zero or more files. All manifest validation and the sole operation-slot
/// reservation happen before the first supplier is invoked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_create_tree_v1<S, C>(
    mut operation: StorageOperationV1<'_>,
    algorithm: CdcAlgorithmV1,
    files: &mut [TreeFileV1<'_, S>],
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: SourceSupplierV1,
    C: LifecycleControlV1 + ?Sized,
{
    let (
        canonical_len,
        global_seen_capacity,
        maximum_records_u32,
        maximum_source_resident,
        storage_resident,
    ) = operation.run_preparation_free_stage_v1(
        counters,
        control,
        |operation, counters, _control| {
            // The split request/run API must not let an in-crate caller
            // replay a live grant for another kind. The zero-write lease
            // gives every later terminal path a directly observed
            // storage equation before request/manifest inspection.
            operation.require_complete_tree_kind_v1()?;
            operation.declare_empty_storage_envelope_v1()?;
            validate_entry_count(files.len() as u64)?;
            validate_file_mode(DEFAULT_EXPLICIT_DIRECTORY_MODE)?;
            let mut canonical_len = 0_u64;
            let mut maximum_refs_per_version = 0_u64;
            let mut previous = None;
            for file in files.iter() {
                let path = ValidatedPath::new(file.path)?;
                if let Some(left) = previous {
                    require_strictly_increasing_paths(left, path)?;
                    if file.path.len() > left.as_bytes().len()
                        && file.path.starts_with(left.as_bytes())
                        && file.path[left.as_bytes().len()] == b'/'
                    {
                        return Err(CoreError::Path.into());
                    }
                }
                previous = Some(path);
                validate_file_mode(file.mode)?;
                validate_logical_length(file.declared_len)?;
                canonical_len = canonical_len
                    .checked_add(file.declared_len)
                    .ok_or(CoreError::IntegerOverflow)?;
                validate_logical_length(canonical_len)?;
                let refs = file
                    .declared_len
                    .checked_add(8_191)
                    .ok_or(CoreError::IntegerOverflow)?
                    / 8_192;
                validate_chunk_refs_per_file(refs)?;
                maximum_refs_per_version = maximum_refs_per_version
                    .checked_add(refs)
                    .ok_or(CoreError::IntegerOverflow)?;
                validate_chunk_refs_per_version(maximum_refs_per_version)?;
            }
            let tree_preflight = preflight_manifest_directory_v1(files, 0, files.len(), 0)?;
            validate_entry_count(tree_preflight.directory_entry_count)?;
            validate_tree_object_count(tree_preflight.tree_object_count)?;
            let maximum_objects = maximum_refs_per_version
                .checked_add(files.len() as u64)
                .and_then(|count| count.checked_add(tree_preflight.tree_object_count))
                .and_then(|count| count.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            validate_total_object_count(maximum_objects)?;
            let global_seen_capacity = global_seen_capacity_v1(maximum_objects)?;
            let maximum_records = maximum_objects.min(MAX_STORAGE_RECORDS_V1);
            let maximum_records_u32 =
                u32::try_from(maximum_records).map_err(|_| CoreError::IntegerOverflow)?;
            let required_traversal_bytes = closure_traversal_bytes_v1(maximum_objects)?;
            if buffers.traversal_state.len() < required_traversal_bytes
                || buffers.tree_pages.len()
                    < usize::try_from(tree_preflight.maximum_page_summary_count)
                        .map_err(|_| CoreError::IntegerOverflow)?
            {
                return Err(CoreError::ResourceRefused.into());
            }

            operation.declare_storage_envelope_v1(storage_envelope_v1(
                maximum_objects,
                maximum_objects,
                maximum_refs_per_version,
                files.len() as u64,
                tree_preflight.tree_object_count,
                canonical_len,
                global_seen_capacity,
                true,
            )?)?;

            // Query suppliers only after the final conservative envelope
            // is admitted. These callbacks create no preparation state.
            let mut maximum_source_resident = 0_u64;
            for file in files.iter() {
                let supplier = file.supplier.as_ref().ok_or(CoreError::SourceFailure)?;
                maximum_source_resident =
                    maximum_source_resident.max(supplier.resident_memory_bound_bytes()?);
            }
            let storage_resident = operation.storage_resident_plan_v1(true, maximum_records_u32)?;
            // `files` and its path bytes are caller-owned immutable
            // manifest input. Charge LayerFS-created views and borrowed
            // ports without relabelling caller storage as slot allocation.
            let port_resident = maximum_source_resident
                .checked_add(storage_resident.total_resident_bytes_v1())
                .ok_or(CoreError::IntegerOverflow)?;
            // Manifest entry construction and authenticated closure
            // reconstruction are sequential phases. The manifest vectors,
            // their bounded directory stack, and the page-summary buffer are
            // all gone from active tree-building work before the closure
            // admission stack is created. Charge the exact larger phase peak
            // in their shared traversal component instead of adding both
            // mutually exclusive peaks. Persistent storage adapters remain in
            // `port_resident` and are therefore still charged across phases.
            let tree_build_resident = tree_preflight
                .peak_entry_memory
                .checked_add(manifest_build_stack_resident_bytes_v1()?)
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of_val(buffers.tree_pages) as u64)
                })
                .ok_or(CoreError::IntegerOverflow)?;
            let traversal_phase_resident =
                admission_traversal_resident_bytes_v1()?.max(tree_build_resident);
            let plan = OperationMemoryPlanV1::empty()
                .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
                .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
                .charge(
                    MemoryComponentV1::ComparisonWindow,
                    (2 * COMPARISON_WINDOW_BYTES) as u64,
                )?
                .charge(
                    MemoryComponentV1::ObjectScratch,
                    buffers.tree_object.len() as u64,
                )?
                .charge(MemoryComponentV1::PageSummaries, traversal_phase_resident)?
                .charge(
                    MemoryComponentV1::TraversalState,
                    buffers.traversal_state.len() as u64,
                )?
                .charge(
                    MemoryComponentV1::MetadataWindow,
                    port_resident.max(DEFAULT_METADATA_RESERVATION_BYTES),
                )?
                .charge(
                    MemoryComponentV1::HashState,
                    IDENTITY_HASHER_BYTES_V1
                        .checked_mul(2)
                        .ok_or(CoreError::IntegerOverflow)?,
                )?;
            operation.declare_plan_v1(plan)?;
            counters.memory_high_water = counters
                .memory_high_water
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                canonical_len,
                global_seen_capacity,
                maximum_records_u32,
                maximum_source_resident,
                storage_resident,
            ))
        },
    )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: true,
            maximum_records: maximum_records_u32,
            algorithm,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let mut reference_spool_bytes = OptionalU64ObservationV1::observed(
                0,
                "direct cumulative chunk-reference spool logical length",
                ObservationScopeV1::Operation,
            );
            let (_, sink) = storage.content_parts_v1();
            sink.begin_closure().map_err(|_| CoreError::SinkRefused)?;
            for file in files.iter_mut() {
                let supplier = file.supplier.take().ok_or(CoreError::SourceFailure)?;
                let declared_resident = supplier.resident_memory_bound_bytes()?;
                let mut source = supplier.supply()?;
                if source.resident_memory_bound_bytes()? > declared_resident
                    || declared_resident > maximum_source_resident
                {
                    return Err(CoreError::ResourceRefused.into());
                }
                let mut cdc_control = SharedOperationControlV1::new(control_cell);
                let (references, sink) = storage.content_parts_v1();
                let prepared = create_file_borrowed_v1(
                    file.path,
                    file.mode,
                    file.declared_len,
                    &mut source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut cdc_control,
                    reservation,
                    algorithm,
                    counters,
                )?;
                reference_spool_bytes = reference_spool_bytes.checked_add_operation_v1(
                    storage.reference_storage_bytes_v1()?,
                    "direct cumulative chunk-reference spool logical length",
                )?;
                storage.push_built_file_v1(BuiltFileRecordV1 {
                    logical: derive_file_node_v1(file.mode, prepared.logical_file())?,
                    physical: prepared.physical_file(),
                    logical_len: file.declared_len,
                    chunk_count: prepared.chunk_count(),
                    extent_count: u8::from(file.declared_len != 0),
                })?;
            }
            let tree = build_manifest_directory_v1(
                files,
                storage,
                0,
                files.len(),
                0,
                DirectoryBuildModeV1::ImplicitRoot,
                reservation,
                counters,
                buffers.tree_object,
                buffers.tree_pages,
            )?;
            let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = tree.logical() else {
                return Err(CoreError::TypeDomain.into());
            };
            let summary = storage.built_version_summary_v1(
                canonical_len,
                counters,
                &mut SharedOperationControlV1::new(control_cell),
            )?;
            let version = storage.write_version_v1(
                derive_version_v1(logical_root),
                tree.physical(),
                summary,
                counters,
            )?;
            let completed = storage.complete_v1(version)?;
            Ok(PreparedCandidateV1::new(
                version,
                tree.physical(),
                completed,
                reference_spool_bytes,
            ))
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::{
        global_seen_hash_v1, FileGlobalSeenSpoolV1, FsCasBoundaryV1, FsCasControlV1, FsCasErrorV1,
        FsCasV1, FsOperationKindV1, GlobalSeenErrorV1, GlobalSeenRecordV1,
        GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1, GLOBAL_SEEN_RECORD_BYTES,
    };
    use crate::cdc::{CdcControlV1, MAXIMUM_CHUNK_BYTES};
    use crate::format::PhysicalObjectKindV1;
    use crate::lifecycle::{request_create_operation_v1, request_tree_operation_v1};
    use crate::object::TypedPhysicalObjectIdV1;
    use crate::pack::MAX_PACK_BYTES;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, AtomicOrdering::Relaxed);
            let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory");
            Self(parent.join(format!(
                "layerfs-private-tree-test-{}-{sequence:016x}",
                std::process::id()
            )))
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct FragmentedSource<'a> {
        bytes: &'a [u8],
        offset: usize,
        maximum_read: usize,
    }

    impl ContentSourceV1 for FragmentedSource<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(
            &mut self,
            destination: &mut [u8],
        ) -> Result<usize, crate::content::ContentSourceErrorV1> {
            let take = destination
                .len()
                .min(self.maximum_read)
                .min(self.bytes.len() - self.offset);
            destination[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
            self.offset += take;
            Ok(take)
        }
    }

    struct FragmentedSupplier<'a> {
        bytes: &'a [u8],
        maximum_read: usize,
        cas: &'a FsCasV1,
    }

    struct EmptyShapeSupplier<'a> {
        calls: &'a AtomicU64,
        cas: &'a FsCasV1,
    }

    impl<'a> SourceSupplierV1 for EmptyShapeSupplier<'a> {
        type Source = FragmentedSource<'static>;

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<FragmentedSource<'static>>() as u64)
        }

        fn supply(self) -> CoreResult<Self::Source> {
            assert_eq!(self.cas.operation_admitted_slots_v1(), 1);
            self.calls.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(FragmentedSource {
                bytes: &[],
                offset: 0,
                maximum_read: 1,
            })
        }
    }

    impl<'a> SourceSupplierV1 for FragmentedSupplier<'a> {
        type Source = FragmentedSource<'a>;

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<FragmentedSource<'_>>() as u64)
        }

        fn supply(self) -> CoreResult<Self::Source> {
            assert_eq!(self.cas.operation_admitted_slots_v1(), 1);
            Ok(FragmentedSource {
                bytes: self.bytes,
                offset: 0,
                maximum_read: self.maximum_read,
            })
        }
    }

    #[derive(Default)]
    struct ContinueControl;

    impl CdcControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct PreparationNamespaceControlV1 {
        preparation: PathBuf,
        carrier_phases: Vec<Vec<String>>,
        marker_phases: Vec<Vec<String>>,
        receipt_spool_high_water: u64,
        receipt_spool_name: Option<String>,
        carrier_receipt_starts: Vec<u64>,
        carrier_receipt_high_waters: Vec<u64>,
    }

    const LOCATOR_RECEIPT_RECORD_BYTES_V1: u64 = 184;

    impl PreparationNamespaceControlV1 {
        fn new(root: &std::path::Path) -> Self {
            Self {
                preparation: root.join("preparation"),
                carrier_phases: Vec::new(),
                marker_phases: Vec::new(),
                receipt_spool_high_water: 0,
                receipt_spool_name: None,
                carrier_receipt_starts: Vec::new(),
                carrier_receipt_high_waters: Vec::new(),
            }
        }

        fn snapshot_v1(&mut self, carrier_start: bool) -> Vec<String> {
            let mut receipt_spool = None;
            let mut names = fs::read_dir(&self.preparation)
                .expect("preparation directory")
                .map(|entry| {
                    let entry = entry.expect("preparation entry");
                    let name = entry
                        .file_name()
                        .into_string()
                        .expect("ASCII preparation name");
                    if name.starts_with("locator-receipts-") {
                        let len = entry.metadata().expect("receipt metadata").len();
                        assert!(receipt_spool.replace((name.clone(), len)).is_none());
                        self.receipt_spool_high_water = self.receipt_spool_high_water.max(len);
                    }
                    name
                })
                .collect::<Vec<_>>();
            names.sort();
            let (receipt_spool_name, receipt_spool_len) =
                receipt_spool.expect("one operation-owned locator receipt spool");
            assert_eq!(
                receipt_spool_len % LOCATOR_RECEIPT_RECORD_BYTES_V1,
                0,
                "every physical receipt-spool observation is record-aligned"
            );
            match self.receipt_spool_name.as_deref() {
                Some(expected) => assert_eq!(receipt_spool_name, expected),
                None => self.receipt_spool_name = Some(receipt_spool_name),
            }
            if carrier_start {
                self.carrier_receipt_starts.push(receipt_spool_len);
                self.carrier_receipt_high_waters.push(receipt_spool_len);
            } else {
                let high_water = self
                    .carrier_receipt_high_waters
                    .last_mut()
                    .expect("carrier start precedes locator publication");
                *high_water = (*high_water).max(receipt_spool_len);
            }
            names
        }

        fn assert_exact_namespace_lifetime_v1(
            &self,
            expected_high_water: u64,
            counters: &OperationCountersV1,
        ) {
            assert_eq!(
                counters.storage_preparation_inodes_high_water, expected_high_water,
                "compatibility counter measures logical namespace entries"
            );
            assert!(!self.carrier_phases.is_empty());
            assert!(!self.marker_phases.is_empty());
            for carrier in &self.carrier_phases {
                assert_eq!(carrier.len() as u64, expected_high_water);
                for marker in &self.marker_phases {
                    assert_eq!(marker.len() as u64, expected_high_water);
                    assert_eq!(
                        carrier.iter().filter(|name| marker.contains(name)).count() as u64,
                        expected_high_water - 1,
                        "the private carrier and private marker names are phase-local"
                    );
                }
            }
            assert_eq!(
                self.receipt_spool_high_water,
                counters.locator_installs * LOCATOR_RECEIPT_RECORD_BYTES_V1,
                "one real file-backed receipt record per installed locator"
            );
        }

        fn assert_receipt_spool_reused_across_carriers_v1(&self, counters: &OperationCountersV1) {
            assert!(self.carrier_receipt_starts.len() >= 2);
            assert_eq!(
                self.carrier_receipt_starts.len(),
                self.carrier_receipt_high_waters.len()
            );
            assert_eq!(self.carrier_receipt_starts[0], 0);
            assert_eq!(
                self.carrier_receipt_starts[1], 0,
                "the same operation spool is reset before carrier two"
            );
            assert!(self.carrier_receipt_high_waters[0] > 0);
            assert!(self.carrier_receipt_high_waters[1] > 0);

            let maximum_carrier_receipts = self
                .carrier_receipt_high_waters
                .iter()
                .copied()
                .max()
                .unwrap()
                / LOCATOR_RECEIPT_RECORD_BYTES_V1;
            assert_eq!(
                self.receipt_spool_high_water,
                maximum_carrier_receipts * LOCATOR_RECEIPT_RECORD_BYTES_V1
            );
            let observed_operation_receipts = self
                .carrier_receipt_high_waters
                .iter()
                .map(|bytes| bytes / LOCATOR_RECEIPT_RECORD_BYTES_V1)
                .sum::<u64>();
            assert_eq!(observed_operation_receipts, counters.locator_installs);
            assert!(
                self.receipt_spool_high_water
                    < counters.locator_installs * LOCATOR_RECEIPT_RECORD_BYTES_V1,
                "physical receipt storage is one carrier maximum, not the operation total"
            );
        }
    }

    impl CdcControlV1 for PreparationNamespaceControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationNamespaceControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            match boundary {
                FsCasBoundaryV1::BeforeCarrierInstall => {
                    let snapshot = self.snapshot_v1(true);
                    self.carrier_phases.push(snapshot);
                }
                FsCasBoundaryV1::AfterObjectLocatorMarkerLink => {
                    let snapshot = self.snapshot_v1(false);
                    self.marker_phases.push(snapshot);
                }
                _ => {}
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct CancelGlobalSeenControl;

    impl FsCasControlV1 for CancelGlobalSeenControl {
        fn cancellation_requested(&mut self) -> bool {
            true
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct CorruptFirstPublishedCarrierControl {
        objects: PathBuf,
        corrupted_locator_count: u64,
        corrupted: bool,
    }

    impl CdcControlV1 for CorruptFirstPublishedCarrierControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CorruptFirstPublishedCarrierControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary != FsCasBoundaryV1::AfterCatalogPublication || self.corrupted {
                return;
            }
            self.corrupted = true;
            for entry in fs::read_dir(&self.objects).expect("published object directory") {
                let path = entry.expect("published object locator").path();
                let mut bytes = fs::read(&path).expect("read published locator");
                assert_eq!(&bytes[..8], b"LFSOBJ01");
                bytes[..8].copy_from_slice(b"CORRUPT!");
                fs::remove_file(&path).expect("remove locator for injected replacement");
                let mut replacement = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .expect("create corrupt locator replacement");
                replacement
                    .write_all(&bytes)
                    .expect("write corrupt locator replacement");
                let mut permissions = replacement
                    .metadata()
                    .expect("corrupt locator metadata")
                    .permissions();
                permissions.set_readonly(true);
                replacement
                    .set_permissions(permissions)
                    .expect("make corrupt locator replacement immutable");
                self.corrupted_locator_count += 1;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn non_tree_operation_reaches_six_live_preparation_namespace_entries() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let bytes = b"namespace-envelope";
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_create_operation_v1(&cas, 6, &mut counters, &mut control).unwrap();

        let handoff = run_create_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            b"file.bin",
            0o644,
            bytes.len() as u64,
            FragmentedSupplier {
                bytes,
                maximum_read: bytes.len(),
                cas: &cas,
            },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert_eq!(handoff.carrier_count(), 1);
        control.assert_exact_namespace_lifetime_v1(6, &counters);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation")).unwrap().count(),
            0
        );
    }

    #[test]
    fn tree_operation_reaches_eight_live_preparation_namespace_entries() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let bytes = b"tree-namespace-envelope";
        let mut files = [TreeFileV1::new(
            b"file.bin",
            0o644,
            bytes.len() as u64,
            FragmentedSupplier {
                bytes,
                maximum_read: bytes.len(),
                cas: &cas,
            },
        )];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_tree_operation_v1(&cas, 8, &mut counters, &mut control).unwrap();

        let handoff = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert_eq!(handoff.carrier_count(), 1);
        control.assert_exact_namespace_lifetime_v1(8, &counters);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation")).unwrap().count(),
            0
        );
    }

    #[test]
    fn global_seen_file_table_has_exact_bounded_collision_work() {
        const CAPACITY: u32 = 64;
        const COLLIDING_IDS: usize = 16;

        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let mut operation_control = ContinueControl;
        let mut operation_counters = OperationCountersV1::default();
        let operation = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0,
                &mut operation_counters,
                &mut operation_control,
            )
            .expect("reserve the root-owned operation first");
        let mut table = FileGlobalSeenSpoolV1::new(
            cas.begin_operation_spool_v1("global-seen-test", &mut operation_control)
                .expect("create operation spool"),
        );
        table.initialize(CAPACITY).expect("initialize table");

        let mut ids = Vec::with_capacity(COLLIDING_IDS);
        let mut candidate = 0_u64;
        while ids.len() < COLLIDING_IDS {
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&candidate.to_be_bytes());
            let id =
                TypedPhysicalObjectIdV1::from_kind_and_digest(PhysicalObjectKindV1::Chunk, digest);
            if global_seen_hash_v1(id) & u64::from(CAPACITY - 1) == 0 {
                ids.push(id);
            }
            candidate = candidate.checked_add(1).expect("bounded search");
        }

        let mut control = ContinueControl;
        for (ordinal, id) in ids.iter().copied().enumerate() {
            let lookup = table.lookup(id, &mut control).expect("vacant lookup");
            assert!(lookup.record.is_none());
            assert_eq!(lookup.vacant_slot, ordinal as u32);
            table
                .insert(
                    lookup.vacant_slot,
                    id,
                    GlobalSeenRecordV1 {
                        complete_len: 52,
                        private_payload_offset: ordinal as u64,
                        carrier_ordinal: 0,
                    },
                )
                .expect("insert");
        }
        for (ordinal, id) in ids.iter().copied().enumerate() {
            let lookup = table.lookup(id, &mut control).expect("occupied lookup");
            let record = lookup.record.expect("record");
            assert_eq!(record.complete_len, 52);
            assert_eq!(record.private_payload_offset, ordinal as u64);
            assert_eq!(record.carrier_ordinal, 0);
        }

        let triangular = (COLLIDING_IDS * (COLLIDING_IDS + 1) / 2) as u64;
        assert_eq!(
            table.work_observation(),
            (
                (COLLIDING_IDS * 2) as u64,
                triangular * 2,
                COLLIDING_IDS as u32,
                COLLIDING_IDS as u32
            )
        );
        assert_eq!(
            table.direct_storage_observation(),
            (
                triangular * 2 * GLOBAL_SEEN_RECORD_BYTES,
                triangular * 2,
                COLLIDING_IDS as u64 * GLOBAL_SEEN_RECORD_BYTES,
            )
        );
        assert_eq!(
            table.storage_bytes(),
            u64::from(CAPACITY) * GLOBAL_SEEN_RECORD_BYTES
        );
        table
            .cleanup_controlled_v1(&mut ContinueControl)
            .expect("controlled cleanup");
        drop(operation);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
    }

    #[test]
    fn global_seen_adversarial_cluster_stops_at_frozen_probe_budget_and_polls_first() {
        const CAPACITY: u32 = 1_024;
        const COLLIDING_IDS: usize = GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1 as usize + 1;

        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let mut operation_control = ContinueControl;
        let mut operation_counters = OperationCountersV1::default();
        let operation = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0,
                &mut operation_counters,
                &mut operation_control,
            )
            .expect("reserve the root-owned operation first");
        let mut table = FileGlobalSeenSpoolV1::new(
            cas.begin_operation_spool_v1("global-seen-budget-test", &mut operation_control)
                .expect("create operation spool"),
        );
        table.initialize(CAPACITY).expect("initialize table");

        let mut ids = Vec::with_capacity(COLLIDING_IDS);
        let mut candidate = 0_u64;
        while ids.len() < COLLIDING_IDS {
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&candidate.to_be_bytes());
            let id =
                TypedPhysicalObjectIdV1::from_kind_and_digest(PhysicalObjectKindV1::Chunk, digest);
            if global_seen_hash_v1(id) & u64::from(CAPACITY - 1) == 0 {
                ids.push(id);
            }
            candidate = candidate.checked_add(1).expect("bounded search");
        }

        let mut control = ContinueControl;
        for (ordinal, id) in ids
            .iter()
            .copied()
            .take(GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1 as usize)
            .enumerate()
        {
            let lookup = table.lookup(id, &mut control).expect("bounded lookup");
            assert!(lookup.record.is_none());
            table
                .insert(
                    lookup.vacant_slot,
                    id,
                    GlobalSeenRecordV1 {
                        complete_len: 52,
                        private_payload_offset: ordinal as u64,
                        carrier_ordinal: 0,
                    },
                )
                .expect("bounded insert");
        }
        assert!(matches!(
            table.lookup(ids[COLLIDING_IDS - 1], &mut control),
            Err(GlobalSeenErrorV1::Core(CoreError::CountCap))
        ));
        assert_eq!(
            table.maximum_probe,
            GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1
        );

        let reads_before_cancel = table.direct_storage_observation();
        let mut cancelled = CancelGlobalSeenControl;
        assert!(matches!(
            table.lookup(ids[0], &mut cancelled),
            Err(GlobalSeenErrorV1::Core(CoreError::Cancelled))
        ));
        assert_eq!(table.direct_storage_observation(), reads_before_cancel);
        table
            .cleanup_controlled_v1(&mut ContinueControl)
            .expect("controlled cleanup");
        drop(operation);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
    }

    fn deterministic_bytes(len: usize, mut state: u64) -> Vec<u8> {
        let mut bytes = vec![0_u8; len];
        for destination in bytes.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            destination.copy_from_slice(&state.to_le_bytes()[..destination.len()]);
        }
        bytes
    }

    #[test]
    fn global_seen_reuses_equal_objects_across_real_carrier_rollover() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let shared = deterministic_bytes(256 * 1024 + 19, 0x61d3_75a4_9bf2_08ce);
        let large = deterministic_bytes(
            usize::try_from(MAX_PACK_BYTES + 2 * 1024 * 1024).expect("bounded fixture"),
            0x8a51_e349_c072_6dbf,
        );
        let mut files = [
            TreeFileV1::new(
                b"a-shared.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"b-large.bin",
                0o644,
                large.len() as u64,
                FragmentedSupplier {
                    bytes: &large,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"z-shared-again.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
        ];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_tree_operation_v1(&cas, 1, &mut counters, &mut control).unwrap();

        let result = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert!(result.carrier_count() >= 2, "real rollover was required");
        control.assert_receipt_spool_reused_across_carriers_v1(&counters);
        assert_eq!(
            counters.carrier_rollovers,
            u64::from(result.carrier_rollovers())
        );
        assert!(counters.global_seen_cross_carrier_reuses > 0);
        assert_eq!(
            counters.global_seen_lookups,
            counters.physical_objects_created + counters.physical_objects_reused
        );
        assert_eq!(
            counters.global_seen_entries,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.pack_local_objects_created + counters.pack_local_objects_reused,
            counters.physical_objects_created + counters.physical_objects_reused
        );
        assert_eq!(
            counters.physical_carrier_object_writes,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.locator_installs,
            counters.physical_carrier_object_writes
        );
        assert_eq!(counters.locator_equal_incumbent_reuses, 0);
        assert_eq!(
            counters.version_objects_created
                + counters.tree_objects_created
                + counters.file_objects_created
                + counters.symlink_objects_created
                + counters.chunk_objects_created,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.version_objects_reused
                + counters.tree_objects_reused
                + counters.file_objects_reused
                + counters.symlink_objects_reused
                + counters.chunk_objects_reused,
            counters.pack_local_objects_reused
        );
        assert!(counters.global_seen_probes >= counters.global_seen_lookups);
        assert!(counters.global_seen_maximum_probe > 0);
        assert!(counters.global_seen_metadata_read_calls > 0);
        assert_eq!(
            counters.global_seen_metadata_bytes_read,
            counters.global_seen_probes * GLOBAL_SEEN_RECORD_BYTES
        );
        assert_eq!(
            counters.global_seen_metadata_bytes_written,
            counters.global_seen_entries * GLOBAL_SEEN_RECORD_BYTES
        );
        assert!(counters.carrier_bytes_total > MAX_PACK_BYTES);
        assert!(counters.maximum_active_carrier_bytes <= MAX_PACK_BYTES);
        assert!(counters.final_carrier_bytes <= MAX_PACK_BYTES);
        assert_eq!(counters.closure_objects_missing, 0);
        assert_eq!(
            counters.closure_objects_occupied_validated,
            result.object_count()
        );
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters
                .storage_bytes_released
                .checked_add(counters.storage_bytes_committed)
                .and_then(|value| value.checked_add(counters.storage_bytes_retained))
                .unwrap()
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters
                .storage_inodes_released
                .checked_add(counters.storage_inodes_committed)
                .and_then(|value| value.checked_add(counters.storage_inodes_retained))
                .unwrap()
        );
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
    }

    #[test]
    fn cross_carrier_occupied_corruption_remains_a_typed_fscas_error() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let shared = deterministic_bytes(256 * 1024 + 19, 0x61d3_75a4_9bf2_08ce);
        let large = deterministic_bytes(
            usize::try_from(MAX_PACK_BYTES + 2 * 1024 * 1024).expect("bounded fixture"),
            0x8a51_e349_c072_6dbf,
        );
        let mut files = [
            TreeFileV1::new(
                b"a-shared.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"b-large.bin",
                0o644,
                large.len() as u64,
                FragmentedSupplier {
                    bytes: &large,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"z-shared-again.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
        ];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = CorruptFirstPublishedCarrierControl {
            objects: fixture.0.join("objects"),
            corrupted_locator_count: 0,
            corrupted: false,
        };
        let operation = request_tree_operation_v1(&cas, 2, &mut counters, &mut control).unwrap();

        let result = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::MalformedOccupant))
        );
        assert!(control.corrupted_locator_count > 0);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(fixture.0.join("closures"))
                .expect("closure directory")
                .count(),
            0
        );
    }

    #[test]
    fn complete_tree_shapes_cover_100_1024_10240_and_25600_files() {
        for file_count in [100_usize, 1_024, 10_240, 25_600] {
            let fixture = TestRoot::new();
            let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
            let supplier_calls = AtomicU64::new(0);
            let paths = (0..file_count)
                .map(|ordinal| format!("f-{ordinal:05}.empty").into_bytes())
                .collect::<Vec<_>>();
            let mut files = paths
                .iter()
                .map(|path| {
                    TreeFileV1::new(
                        path,
                        0o644,
                        0,
                        EmptyShapeSupplier {
                            calls: &supplier_calls,
                            cas: &cas,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut counters = OperationCountersV1::default();
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages =
                vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = vec![0_u8; 64 * 1024];
            let mut control = ContinueControl;
            let operation =
                request_tree_operation_v1(&cas, 3, &mut counters, &mut control).unwrap();

            let result = run_create_tree_v1(
                operation,
                CdcAlgorithmV1::FastCdc,
                &mut files,
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_or_else(|error| panic!("{file_count} files: {error:?}; {counters:#?}"));

            assert_eq!(
                supplier_calls.load(AtomicOrdering::Relaxed),
                file_count as u64
            );
            assert_eq!(result.carrier_count(), 1);
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.closure_objects_missing, 0);
            assert_eq!(
                counters.closure_objects_occupied_validated,
                result.object_count()
            );
            assert_eq!(
                counters.pack_local_objects_created + counters.pack_local_objects_reused,
                counters.physical_objects_created + counters.physical_objects_reused
            );
            assert_eq!(
                counters.physical_carrier_object_writes,
                counters.pack_local_objects_created
            );
            assert_eq!(
                counters.locator_installs,
                counters.physical_carrier_object_writes
            );
            assert_eq!(counters.locator_equal_incumbent_reuses, 0);
            assert_eq!(
                counters.version_objects_created
                    + counters.version_objects_reused
                    + counters.tree_objects_created
                    + counters.tree_objects_reused
                    + counters.file_objects_created
                    + counters.file_objects_reused
                    + counters.symlink_objects_created
                    + counters.symlink_objects_reused
                    + counters.chunk_objects_created
                    + counters.chunk_objects_reused,
                counters.pack_local_objects_created + counters.pack_local_objects_reused
            );
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(
                fs::read_dir(fixture.0.join("preparation"))
                    .expect("preparation directory")
                    .count(),
                0
            );
        }
    }

    #[test]
    fn private_multi_entry_candidate_root_is_complete_and_fragmentation_independent() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let alpha = vec![0x11; 17 * 1024 + 3];
        let beta = vec![0x22; 41 * 1024 + 5];
        let gamma = vec![0x33; 9 * 1024 + 7];
        let omega = vec![0x44; 65 * 1024 + 11];
        let mut expected_root = None;

        for maximum_read in [1, MAXIMUM_CHUNK_BYTES] {
            let mut counters = OperationCountersV1::default();
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = vec![0_u8; 64 * 1024];
            let mut control = ContinueControl;
            let mut files = [
                TreeFileV1::new(
                    b"alpha.bin",
                    0o644,
                    alpha.len() as u64,
                    FragmentedSupplier {
                        bytes: &alpha,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"dir/beta.bin",
                    0o600,
                    beta.len() as u64,
                    FragmentedSupplier {
                        bytes: &beta,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"dir/nested/gamma.bin",
                    0o644,
                    gamma.len() as u64,
                    FragmentedSupplier {
                        bytes: &gamma,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"omega.bin",
                    0o644,
                    omega.len() as u64,
                    FragmentedSupplier {
                        bytes: &omega,
                        maximum_read,
                        cas: &cas,
                    },
                ),
            ];
            let operation =
                request_tree_operation_v1(&cas, 4, &mut counters, &mut control).unwrap();
            let result = run_create_tree_v1(
                operation,
                CdcAlgorithmV1::FastCdc,
                &mut files,
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

            if let Some(root) = expected_root {
                assert_eq!(result.root_tree(), root);
                assert_eq!(result.carriers_reused(), result.carrier_count());
            } else {
                expected_root = Some(result.root_tree());
                assert_eq!(result.carriers_installed(), result.carrier_count());
            }
            assert!(result.object_count() >= 12);
            assert_eq!(
                counters.source_bytes_read,
                (alpha.len() + beta.len() + gamma.len() + omega.len()) as u64
            );
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(
                fs::read_dir(fixture.0.join("preparation"))
                    .expect("preparation directory")
                    .count(),
                0
            );
        }
    }
}
