use super::receipt::OperationReceipt;
use super::workflow::run_workflow;
use crate::legacy_full::{Diagnostics, OperationDiagnostics};
use std::fs;
use std::path::{Path, PathBuf};

fn counter_delta(operation: &OperationReceipt, field: fn(Diagnostics) -> u64) -> String {
    operation
        .diagnostics
        .and_then(|(before, after)| field(after).checked_sub(field(before)))
        .map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
}

fn signed_delta(operation: &OperationReceipt, field: fn(Diagnostics) -> u64) -> String {
    operation.diagnostics.map_or_else(
        || "Unavailable".to_owned(),
        |(before, after)| (i128::from(field(after)) - i128::from(field(before))).to_string(),
    )
}

fn optional_signed_delta(
    operation: &OperationReceipt,
    field: fn(Diagnostics) -> Option<u64>,
) -> String {
    operation
        .diagnostics
        .and_then(|(before, after)| Some((field(before)?, field(after)?)))
        .map_or_else(
            || "Unavailable".to_owned(),
            |(before, after)| (i128::from(after) - i128::from(before)).to_string(),
        )
}

fn operation_counter(
    operation: &OperationReceipt,
    field: fn(OperationDiagnostics) -> u64,
) -> String {
    operation.operation_diagnostics.map_or_else(
        || "Unavailable".to_owned(),
        |value| field(value).to_string(),
    )
}

fn native_route(operation: &OperationReceipt) -> String {
    operation
        .operation_diagnostics
        .and_then(|value| value.native.route)
        .map_or_else(|| "Unavailable".to_owned(), |route| format!("{route:?}"))
}

