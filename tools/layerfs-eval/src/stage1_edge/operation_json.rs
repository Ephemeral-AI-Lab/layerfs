use super::engine_counters::EngineDelta;
use super::locality::content_counters;
use super::receipt_model::{OracleReceipt, ResourceObservation};
use crate::legacy_full::{Diagnostics, PRODUCT_BUFFER_BOUND_BYTES};
use crate::stage1_fixture::EvalResult;
pub(crate) fn counters_json(
    engine: Option<EngineDelta>,
    operation: Option<&crate::legacy_full::OperationDiagnostics>,
) -> EvalResult<String> {
    if engine.is_none() && operation.is_none() {
        return Ok(format!(
            "{{{}}}",
            [
                "transactions_started",
                "transactions_committed",
                "transactions_rolled_back",
                "statements",
                "admission_transactions_started",
                "admission_transactions_committed",
                "admission_transactions_rolled_back",
                "admission_statements",
                "integrity_transactions_started",
                "integrity_transactions_committed",
                "integrity_transactions_rolled_back",
                "integrity_statements",
                "busy_events",
                "locked_events",
                "objects_validated",
                "objects_created",
                "objects_reused",
                "object_bytes_read",
                "object_bytes_written",
                "fetched_rows",
                "fetched_row_authentication_passes",
                "fetched_row_role_decode_passes",
                "new_object_authentication_passes",
                "incumbent_authentication_passes",
                "payload_batch_queries",
                "payload_batch_references",
                "payload_batch_maximum",
                "put_lookup_statements",
                "put_insert_statements",
                "created_rows",
                "reused_rows",
                "publication_transactions_started",
                "publication_transactions_rolled_back",
                "publication_commits",
                "publication_closure_passes",
                "namespace_graph_verification_passes",
                "scratch_tables",
                "scratch_statements",
                "scratch_rows",
                "scratch_high_water_bytes",
                "retained_roots_validated",
                "cdc_bytes_scanned",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "rope_nodes_read",
                "rope_nodes_emitted",
                "content_directory_nodes_emitted",
                "workspace_materializations",
                "workspace_reuses",
                "rematerializations",
                "descriptor_resets",
            ]
            .into_iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
        ));
    }
    let e = engine.unwrap_or_default();
    let c = operation
        .map(content_counters)
        .transpose()?
        .unwrap_or_default();
    let o = operation.copied().unwrap_or_default();
    let (scratch_tables, scratch_statements, scratch_rows, scratch_high_water_bytes) =
        joined_scratch_counts(e, o)?;
    Ok(format!(
        concat!(
            "{{\"transactions_started\":{},\"transactions_committed\":{},",
            "\"transactions_rolled_back\":{},\"statements\":{},",
            "\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"busy_events\":{},\"locked_events\":{},\"objects_validated\":{},",
            "\"objects_created\":{},\"objects_reused\":{},",
            "\"object_bytes_read\":{},\"object_bytes_written\":{},",
            "\"fetched_rows\":{},\"fetched_row_authentication_passes\":{},",
            "\"fetched_row_role_decode_passes\":{},",
            "\"new_object_authentication_passes\":{},",
            "\"incumbent_authentication_passes\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"put_lookup_statements\":{},",
            "\"put_insert_statements\":{},\"created_rows\":{},\"reused_rows\":{},",
            "\"publication_transactions_started\":{},",
            "\"publication_transactions_rolled_back\":{},",
            "\"publication_commits\":{},\"publication_closure_passes\":{},",
            "\"namespace_graph_verification_passes\":{},\"scratch_tables\":{},",
            "\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_high_water_bytes\":{},\"retained_roots_validated\":{},",
            "\"cdc_bytes_scanned\":{},",
            "\"payload_bytes_written\":{},\"unaffected_payload_reads\":{},",
            "\"unaffected_payload_writes\":{},\"rope_nodes_read\":{},",
            "\"rope_nodes_emitted\":{},\"content_directory_nodes_emitted\":{},",
            "\"workspace_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{},\"descriptor_resets\":{}}}"
        ),
        e.transactions_started,
        e.transactions_committed,
        e.transactions_rolled_back,
        e.statements,
        e.admission_transactions_started,
        e.admission_transactions_committed,
        e.admission_transactions_rolled_back,
        e.admission_statements,
        e.integrity_transactions_started,
        e.integrity_transactions_committed,
        e.integrity_transactions_rolled_back,
        e.integrity_statements,
        e.busy_events,
        e.locked_events,
        e.objects_validated,
        e.objects_created,
        e.objects_reused,
        e.object_bytes_read,
        e.object_bytes_written,
        e.fetched_rows,
        e.fetched_row_authentication_passes,
        e.fetched_row_role_decode_passes,
        e.new_object_authentication_passes,
        e.incumbent_authentication_passes,
        e.payload_batch_queries,
        e.payload_batch_references,
        e.payload_batch_maximum,
        e.put_lookup_statements,
        e.put_insert_statements,
        e.created_rows,
        e.reused_rows,
        e.publication_transactions_started,
        e.publication_transactions_rolled_back,
        e.publication_commits,
        e.publication_closure_passes,
        e.namespace_graph_verification_passes,
        scratch_tables,
        scratch_statements,
        scratch_rows,
        scratch_high_water_bytes,
        e.retained_roots_validated,
        c.cdc_bytes_scanned,
        c.payload_bytes_written,
        c.unaffected_payload_reads,
        c.unaffected_payload_writes,
        c.rope_nodes_read,
        c.rope_nodes_emitted,
        c.content_directory_nodes_emitted,
        o.workspace_materializations,
        o.workspace_reuses,
        o.rematerializations,
        o.descriptor_resets,
    ))
}
pub(crate) fn joined_scratch_counts(
    engine: EngineDelta,
    operation: crate::legacy_full::OperationDiagnostics,
) -> EvalResult<(u64, u64, u64, u64)> {
    Ok((
        engine
            .scratch_tables
            .checked_add(operation.scratch_tables)
            .ok_or_else(|| "combined scratch tables overflow".to_owned())?,
        engine
            .scratch_statements
            .checked_add(operation.scratch_statements)
            .ok_or_else(|| "combined scratch statements overflow".to_owned())?,
        engine
            .scratch_rows
            .checked_add(operation.scratch_rows)
            .ok_or_else(|| "combined scratch rows overflow".to_owned())?,
        engine
            .scratch_high_water_bytes
            .max(operation.scratch_high_water_bytes),
    ))
}
pub(crate) fn native_json(operation: Option<&crate::legacy_full::OperationDiagnostics>) -> String {
    if operation.is_none() {
        return format!(
            "{{{}}}",
            [
                "bytes_read",
                "bytes_written",
                "patch_bytes",
                "suffix_bytes_shifted",
                "clone_attempts",
                "clone_successes",
                "clone_fallbacks",
                "full_fallback_files",
                "files_created",
                "files_replaced",
                "files_removed",
                "sync_regular_calls",
                "sync_directory_calls",
            ]
            .into_iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
        );
    }
    let value = operation.copied().unwrap_or_default();
    format!(
        concat!(
            "{{\"bytes_read\":{},\"bytes_written\":{},\"patch_bytes\":{},",
            "\"suffix_bytes_shifted\":{},\"clone_attempts\":{},",
            "\"clone_successes\":{},\"clone_fallbacks\":{},",
            "\"full_fallback_files\":{},\"files_created\":{},",
            "\"files_replaced\":{},\"files_removed\":{},",
            "\"sync_regular_calls\":null,\"sync_directory_calls\":null}}"
        ),
        value.native.bytes_read,
        value.native.bytes_written,
        value.native.patch_bytes,
        value.native.suffix_bytes_shifted,
        value.native.clone_attempts,
        value.native.clone_successes,
        value.native.clone_fallbacks,
        value.full_fallback_files,
        value.native.create_calls,
        value.native.replace_calls,
        value.native.remove_calls,
    )
}
pub(crate) fn storage_json(before: Option<&Diagnostics>, after: Option<&Diagnostics>) -> String {
    let database_before = before.and_then(|value| value.database_bytes);
    let database_after = after.and_then(|value| value.database_bytes);
    let engine_after = after.and_then(|value| value.logical_engine_bytes);
    let database_growth = database_before
        .zip(database_after)
        .and_then(|(before, after)| after.checked_sub(before));
    let canonical = before.zip(after).and_then(|(before, after)| {
        after
            .object_bytes_written
            .checked_sub(before.object_bytes_written)
    });
    let amplification = database_growth
        .zip(canonical)
        .and_then(|(database, canonical)| {
            (canonical != 0).then_some(database as f64 / canonical as f64)
        });
    format!(
        concat!(
            "{{\"database_bytes\":{},\"logical_engine_bytes\":{},",
            "\"rollback_journal_bytes\":null,\"temporary_file_bytes\":null,",
            "\"database_growth_bytes\":{},\"canonical_object_bytes_written\":{},",
            "\"physical_to_canonical_amplification\":{}}}"
        ),
        option_u64_json(database_after),
        option_u64_json(engine_after),
        option_u64_json(database_growth),
        option_u64_json(canonical),
        amplification.map_or_else(|| "null".to_owned(), |value| format!("{value:.9}")),
    )
}
pub(crate) fn resources_json(
    resources: &ResourceObservation,
    operation: Option<&crate::legacy_full::OperationDiagnostics>,
) -> String {
    let operation_value = operation.copied().unwrap_or_default();
    let q_current = operation.map(|_| operation_value.operation_q_current_bytes);
    let q_high = operation.map(|_| operation_value.operation_q_high_water_bytes);
    let q_terminal = operation.map(|_| operation_value.operation_q_terminal_bytes);
    let owned_temp = operation
        .map(|_| operation_value.owned_temp_current)
        .or(resources.owned_temp_entries);
    format!(
        concat!(
            "{{\"rss_current_bytes\":{},\"rss_peak_bytes\":{},",
            "\"operation_q_current_bytes\":{},\"operation_q_high_water_bytes\":{},",
            "\"operation_q_terminal_bytes\":{},\"fd_current\":{},",
            "\"active_store_connections\":{},\"child_processes\":{},",
            "\"owned_temp_entries\":{},\"residue_entries\":{},",
            "\"largest_buffer_bytes\":{},\"page_size\":4096,",
            "\"cache_pages\":1280,\"cache_spill_pages\":1280,",
            "\"network_operations\":0}}"
        ),
        option_u64_json(resources.rss_current_bytes),
        resources.rss_peak_bytes,
        option_u64_json(q_current),
        option_u64_json(q_high),
        option_u64_json(q_terminal),
        resources.fd_current,
        resources.active_store_connections,
        resources.child_processes,
        option_u64_json(owned_temp),
        resources.residue_entries,
        PRODUCT_BUFFER_BOUND_BYTES,
    )
}
pub(crate) fn oracle_json(oracle: &OracleReceipt) -> String {
    format!(
        concat!(
            "{{\"logical_length\":{},\"content_digest\":\"{}\",",
            "\"physical_bytes_exact\":{},\"canonical_bytes_exact\":{},",
            "\"metadata_exact\":{},\"historical_roots_exact\":{},",
            "\"route_exact\":{}}}"
        ),
        oracle.logical_length,
        oracle.content_digest,
        option_bool_json(oracle.physical_bytes_exact),
        option_bool_json(oracle.canonical_bytes_exact),
        option_bool_json(oracle.metadata_exact),
        option_bool_json(oracle.historical_roots_exact),
        option_bool_json(oracle.route_exact),
    )
}
pub(crate) fn option_u64_json(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}
pub(crate) fn option_bool_json(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}
