//! Exact evaluator-local aggregation for the frozen legacy evidence counters.

use super::{NativeOperationCounters, OperationCounters};
use layerfs_core::content::rope::RopeCounters;
use layerfs_core::{CoreError, CoreError::LengthOverflow};
use layerfs_storage::scratch::ScratchObservation;

pub(crate) fn add_scratch(
    counters: &mut OperationCounters,
    source: ScratchObservation,
) -> Result<(), CoreError> {
    counters.scratch_tables = add(counters.scratch_tables, source.tables)?;
    counters.scratch_statements = add(counters.scratch_statements, source.statements)?;
    counters.scratch_rows = add(counters.scratch_rows, source.rows)?;
    counters.scratch_high_water_bytes =
        add(counters.scratch_high_water_bytes, source.high_water_bytes)?;
    counters.scratch_owner_setup_statements = add(
        counters.scratch_owner_setup_statements,
        source.owner_setup_statements,
    )?;
    counters.scratch_derived_setup_statements = add(
        counters.scratch_derived_setup_statements,
        source.derived_setup_statements,
    )?;
    counters.scratch_operation_statements = add(
        counters.scratch_operation_statements,
        source.operation_statements,
    )?;
    counters.scratch_store_reopens = add(counters.scratch_store_reopens, source.store_reopens)?;
    counters.scratch_store_inspection_statements = add(
        counters.scratch_store_inspection_statements,
        source.store_inspection_statements,
    )?;
    counters.scratch_store_inspection_wall_ns = add(
        counters.scratch_store_inspection_wall_ns,
        source.store_inspection_wall_ns,
    )?;
    counters.scratch_setup_wall_ns = add(counters.scratch_setup_wall_ns, source.setup_wall_ns)?;
    counters.scratch_operation_wall_ns =
        add(counters.scratch_operation_wall_ns, source.operation_wall_ns)?;
    Ok(())
}

pub(crate) fn add_metadata_rope(
    counters: &mut OperationCounters,
    source: RopeCounters,
) -> Result<(), CoreError> {
    add_rope(&mut counters.metadata_rope, source)?;
    counters.add_rope(source)
}

pub(crate) fn add_native(
    counters: &mut OperationCounters,
    source: NativeOperationCounters,
) -> Result<(), CoreError> {
    counters.native.route = source.route.or(counters.native.route);
    counters.native.bytes_read = add(counters.native.bytes_read, source.bytes_read)?;
    counters.native.bytes_written = add(counters.native.bytes_written, source.bytes_written)?;
    counters.native.patch_bytes = add(counters.native.patch_bytes, source.patch_bytes)?;
    counters.native.suffix_bytes_shifted = add(
        counters.native.suffix_bytes_shifted,
        source.suffix_bytes_shifted,
    )?;
    counters.native.clone_attempts = add(counters.native.clone_attempts, source.clone_attempts)?;
    counters.native.clone_successes = add(counters.native.clone_successes, source.clone_successes)?;
    counters.native.clone_fallbacks = add(counters.native.clone_fallbacks, source.clone_fallbacks)?;
    counters.native.temp_calls = add(counters.native.temp_calls, source.temp_calls)?;
    counters.native.sync_calls = add(counters.native.sync_calls, source.sync_calls)?;
    counters.native.rename_calls = add(counters.native.rename_calls, source.rename_calls)?;
    counters.native.replace_calls = add(counters.native.replace_calls, source.replace_calls)?;
    counters.native.metadata_calls = add(counters.native.metadata_calls, source.metadata_calls)?;
    counters.native.create_calls = add(counters.native.create_calls, source.create_calls)?;
    counters.native.remove_calls = add(counters.native.remove_calls, source.remove_calls)?;
    counters.native.hard_link_calls = add(counters.native.hard_link_calls, source.hard_link_calls)?;
    Ok(())
}

fn add_rope(target: &mut RopeCounters, source: RopeCounters) -> Result<(), CoreError> {
    target.payload_bytes_read = add(target.payload_bytes_read, source.payload_bytes_read)?;
    target.payload_bytes_written = add(target.payload_bytes_written, source.payload_bytes_written)?;
    target.cdc_bytes_scanned = add(target.cdc_bytes_scanned, source.cdc_bytes_scanned)?;
    target.chunks_created = add(target.chunks_created, source.chunks_created)?;
    target.nodes_read = add(target.nodes_read, source.nodes_read)?;
    target.nodes_created = add(target.nodes_created, source.nodes_created)?;
    target.tree_level_before = merge_optional(target.tree_level_before, source.tree_level_before);
    target.logical_len_before =
        merge_optional(target.logical_len_before, source.logical_len_before);
    target.logical_len_after = merge_optional(target.logical_len_after, source.logical_len_after);
    Ok(())
}

fn merge_optional<T: Copy + Eq>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    }
}

fn add(left: u64, right: u64) -> Result<u64, CoreError> {
    left.checked_add(right).ok_or(LengthOverflow)
}
