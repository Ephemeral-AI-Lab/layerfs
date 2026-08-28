use super::super::contract::EvalResult;
use super::super::error::display_error;
use super::contract::Row;
use super::projection::projection_json;
use crate::legacy_full::{ProjectionTimer, ProjectionTimerAvailability};

pub(in crate::stage1_materialize) fn print_row(
    kind: &str,
    identity: &str,
    size_mib: u64,
    root: crate::legacy_full::RootId,
    source_digest: &str,
    row: &Row,
) -> EvalResult<()> {
    let native = row.operation.native;
    let (operation_label, source_conditioning) = if kind == "warmup" {
        ("first_open_fresh_destination", "fresh_open_after_scrub")
    } else {
        (
            "same_open_warmed_source_fresh_destination",
            "same_open_after_primer",
        )
    };
    let engine = format!(
        concat!(
            "{{\"statements\":{},\"integrity_statements\":{},\"busy_events\":{},",
            "\"locked_events\":{},\"fetched_rows\":{},\"authentication_passes\":{},",
            "\"role_decode_passes\":{},\"object_bytes_read\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"publication_commits\":{},",
            "\"publication_statements\":{},\"live_verified_integrity_statements\":{},",
            "\"primary_read_statements\":{},\"reconciliation_statements\":{},",
            "\"compaction_statements\":{},\"connection_mutex_wait_ns\":{},",
            "\"trust_guard_ns\":{},\"nonpayload_query_ns\":{},",
            "\"payload_query_ns\":{},\"identity_authentication_ns\":{},",
            "\"role_decode_ns\":{},\"payload_callback_inclusive_ns\":{},",
            "\"counter_merge_ns\":{},\"store_id_queries\":{}}}"
        ),
        row.engine.statements,
        row.engine.integrity_statements,
        row.engine.busy_events,
        row.engine.locked_events,
        row.engine.fetched_rows,
        row.engine.authentication_passes,
        row.engine.role_decode_passes,
        row.engine.object_bytes_read,
        row.engine.payload_batch_queries,
        row.engine.payload_batch_references,
        row.engine.payload_batch_maximum,
        row.engine.publication_commits,
        row.engine.publication_statements,
        row.engine.live_verified_integrity_statements,
        row.engine.primary_read_statements,
        row.engine.reconciliation_statements,
        row.engine.compaction_statements,
        row.engine.connection_mutex_wait_ns,
        row.engine.trust_guard_ns,
        row.engine.nonpayload_query_ns,
        row.engine.payload_query_ns,
        row.engine.identity_authentication_ns,
        row.engine.role_decode_ns,
        row.engine.payload_callback_inclusive_ns,
        row.engine.counter_merge_ns,
        row.engine.store_id_queries,
    );
    let scratch = format!(
        concat!(
            "{{\"tables\":{},\"statements\":{},\"rows\":{},\"high_water_bytes\":{},",
            "\"owner_setup_statements\":{},\"derived_setup_statements\":{},",
            "\"operation_statements\":{},\"store_reopens\":{},",
            "\"store_inspection_statements\":{},\"store_inspection_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"operation_wall_ns\":{}}}"
        ),
        row.operation.scratch_tables,
        row.operation.scratch_statements,
        row.operation.scratch_rows,
        row.operation.scratch_high_water_bytes,
        row.operation.scratch_owner_setup_statements,
        row.operation.scratch_derived_setup_statements,
        row.operation.scratch_operation_statements,
        row.operation.scratch_store_reopens,
        row.operation.scratch_store_inspection_statements,
        row.operation.scratch_store_inspection_wall_ns,
        row.operation.scratch_setup_wall_ns,
        row.operation.scratch_operation_wall_ns,
    );
    let projection = projection_json(row.operation.projection);
    let projection_total = projection_json(row.projection_total);
    let engine_sql = row
        .engine
        .publication_statements
        .checked_add(row.engine.live_verified_integrity_statements)
        .and_then(|value| value.checked_add(row.engine.primary_read_statements))
        .and_then(|value| value.checked_add(row.engine.reconciliation_statements))
        .and_then(|value| value.checked_add(row.engine.compaction_statements))
        .ok_or_else(|| "Engine SQL equation overflow".to_owned())?;
    let scratch_sql = row
        .operation
        .scratch_owner_setup_statements
        .checked_add(row.operation.scratch_derived_setup_statements)
        .and_then(|value| value.checked_add(row.operation.scratch_operation_statements))
        .ok_or_else(|| "scratch SQL equation overflow".to_owned())?;
    let (leaf_ns, vfs_dispatch_ns) = exclusive_leaf_ns(row)?;
    let operation_residual_ns =
        i128::try_from(row.product_wall_ns).map_err(display_error)? - i128::from(leaf_ns);
    println!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-row-v1\",\"status\":\"PASS\",",
            "\"row_kind\":\"{}\",\"identity\":\"{}\",\"operation_label\":\"{}\",",
            "\"source_conditioning\":\"{}\",\"controlled_device_cold\":false,",
            "\"incremental_refresh\":false,\"size_mib\":{},\"logical_bytes\":{},",
            "\"root\":\"{}\",\"source_digest\":\"{}\",\"output_digest\":\"{}\",",
            "\"product_operation_wall_ns\":{},\"oracle_wall_ns\":{},",
            "\"cleanup_wall_ns\":{},\"engine\":{},\"scratch\":{},",
            "\"projection\":{},\"projection_through_cleanup\":{},",
            "\"native\":{{\"bytes_written\":{},\"temp_calls\":{},\"sync_calls\":{},",
            "\"replace_calls\":{},\"metadata_calls\":{}}},",
            "\"resources\":{{\"user_cpu_ns\":{},\"system_cpu_ns\":{},",
            "\"rss_peak_bytes\":{},\"rss_current_bytes\":{},\"process_fd_baseline\":{},",
            "\"fd_before\":{},",
            "\"fd_after\":{},\"active_connections\":{},",
            "\"scratch_connections_current\":{},\"scratch_connections_peak\":{},",
            "\"total_connections_current\":{},\"total_connections_peak\":{},",
            "\"fd_terminal\":{},\"connections_terminal\":{},",
            "\"scratch_connections_terminal\":{},\"total_connections_terminal\":{},",
            "\"operation_q_high_water_bytes\":{},\"owned_temp_terminal\":{},",
            "\"descriptor_spool_bytes_terminal\":{}}},",
            "\"equations\":{{\"engine_sql_sum\":{},\"engine_sql_exact\":{},",
            "\"scratch_sql_sum\":{},\"scratch_sql_exact\":{},",
            "\"fetched_auth_decode_exact\":{},\"materialize_inclusive_ns\":{},",
            "\"vfs_dispatch_ns\":{},\"exclusive_leaf_ns\":{},",
            "\"operation_residual_ns\":{}}},",
            "\"operation_q_terminal_bytes\":{},\"residue\":0}}"
        ),
        kind,
        identity,
        operation_label,
        source_conditioning,
        size_mib,
        size_mib * 1024 * 1024,
        root,
        source_digest,
        row.output_digest,
        row.product_wall_ns,
        row.oracle_wall_ns,
        row.cleanup_wall_ns,
        engine,
        scratch,
        projection,
        projection_total,
        native.bytes_written,
        native.temp_calls,
        native.sync_calls,
        native.replace_calls,
        native.metadata_calls,
        row.user_cpu_ns,
        row.system_cpu_ns,
        row.rss_peak_bytes,
        row.rss_current_bytes,
        row.process_fd_baseline,
        row.fd_before,
        row.fd_after,
        row.active_connections,
        row.scratch_connections_current,
        row.scratch_connections_peak,
        row.total_connections_current,
        row.total_connections_peak,
        json_optional_u64(row.fd_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.scratch_connections_terminal),
        json_optional_u64(row.total_connections_terminal),
        row.operation.operation_q_high_water_bytes,
        row.operation.owned_temp_terminal,
        row.operation.descriptor_spool_bytes_terminal,
        engine_sql,
        engine_sql == row.engine.statements,
        scratch_sql,
        scratch_sql == row.operation.scratch_statements,
        row.engine.fetched_rows == row.engine.authentication_passes
            && row.engine.fetched_rows == row.engine.role_decode_passes,
        row.operation.materialize_inclusive_ns,
        vfs_dispatch_ns,
        leaf_ns,
        operation_residual_ns,
        row.operation.operation_q_terminal_bytes,
    );
    Ok(())
}

