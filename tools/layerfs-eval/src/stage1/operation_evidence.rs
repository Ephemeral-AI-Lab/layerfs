use super::model::{EditCase, EngineDelta};
use super::root_validation::edit_result_len;
use crate::legacy_full::{NativeRoute, OperationDiagnostics};
use crate::stage1_fixture::{CloneReceipt, EvalResult};
pub(crate) fn clone_json(value: &CloneReceipt) -> String {
    format!(
        "{{\"evidence\":\"APFSCloneReturnPlusSealedMasterCustodyNotPerResetFullRehash\",\"reset_wall_ns\":{},\"clone_return_wall_ns\":{},\"source_logical_bytes\":{},\"destination_logical_bytes\":{},\"source_allocated_bytes\":{},\"destination_allocated_bytes\":{},\"distinct_regular_inodes\":{},\"clone_id\":{}}}",
        value.wall_ns,
        value.clone_wall_ns,
        value.source_logical_bytes,
        value.destination_logical_bytes,
        value.source_allocated_bytes,
        value.destination_allocated_bytes,
        value.distinct_regular_inodes,
        value.clone_id,
    )
}
pub(crate) fn edit_case_json(value: &EditCase) -> EvalResult<String> {
    Ok(format!(
        "{{\"base_bytes\":{},\"offset\":{},\"delete_bytes\":{},\"replacement_bytes\":{},\"result_bytes\":{}}}",
        value.base_len,
        value.start,
        value.delete_len,
        value.replacement.len(),
        edit_result_len(value)?,
    ))
}
pub(crate) fn native_json(value: &OperationDiagnostics) -> String {
    let native = &value.native;
    format!(
        concat!(
            "{{\"route\":{},\"bytes_read\":{},\"bytes_written\":{},\"patch_bytes\":{},",
            "\"suffix_bytes_shifted\":{},\"clone_attempts\":{},\"clone_successes\":{},",
            "\"clone_fallbacks\":{},\"temp_calls\":{},\"sync_calls\":{},",
            "\"rename_calls\":{},\"replace_calls\":{},\"metadata_calls\":{},",
            "\"create_calls\":{},\"remove_calls\":{},\"hard_link_calls\":{}}}"
        ),
        native_route_json(native.route),
        native.bytes_read,
        native.bytes_written,
        native.patch_bytes,
        native.suffix_bytes_shifted,
        native.clone_attempts,
        native.clone_successes,
        native.clone_fallbacks,
        native.temp_calls,
        native.sync_calls,
        native.rename_calls,
        native.replace_calls,
        native.metadata_calls,
        native.create_calls,
        native.remove_calls,
        native.hard_link_calls,
    )
}
pub(crate) fn counters_json(value: &OperationDiagnostics) -> String {
    format!(
        concat!(
            "{{\"rope\":{{\"payload_bytes_read\":{},\"payload_bytes_written\":{},",
            "\"cdc_bytes_scanned\":{},\"chunks_emitted\":{},\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"metadata_rope\":{{\"payload_bytes_read\":{},\"payload_bytes_written\":{},",
            "\"cdc_bytes_scanned\":{},\"chunks_emitted\":{},\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"namespace\":{{\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"inode_table\":{{\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"native\":{},\"workspace_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{},\"descriptor_resets\":{},\"root_diff_nodes\":{},",
            "\"changed_paths\":{},\"full_fallback_files\":{},\"plan_rows\":{},",
            "\"plan_scratch_high_water_bytes\":{},\"current_digest_bytes\":{},",
            "\"uncached_prior_digest_bytes\":{},\"changed_current_cdc_bytes\":{},",
            "\"unchanged_file_roots_reused\":{},\"authority_full_scans\":{},",
            "\"scratch_tables\":{},\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_high_water_bytes\":{},\"operation_q_current_bytes\":{},",
            "\"operation_q_high_water_bytes\":{},\"operation_q_terminal_bytes\":{},",
            "\"owned_temp_current\":{},\"owned_temp_terminal\":{},",
            "\"descriptor_spool_bytes_current\":{},\"descriptor_spool_bytes_terminal\":{}}}"
        ),
        value.rope.payload_bytes_read,
        value.rope.payload_bytes_written,
        value.rope.cdc_bytes_scanned,
        value.rope.chunks_created,
        value.rope.nodes_read,
        value.rope.nodes_created,
        value.metadata_rope.payload_bytes_read,
        value.metadata_rope.payload_bytes_written,
        value.metadata_rope.cdc_bytes_scanned,
        value.metadata_rope.chunks_created,
        value.metadata_rope.nodes_read,
        value.metadata_rope.nodes_created,
        value.namespace.nodes_read,
        value.namespace.nodes_created,
        value.inode_table.nodes_read,
        value.inode_table.nodes_created,
        native_json(value),
        value.workspace_materializations,
        value.workspace_reuses,
        value.rematerializations,
        value.descriptor_resets,
        value.root_diff_nodes,
        value.changed_paths,
        value.full_fallback_files,
        value.plan_rows,
        value.plan_scratch_high_water_bytes,
        value.current_digest_bytes,
        value.uncached_prior_digest_bytes,
        value.changed_current_cdc_bytes,
        value.unchanged_file_roots_reused,
        value.authority_full_scans,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_high_water_bytes,
        value.operation_q_current_bytes,
        value.operation_q_high_water_bytes,
        value.operation_q_terminal_bytes,
        value.owned_temp_current,
        value.owned_temp_terminal,
        value.descriptor_spool_bytes_current,
        value.descriptor_spool_bytes_terminal,
    )
}
pub(crate) fn engine_json(value: &EngineDelta) -> String {
    let integrity_mode = match value.integrity_mode {
        crate::legacy_full::IntegrityMode::Verified => "Verified",
        crate::legacy_full::IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
    };
    let operation_class = if value.publication_commits == 0 && value.transactions_started == 0 {
        "read_only"
    } else {
        "state_change"
    };
    let authentication_scope = match (
        value.integrity_mode,
        value.fetched_row_authentication_passes,
        value.fetched_rows,
    ) {
        (crate::legacy_full::IntegrityMode::Verified, _, _) => "verified_all_fetched_rows",
        (crate::legacy_full::IntegrityMode::TrustedLocalDev, 0, _) => "trusted_structural_only",
        (crate::legacy_full::IntegrityMode::TrustedLocalDev, authenticated, fetched)
            if authenticated == fetched =>
        {
            "authoritative_publication_transaction_authenticated_rows"
        }
        (crate::legacy_full::IntegrityMode::TrustedLocalDev, _, _) => {
            "authoritative_publication_transaction_authenticated_rows"
        }
    };
    format!(
        concat!(
            "{{\"integrity_mode\":\"{}\",\"operation_class\":\"{}\",",
            "\"authentication_scope\":\"{}\",\"transactions_started\":{},",
            "\"transactions_committed\":{},",
            "\"transactions_rolled_back\":{},\"statements\":{},\"objects_validated\":{},",
            "\"objects_created\":{},\"objects_reused\":{},\"object_bytes_read\":{},",
            "\"object_bytes_written\":{},\"range_bytes_requested\":{},",
            "\"range_bytes_returned\":{},\"root_verifications\":{},",
            "\"root_verification_objects\":{},\"root_verification_bytes\":{},",
            "\"fetched_rows\":{},\"fetched_row_authentication_passes\":{},",
            "\"fetched_row_role_decode_passes\":{},\"new_object_authentication_passes\":{},",
            "\"incumbent_authentication_passes\":{},\"payload_batch_queries\":{},",
            "\"payload_batch_references\":{},\"payload_batch_session_maximum\":{},",
            "\"put_lookup_statements\":{},\"put_insert_statements\":{},",
            "\"created_rows\":{},\"reused_rows\":{},\"publication_commits\":{},",
            "\"publication_closure_passes\":{},\"namespace_graph_verification_passes\":{},",
            "\"scratch_tables\":{},\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_session_high_water_bytes\":{}}}"
        ),
        integrity_mode,
        operation_class,
        authentication_scope,
        value.transactions_started,
        value.transactions_committed,
        value.transactions_rolled_back,
        value.statements,
        value.objects_validated,
        value.objects_created,
        value.objects_reused,
        value.object_bytes_read,
        value.object_bytes_written,
        value.range_bytes_requested,
        value.range_bytes_returned,
        value.root_verifications,
        value.root_verification_objects,
        value.root_verification_bytes,
        value.fetched_rows,
        value.fetched_row_authentication_passes,
        value.fetched_row_role_decode_passes,
        value.new_object_authentication_passes,
        value.incumbent_authentication_passes,
        value.payload_batch_queries,
        value.payload_batch_references,
        value.payload_batch_session_maximum,
        value.put_lookup_statements,
        value.put_insert_statements,
        value.created_rows,
        value.reused_rows,
        value.publication_commits,
        value.publication_closure_passes,
        value.namespace_graph_verification_passes,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_session_high_water_bytes,
    )
}
pub(crate) fn native_route_json(route: Option<NativeRoute>) -> String {
    route.map_or_else(|| "null".to_owned(), |route| format!("\"{route:?}\""))
}
pub(crate) fn option_u64_json(value: Option<u64>) -> String {
    value.map_or_else(|| "\"Unavailable\"".to_owned(), |value| value.to_string())
}
pub(crate) fn observed_u64_json(observed: bool, value: u64) -> String {
    if observed {
        value.to_string()
    } else {
        "\"Unavailable\"".to_owned()
    }
}
pub(crate) fn option_growth_json(before: Option<u64>, after: Option<u64>) -> String {
    match (before, after) {
        (Some(before), Some(after)) if after >= before => (after - before).to_string(),
        _ => "\"Unavailable\"".to_owned(),
    }
}
pub(crate) fn json_u64_array(values: &[u64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}