pub fn run(directory: &Path) -> Result<(), String> {
    let receipt = run_workflow(directory).map_err(|error| error.to_string())?;
    for operation in &receipt.operations {
        println!(
            "apple-poc-operation status=PASS operation={} api_route={} native_route={} wall_ns={} root_before={} root_after={} transactions_delta={} commits_delta={} rollbacks_delta={} statements_delta={} busy_delta={} locked_delta={} objects_validated_delta={} objects_created_delta={} objects_reused_delta={} object_bytes_read_delta={} object_bytes_written_delta={} range_bytes_requested_delta={} range_bytes_returned_delta={} logical_object_bytes_delta={} logical_root_bytes_delta={} logical_delta_bytes_delta={} retained_union_scrubs_delta={} root_verifications_delta={} root_verification_objects_delta={} root_verification_bytes_delta={} database_bytes_delta={} rollback_journal_bytes_delta={} temporary_file_bytes_delta={} logical_engine_bytes_delta={} active_connections_delta={} q_current_delta={} q_high_water_delta={} payload_bytes_read={} payload_bytes_written={} cdc_bytes_scanned={} chunks_created={} rope_nodes_read={} rope_nodes_created={} namespace_nodes_read={} namespace_nodes_created={} inode_nodes_read={} inode_nodes_created={} clone_attempts={} clone_successes={} clone_fallbacks={} native_bytes_read={} native_bytes_written={} native_patch_bytes={} native_suffix_bytes_shifted={}",
            operation.operation,
            operation.api_route,
            native_route(operation),
            operation.wall_ns,
            operation.root_before,
            operation.root_after,
            counter_delta(operation, |value| value.transactions_started),
            counter_delta(operation, |value| value.transactions_committed),
            counter_delta(operation, |value| value.transactions_rolled_back),
            counter_delta(operation, |value| value.statements),
            counter_delta(operation, |value| value.busy_events),
            counter_delta(operation, |value| value.locked_events),
            counter_delta(operation, |value| value.objects_validated),
            counter_delta(operation, |value| value.objects_created),
            counter_delta(operation, |value| value.objects_reused),
            counter_delta(operation, |value| value.object_bytes_read),
            counter_delta(operation, |value| value.object_bytes_written),
            counter_delta(operation, |value| value.range_bytes_requested),
            counter_delta(operation, |value| value.range_bytes_returned),
            counter_delta(operation, |value| value.logical_object_bytes),
            counter_delta(operation, |value| value.logical_root_bytes),
            counter_delta(operation, |value| value.logical_delta_bytes),
            counter_delta(operation, |value| value.retained_union_scrubs),
            counter_delta(operation, |value| value.root_verifications),
            counter_delta(operation, |value| value.root_verification_objects),
            counter_delta(operation, |value| value.root_verification_bytes),
            optional_signed_delta(operation, |value| value.database_bytes),
            optional_signed_delta(operation, |value| value.rollback_journal_bytes),
            optional_signed_delta(operation, |value| value.temporary_file_bytes),
            optional_signed_delta(operation, |value| value.logical_engine_bytes),
            signed_delta(operation, |value| value.active_connections),
            signed_delta(operation, |value| value.operation_q_current_bytes),
            signed_delta(operation, |value| value.operation_q_high_water_bytes),
            operation_counter(operation, |value| value.rope.payload_bytes_read),
            operation_counter(operation, |value| value.rope.payload_bytes_written),
            operation_counter(operation, |value| value.rope.cdc_bytes_scanned),
            operation_counter(operation, |value| value.rope.chunks_created),
            operation_counter(operation, |value| value.rope.nodes_read),
            operation_counter(operation, |value| value.rope.nodes_created),
            operation_counter(operation, |value| value.namespace.nodes_read),
            operation_counter(operation, |value| value.namespace.nodes_created),
            operation_counter(operation, |value| value.inode_table.nodes_read),
            operation_counter(operation, |value| value.inode_table.nodes_created),
            operation_counter(operation, |value| value.native.clone_attempts),
            operation_counter(operation, |value| value.native.clone_successes),
            operation_counter(operation, |value| value.native.clone_fallbacks),
            operation_counter(operation, |value| value.native.bytes_read),
            operation_counter(operation, |value| value.native.bytes_written),
            operation_counter(operation, |value| value.native.patch_bytes),
            operation_counter(operation, |value| value.native.suffix_bytes_shifted),
        );
    }
    for stage in &receipt.stages {
        println!(
            "apple-poc-stage stage={} root={} transactions={} commits={} objects_created={} objects_reused={} connections={} q_current={} q_high_water={}",
            stage.label,
            stage.root,
            stage.diagnostics.transactions_started,
            stage.diagnostics.transactions_committed,
            stage.diagnostics.objects_created,
            stage.diagnostics.objects_reused,
            stage.diagnostics.active_connections,
            stage.diagnostics.operation_q_current_bytes,
            stage.diagnostics.operation_q_high_water_bytes,
        );
    }
    println!(
        "apple-poc stages=S0-S12 root_a={} root_final={} wall_ms={} store_bytes={} page_size={} cache_pages={} database_bytes={:?} logical_bytes={:?} objects_created={} objects_reused={} compact_old={} compact_new={} compact_mark={} compact_aux_peak={} compact_verify_scratch={} compact_selector={} compact_total_peak={} q_structural_bound={} q_current={} q_high_water={} fd_baseline={} fd_terminal={} residue={}",
        receipt.root_a,
        receipt.root_final,
        receipt.wall.as_millis(),
        receipt.store_bytes,
        receipt.page_size,
        receipt.cache_pages,
        receipt.database_bytes,
        receipt.logical_engine_bytes,
        receipt.diagnostics.objects_created,
        receipt.diagnostics.objects_reused,
        receipt.compaction.old_generation_bytes,
        receipt.compaction.new_generation_bytes,
        receipt.compaction.mark_database_bytes,
        receipt.compaction.candidate_journal_temp_peak_bytes,
        receipt.compaction.verification_scratch_peak_bytes,
        receipt.compaction.selector_temporary_bytes,
        receipt.compaction.total_peak_bytes,
        receipt.operation_q_bound_bytes,
        receipt.diagnostics.operation_q_current_bytes,
        receipt.diagnostics.operation_q_high_water_bytes,
        receipt.fd_baseline,
        receipt.fd_terminal,
        receipt.residue.len()
    );
    if !receipt.residue.is_empty() {
        return Err(format!("owned residue: {:?}", receipt.residue));
    }
    fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn fd_count() -> Result<usize, std::io::Error> {
    Ok(fs::read_dir("/dev/fd")?
        .collect::<Result<Vec<_>, _>>()?
        .len())
}

pub(super) fn owned_residue(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    fn walk(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".layerfs-")
                || name == "CURRENT.tmp"
                || name.ends_with("-journal")
                || name.ends_with("-wal")
                || name.ends_with("-shm")
            {
                output.push(path.clone());
            }
            if entry.file_type()?.is_dir() {
                walk(&path, output)?;
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    walk(root, &mut output)?;
    Ok(output)
}

pub(super) fn tree_bytes(root: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        total = total.saturating_add(if metadata.is_dir() {
            tree_bytes(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(total)
}
