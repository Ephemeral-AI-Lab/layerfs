use super::super::contract::EvalResult;
use super::super::error::display_error;
use super::super::row::contract::EngineDelta;
use super::super::row::output::json_optional_u64;
use super::super::row::projection::projection_json;
use super::contract::AttributionArm;
use super::observation::AttributionObservation;
use super::projection::{
    attribution_timer_equation, engine_sql, scratch_observation_json, scratch_sql,
};
use crate::legacy_full::IntegrityMode;

pub(in crate::stage1_materialize) fn trust_equation(
    mode: IntegrityMode,
    engine: &EngineDelta,
) -> bool {
    engine.fetched_rows == engine.role_decode_passes
        && match mode {
            IntegrityMode::Verified => engine.fetched_rows == engine.authentication_passes,
            IntegrityMode::TrustedLocalDev => {
                engine.authentication_passes == 0 && engine.identity_authentication_ns == 0
            }
        }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::stage1_materialize) fn attribution_row_json(
    row_kind: &str,
    block_arm: AttributionArm,
    observed_arm: AttributionArm,
    measured_ordinal: usize,
    identity: &str,
    size_mib: u64,
    root: crate::legacy_full::RootId,
    source_digest: &str,
    observation: &AttributionObservation,
    mode: IntegrityMode,
) -> EvalResult<String> {
    let row = &observation.row;
    let operation = &row.operation;
    let expected_bytes = size_mib * 1024 * 1024;
    let content_bytes = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "content payload accounting underflow".to_owned())?;
    let metadata_bytes = operation.metadata_rope.payload_bytes_read;
    let engine_sql = engine_sql(&row.engine)?;
    let scratch_sql = scratch_sql(operation)?;
    let trust_exact = trust_equation(mode, &row.engine);
    let byte_equations_pass = match observed_arm {
        AttributionArm::Complete | AttributionArm::Null | AttributionArm::Digest => {
            content_bytes == expected_bytes
                && operation.rope.payload_bytes_read
                    == content_bytes
                        .checked_add(metadata_bytes)
                        .ok_or_else(|| "payload byte equation overflow".to_owned())?
        }
        AttributionArm::Native => {
            content_bytes == 0 && operation.native.bytes_written == expected_bytes
        }
    };
    let resource_gates_pass = operation.operation_q_high_water_bytes < 8 * 1024 * 1024
        && operation.operation_q_terminal_bytes == 0
        && row.scratch_connections_peak <= 1
        && row.total_connections_peak <= 2
        && row.total_connections_current
            == row
                .active_connections
                .checked_add(row.scratch_connections_current)
                .ok_or_else(|| "current connection equation overflow".to_owned())?
        && row.fd_before <= 24
        && row.fd_after <= 24
        && row.rss_peak_bytes <= 32 * 1024 * 1024
        && row.rss_current_bytes <= 32 * 1024 * 1024;
    let (leaf_ns, vfs_dispatch_ns, operation_residual_ns) =
        attribution_timer_equation(observed_arm, observation)?;
    let digest_fact = observation.digest_sink_hash_bytes.map_or_else(
        || "{\"applicability\":\"NotApplicable\"}".to_owned(),
        |bytes| format!("{{\"applicability\":\"Applicable\",\"bytes\":{bytes}}}"),
    );
    let source_applicability = if observed_arm == AttributionArm::Native {
        "NotApplicable"
    } else {
        "Applicable"
    };
    let native_applicability = if matches!(
        observed_arm,
        AttributionArm::Complete | AttributionArm::Native
    ) {
        "Applicable"
    } else {
        "NotApplicable"
    };
    let projection_through_row =
        if matches!(observed_arm, AttributionArm::Null | AttributionArm::Digest) {
            "{\"applicability\":\"NotApplicable\"}".to_owned()
        } else {
            format!(
                "{{\"applicability\":\"Applicable\",\"facts\":{}}}",
                projection_json(row.projection_total)
            )
        };
    let materialize_inclusive = if observed_arm == AttributionArm::Complete {
        format!(
            "{{\"applicability\":\"Applicable\",\"nanoseconds\":{}}}",
            operation.materialize_inclusive_ns
        )
    } else {
        "{\"applicability\":\"NotApplicable\"}".to_owned()
    };
    let payload_batch_maximum = if observed_arm == AttributionArm::Native {
        "{\"applicability\":\"NotApplicable\",\"value\":0}".to_owned()
    } else {
        format!(
            "{{\"applicability\":\"Applicable\",\"value\":{}}}",
            row.engine.payload_batch_maximum
        )
    };
    let operation_label = match (mode, row_kind) {
        (IntegrityMode::TrustedLocalDev, "warmup") => {
            "trusted_localdev_first_open_fresh_destination"
        }
        (IntegrityMode::TrustedLocalDev, _) => {
            "trusted_localdev_same_open_warmed_source_fresh_destination"
        }
        (IntegrityMode::Verified, "warmup") => "first_open_fresh_destination",
        (IntegrityMode::Verified, _) => observed_arm.operation_label(),
    };
    let source_conditioning = match (mode, row_kind) {
        (IntegrityMode::TrustedLocalDev, "warmup") => "explicit_trusted_open",
        (IntegrityMode::TrustedLocalDev, _) => "same_trusted_open_after_primer",
        (IntegrityMode::Verified, "warmup") => "fresh_open_after_scrub",
        (IntegrityMode::Verified, _) => "same_open_after_primer",
    };
    let authenticated_bytes = if mode == IntegrityMode::Verified {
        row.engine.object_bytes_read
    } else {
        0
    };
    let mode_label = match mode {
        IntegrityMode::Verified => "Verified",
        IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
    };
    let named_row_wall_ns = row
        .product_wall_ns
        .checked_add(row.oracle_wall_ns)
        .and_then(|value| value.checked_add(row.cleanup_wall_ns))
        .ok_or_else(|| "row wall overflow".to_owned())?;
    let row_wall_residual_ns = i128::try_from(row.row_wall_ns).map_err(display_error)?
        - i128::try_from(named_row_wall_ns).map_err(display_error)?;
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-attribution-row-v2\",\"status\":\"PASS\",",
            "\"integrity_mode\":\"{}\",\"row_kind\":\"{}\",\"block_identity\":\"{}\",",
            "\"requested_arm\":\"{}\",\"executed_arm\":\"{}\",",
            "\"measured_ordinal\":{},\"operation_label\":\"{}\",",
            "\"source_conditioning\":\"{}\",\"controlled_device_cold\":false,",
            "\"incremental_refresh\":false,\"size_mib\":{},\"logical_bytes\":{},",
            "\"root\":\"{}\",\"source_digest\":\"{}\",\"output_digest\":\"{}\",",
            "\"oracle\":{{\"status\":\"PASS\",\"kind\":\"{}\"}},",
            "\"product_operation_wall_ns\":{},\"row_wall_ns\":{},\"oracle_wall_ns\":{},",
            "\"cleanup_wall_ns\":{},\"source\":{{\"applicability\":\"{}\",",
            "\"content_payload_bytes\":{},\"metadata_payload_bytes\":{},",
            "\"canonical_bytes_authenticated\":{},\"identity_hash_bytes\":{},",
            "\"sink_write_calls\":{},",
            "\"sink_write_ns\":{},\"digest_sink_hash\":{}}},",
            "\"native_applicability\":\"{}\",\"engine\":{},\"scratch\":{},",
            "\"projection\":{},\"projection_through_row\":{},",
            "\"native\":{{\"bytes_written\":{},\"temp_calls\":{},",
            "\"sync_calls\":{},\"replace_calls\":{},\"metadata_calls\":{}}},",
            "\"resources\":{{\"user_cpu_ns\":{},\"system_cpu_ns\":{},",
            "\"rss_peak_bytes\":{},\"rss_current_bytes\":{},",
            "\"process_fd_baseline\":{},\"fd_before\":{},\"fd_after\":{},",
            "\"active_connections\":{},\"primary_connections_current\":{},",
            "\"scratch_connections_current\":{},\"scratch_connections_peak\":{},",
            "\"total_connections_current\":{},\"total_connections_peak\":{},",
            "\"fd_terminal\":{},\"connections_terminal\":{},",
            "\"primary_connections_terminal\":{},\"scratch_connections_terminal\":{},",
            "\"total_connections_terminal\":{},\"operation_q_high_water_bytes\":{},",
            "\"operation_q_terminal_bytes\":{}}},",
            "\"equations\":{{\"engine_sql_sum\":{},\"engine_sql_exact\":{},",
            "\"scratch_sql_sum\":{},\"scratch_sql_exact\":{},",
            "\"fetched_auth_decode_exact\":{},\"trust_work_exact\":{},",
            "\"byte_equations_pass\":{},\"resource_gates_pass\":{},",
            "\"canonical_store_writer_transactions\":0,\"publication_commits\":{},",
            "\"canonical_cdc_bytes\":{},\"store_id_queries\":{},",
            "\"payload_batch_maximum\":{},\"materialize_inclusive\":{},",
            "\"payload_callback_timer_class\":\"inclusive_report_only\",",
            "\"exclusive_leaf_ns\":{},",
            "\"vfs_dispatch_ns\":{},\"operation_residual_ns\":{},",
            "\"row_wall_residual_ns\":{}}},",
            "\"operation_q_terminal_bytes\":{},\"residue\":0}}"
        ),
        mode_label,
        row_kind,
        identity,
        block_arm.name(),
        observed_arm.name(),
        measured_ordinal,
        operation_label,
        source_conditioning,
        size_mib,
        expected_bytes,
        root,
        source_digest,
        row.output_digest,
        match observed_arm {
            AttributionArm::Complete => "exact_public_complete",
            AttributionArm::Null => "exact_source_byte_equation",
            AttributionArm::Digest => "exact_source_digest",
            AttributionArm::Native => "exact_native_bytes_metadata",
        },
        row.product_wall_ns,
        row.row_wall_ns,
        row.oracle_wall_ns,
        row.cleanup_wall_ns,
        source_applicability,
        content_bytes,
        metadata_bytes,
        authenticated_bytes,
        authenticated_bytes,
        observation.sink_write_calls,
        observation.sink_write_ns,
        digest_fact,
        native_applicability,
        engine_delta_json(&row.engine),
        scratch_observation_json(operation),
        projection_json(operation.projection),
        projection_through_row,
        operation.native.bytes_written,
        operation.native.temp_calls,
        operation.native.sync_calls,
        operation.native.replace_calls,
        operation.native.metadata_calls,
        row.user_cpu_ns,
        row.system_cpu_ns,
        row.rss_peak_bytes,
        row.rss_current_bytes,
        row.process_fd_baseline,
        row.fd_before,
        row.fd_after,
        row.active_connections,
        row.active_connections,
        row.scratch_connections_current,
        row.scratch_connections_peak,
        row.total_connections_current,
        row.total_connections_peak,
        json_optional_u64(row.fd_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.connections_terminal),
        json_optional_u64(row.scratch_connections_terminal),
        json_optional_u64(row.total_connections_terminal),
        operation.operation_q_high_water_bytes,
        operation.operation_q_terminal_bytes,
        engine_sql,
        engine_sql == row.engine.statements,
        scratch_sql,
        scratch_sql == operation.scratch_statements,
        trust_exact,
        trust_exact && row.engine.busy_events == 0 && row.engine.locked_events == 0,
        byte_equations_pass,
        resource_gates_pass,
        row.engine.publication_commits,
        operation.rope.cdc_bytes_scanned,
        row.engine.store_id_queries,
        payload_batch_maximum,
        materialize_inclusive,
        leaf_ns,
        vfs_dispatch_ns,
        operation_residual_ns,
        row_wall_residual_ns,
        operation.operation_q_terminal_bytes,
    ))
}

pub(in crate::stage1_materialize) fn engine_delta_json(engine: &EngineDelta) -> String {
    format!(
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
        engine.statements,
        engine.integrity_statements,
        engine.busy_events,
        engine.locked_events,
        engine.fetched_rows,
        engine.authentication_passes,
        engine.role_decode_passes,
        engine.object_bytes_read,
        engine.payload_batch_queries,
        engine.payload_batch_references,
        engine.payload_batch_maximum,
        engine.publication_commits,
        engine.publication_statements,
        engine.live_verified_integrity_statements,
        engine.primary_read_statements,
        engine.reconciliation_statements,
        engine.compaction_statements,
        engine.connection_mutex_wait_ns,
        engine.trust_guard_ns,
        engine.nonpayload_query_ns,
        engine.payload_query_ns,
        engine.identity_authentication_ns,
        engine.role_decode_ns,
        engine.payload_callback_inclusive_ns,
        engine.counter_merge_ns,
        engine.store_id_queries,
    )
}