pub(in crate::stage1_materialize) fn json_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

pub(in crate::stage1_materialize) fn exclusive_leaf_ns(row: &Row) -> EvalResult<(u64, u64)> {
    let projection = row.operation.projection;
    let content_write_ns = timer_ns(projection.content_write.wall)?;
    let mut total = 0_u64;
    for value in [
        row.engine.connection_mutex_wait_ns,
        row.engine.trust_guard_ns,
        row.engine.nonpayload_query_ns,
        row.engine.payload_query_ns,
        row.engine.identity_authentication_ns,
        row.engine.role_decode_ns,
        row.engine.counter_merge_ns,
        row.operation.scratch_store_inspection_wall_ns,
        row.operation.scratch_setup_wall_ns,
        row.operation.scratch_operation_wall_ns,
        timer_ns(projection.workspace_root_create_open.wall)?,
        timer_ns(projection.staging_create_open.wall)?,
        timer_ns(projection.recovery_marker_create.wall)?,
        timer_ns(projection.name_preflight.wall)?,
        timer_ns(projection.temp_create.wall)?,
        timer_ns(projection.workspace_marker_write.wall)?,
        content_write_ns,
        timer_ns(projection.metadata_value_write.wall)?,
        timer_ns(projection.content_flush.wall)?,
        timer_ns(projection.metadata_validate.wall)?,
        timer_ns(projection.metadata_apply.wall)?,
        timer_ns(projection.metadata_preinstall_verify.wall)?,
        timer_ns(projection.metadata_postinstall_verify.wall)?,
        timer_ns(projection.root_binding_revalidate.wall)?,
        timer_ns(projection.recovery_marker_file_sync.wall)?,
        timer_ns(projection.content_temp_file_sync.wall)?,
        timer_ns(projection.post_hardlink_file_sync.wall)?,
        timer_ns(projection.staging_directory_sync.wall)?,
        timer_ns(projection.root_parent_directory_sync.wall)?,
        timer_ns(projection.install_parent_directory_sync.wall)?,
        timer_ns(projection.dirty_tree_directory_sync.wall)?,
        timer_ns(projection.final_root_directory_sync.wall)?,
        timer_ns(projection.replace.wall)?,
        timer_ns(projection.cleanup.wall)?,
    ] {
        total = total
            .checked_add(value)
            .ok_or_else(|| "exclusive timer equation overflow".to_owned())?;
    }
    let vfs_dispatch_ns = row
        .operation
        .materialize_inclusive_ns
        .checked_sub(total)
        .ok_or_else(|| "named children exceed VFS materialization parent".to_owned())?;
    Ok((
        total
            .checked_add(vfs_dispatch_ns)
            .ok_or_else(|| "VFS timer equation overflow".to_owned())?,
        vfs_dispatch_ns,
    ))
}

pub(in crate::stage1_materialize) fn timer_ns(timer: ProjectionTimer) -> EvalResult<u64> {
    match timer.availability {
        ProjectionTimerAvailability::Available => Ok(timer.nanoseconds),
        ProjectionTimerAvailability::Unavailable => {
            Err("required Apple timer unavailable".to_owned())
        }
    }
}
