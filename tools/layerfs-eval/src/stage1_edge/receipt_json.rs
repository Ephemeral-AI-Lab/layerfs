use super::artifact::{display_error, json_escape};
use super::engine_counters::PhaseCounterDelta;
use super::locality::content_counters;
use super::operation_json::counters_json;
use super::receipt_model::{HistoryProbeReceipt, SubEditReceipt};
use super::schedule_model::{replacement_bytes, EditSpec};
use crate::legacy_full::RefState;
use crate::stage1_fixture::EvalResult;
pub(crate) fn history_probe_json(probe: &HistoryProbeReceipt) -> EvalResult<String> {
    let content = content_counters(&probe.operation)?;
    let non_payload_rows = probe
        .engine
        .fetched_rows
        .checked_sub(probe.engine.payload_batch_references)
        .ok_or_else(|| "history probe payload rows exceed fetched rows".to_owned())?;
    let non_payload_statements = probe
        .engine
        .statements
        .checked_sub(probe.engine.payload_batch_queries)
        .ok_or_else(|| "history probe payload queries exceed statements".to_owned())?;
    Ok(format!(
        concat!(
            "{{\"root\":\"R{}\",\"ordinal\":{},\"start\":{},\"length\":{},",
            "\"wall_ns\":{},\"namespace_nodes_read\":{},",
            "\"inode_table_nodes_read\":{},\"rope_nodes_read\":{},",
            "\"payload_bytes_read\":{},\"payload_batch_queries\":{},",
            "\"payload_batch_references\":{},\"non_payload_statements\":{},",
            "\"non_payload_rows\":{},\"fetched_rows\":{},",
            "\"authentication_passes\":{},\"role_decode_passes\":{},",
            "\"engine_counters\":{}}}"
        ),
        probe.root_index,
        probe.ordinal,
        probe.start,
        probe.length,
        probe.wall_ns,
        probe.operation.namespace.nodes_read,
        probe.operation.inode_table.nodes_read,
        content.rope_nodes_read,
        probe.operation.rope.payload_bytes_read,
        probe.engine.payload_batch_queries,
        probe.engine.payload_batch_references,
        non_payload_statements,
        non_payload_rows,
        probe.engine.fetched_rows,
        probe.engine.fetched_row_authentication_passes,
        probe.engine.fetched_row_role_decode_passes,
        counters_json(Some(probe.engine), Some(&probe.operation))?,
    ))
}
pub(crate) fn phase_counter_json(phase: &PhaseCounterDelta) -> String {
    let value = phase.engine;
    format!(
        concat!(
            "{{\"name\":\"{}\",\"transactions_started\":{},",
            "\"transactions_committed\":{},\"transactions_rolled_back\":{},",
            "\"statements\":{},\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"busy_events\":{},\"locked_events\":{},",
            "\"objects_validated\":{},\"objects_created\":{},\"objects_reused\":{},",
            "\"object_bytes_read\":{},\"object_bytes_written\":{},",
            "\"range_bytes_requested\":{},\"range_bytes_returned\":{},",
            "\"logical_object_bytes\":{},\"logical_root_bytes\":{},",
            "\"logical_delta_bytes\":{},\"retained_union_scrubs\":{},",
            "\"root_verifications\":{},\"root_verification_objects\":{},",
            "\"root_verification_bytes\":{},\"fetched_rows\":{},",
            "\"fetched_row_authentication_passes\":{},",
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
            "\"q_before_bytes\":{},",
            "\"q_after_bytes\":{},\"q_high_water_bytes\":{},",
            "\"active_connections\":{},\"operation_scratch_tables\":{},",
            "\"operation_scratch_statements\":{},\"operation_scratch_rows\":{},",
            "\"operation_scratch_high_water_bytes\":{}}}"
        ),
        phase.name,
        value.transactions_started,
        value.transactions_committed,
        value.transactions_rolled_back,
        value.statements,
        value.admission_transactions_started,
        value.admission_transactions_committed,
        value.admission_transactions_rolled_back,
        value.admission_statements,
        value.integrity_transactions_started,
        value.integrity_transactions_committed,
        value.integrity_transactions_rolled_back,
        value.integrity_statements,
        value.busy_events,
        value.locked_events,
        value.objects_validated,
        value.objects_created,
        value.objects_reused,
        value.object_bytes_read,
        value.object_bytes_written,
        value.range_bytes_requested,
        value.range_bytes_returned,
        value.logical_object_bytes,
        value.logical_root_bytes,
        value.logical_delta_bytes,
        value.retained_union_scrubs,
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
        value.payload_batch_maximum,
        value.put_lookup_statements,
        value.put_insert_statements,
        value.created_rows,
        value.reused_rows,
        value.publication_transactions_started,
        value.publication_transactions_rolled_back,
        value.publication_commits,
        value.publication_closure_passes,
        value.namespace_graph_verification_passes,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_high_water_bytes,
        value.retained_roots_validated,
        phase.q_before_bytes,
        phase.q_after_bytes,
        phase.q_high_water_bytes,
        phase.active_connections,
        phase.operation_scratch_tables,
        phase.operation_scratch_statements,
        phase.operation_scratch_rows,
        phase.operation_scratch_high_water_bytes,
    )
}
pub(crate) fn edit_json(edit: &EditSpec) -> EvalResult<String> {
    let bytes = replacement_bytes(
        edit.serial,
        usize::try_from(edit.insert_bytes).map_err(display_error)?,
    );
    Ok(format!(
        concat!(
            "{{\"tag\":\"{}\",\"offset\":{},\"delete_bytes\":{},",
            "\"insert_bytes\":{},\"replacement_digest\":\"{}\"}}"
        ),
        edit.tag,
        edit.offset,
        edit.delete_bytes,
        edit.insert_bytes,
        blake3::hash(&bytes).to_hex(),
    ))
}
pub(crate) fn sub_edit_json(receipt: &SubEditReceipt) -> EvalResult<String> {
    let replacement = replacement_bytes(
        receipt.edit.serial,
        usize::try_from(receipt.edit.insert_bytes).map_err(display_error)?,
    );
    Ok(format!(
        concat!(
            "{{\"tag\":\"{}\",\"offset\":{},\"delete_bytes\":{},",
            "\"insert_bytes\":{},\"replacement_digest\":\"{}\",",
            "\"before_bytes\":{},\"after_bytes\":{},",
            "\"native_wall_ns\":{},\"physical_oracle_wall_ns\":{},",
            "\"native_route\":\"{}\",\"native_bytes_read\":{},",
            "\"native_bytes_written\":{},\"native_patch_bytes\":{},",
            "\"native_suffix_bytes_shifted\":{},\"native_clone_attempts\":{},",
            "\"native_clone_successes\":{},\"native_clone_fallbacks\":{},",
            "\"native_full_fallback_files\":{},\"tree_level_before\":{},",
            "\"cdc_bytes_scanned\":{},\"payload_bytes_written\":{},",
            "\"unaffected_payload_reads\":{},\"unaffected_payload_writes\":{},",
            "\"rope_nodes_read\":{},\"rope_nodes_emitted\":{},",
            "\"content_directory_nodes_emitted\":{}}}"
        ),
        receipt.edit.tag,
        receipt.edit.offset,
        receipt.edit.delete_bytes,
        receipt.edit.insert_bytes,
        blake3::hash(&replacement).to_hex(),
        receipt.edit.before_bytes,
        receipt.edit.after_bytes,
        receipt.native_wall_ns,
        receipt.physical_oracle_wall_ns,
        receipt.native_route,
        receipt.native_bytes_read,
        receipt.native_bytes_written,
        receipt.native_patch_bytes,
        receipt.native_suffix_bytes_shifted,
        receipt.native_clone_attempts,
        receipt.native_clone_successes,
        receipt.native_clone_fallbacks,
        receipt.native_full_fallback_files,
        receipt
            .tree_level_before
            .map_or_else(|| "null".to_owned(), |value| value.to_string()),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.cdc_bytes_scanned.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.payload_bytes_written.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.unaffected_payload_reads.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.unaffected_payload_writes.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.rope_nodes_read.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.rope_nodes_emitted.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.content_directory_nodes_emitted.to_string()
        ),
    ))
}
pub(crate) fn ref_json(reference: Option<&RefState>) -> String {
    reference.map_or_else(
        || "null".to_owned(),
        |value| {
            format!(
                "{{\"name\":\"{}\",\"generation\":{},\"root\":\"{}\"}}",
                json_escape(&value.name),
                value.generation,
                value.root
            )
        },
    )
}
