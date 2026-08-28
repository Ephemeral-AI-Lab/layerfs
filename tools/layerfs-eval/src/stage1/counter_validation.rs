use super::model::{EditCase, EngineDelta, MIB};
use super::operation_evidence::native_json;
use crate::legacy_full::{Diagnostics, NativeRoute, OperationDiagnostics};
use crate::stage1_fixture::EvalResult;
pub(crate) fn engine_delta(before: &Diagnostics, after: &Diagnostics) -> EvalResult<EngineDelta> {
    if before.integrity_mode != after.integrity_mode {
        return Err("engine integrity mode changed during observation".to_owned());
    }
    macro_rules! delta {
        ($field:ident) => {
            after
                .$field
                .checked_sub(before.$field)
                .ok_or_else(|| format!("engine counter {} moved backwards", stringify!($field)))?
        };
    }
    Ok(EngineDelta {
        integrity_mode: after.integrity_mode,
        transactions_started: delta!(transactions_started),
        transactions_committed: delta!(transactions_committed),
        transactions_rolled_back: delta!(transactions_rolled_back),
        statements: delta!(statements),
        objects_validated: delta!(objects_validated),
        objects_created: delta!(objects_created),
        objects_reused: delta!(objects_reused),
        object_bytes_read: delta!(object_bytes_read),
        object_bytes_written: delta!(object_bytes_written),
        range_bytes_requested: delta!(range_bytes_requested),
        range_bytes_returned: delta!(range_bytes_returned),
        root_verifications: delta!(root_verifications),
        root_verification_objects: delta!(root_verification_objects),
        root_verification_bytes: delta!(root_verification_bytes),
        fetched_rows: delta!(fetched_rows),
        fetched_row_authentication_passes: delta!(fetched_row_authentication_passes),
        fetched_row_role_decode_passes: delta!(fetched_row_role_decode_passes),
        new_object_authentication_passes: delta!(new_object_authentication_passes),
        incumbent_authentication_passes: delta!(incumbent_authentication_passes),
        payload_batch_queries: delta!(payload_batch_queries),
        payload_batch_references: delta!(payload_batch_references),
        payload_batch_session_maximum: after.payload_batch_maximum,
        put_lookup_statements: delta!(put_lookup_statements),
        put_insert_statements: delta!(put_insert_statements),
        created_rows: delta!(created_rows),
        reused_rows: delta!(reused_rows),
        publication_commits: delta!(publication_commits),
        publication_closure_passes: delta!(publication_closure_passes),
        namespace_graph_verification_passes: delta!(namespace_graph_verification_passes),
        scratch_tables: delta!(scratch_tables),
        scratch_statements: delta!(scratch_statements),
        scratch_rows: delta!(scratch_rows),
        scratch_session_high_water_bytes: after.scratch_high_water_bytes,
    })
}
pub(crate) fn verify_engine_equations(delta: &EngineDelta) -> EvalResult<()> {
    match delta.integrity_mode {
        crate::legacy_full::IntegrityMode::Verified
            if delta.fetched_rows != delta.fetched_row_authentication_passes
                || delta.fetched_rows != delta.fetched_row_role_decode_passes =>
        {
            return Err(format!(
                "verified fetched/auth/decode equation failed: {}/{}/{}",
                delta.fetched_rows,
                delta.fetched_row_authentication_passes,
                delta.fetched_row_role_decode_passes
            ));
        }
        crate::legacy_full::IntegrityMode::TrustedLocalDev
            if delta.fetched_rows != delta.fetched_row_role_decode_passes
                || delta.fetched_row_authentication_passes > delta.fetched_rows =>
        {
            return Err(format!(
                "trusted fetched/auth/decode equation failed: {}/{}/{}",
                delta.fetched_rows,
                delta.fetched_row_authentication_passes,
                delta.fetched_row_role_decode_passes
            ));
        }
        _ => {}
    }
    if delta.payload_batch_session_maximum > 64 {
        return Err(format!(
            "payload batch maximum {} exceeds 64",
            delta.payload_batch_session_maximum
        ));
    }
    if delta.scratch_session_high_water_bytes > 8 * MIB {
        return Err(format!(
            "engine scratch high-water {} exceeds 8 MiB",
            delta.scratch_session_high_water_bytes
        ));
    }
    Ok(())
}
pub(crate) fn verify_operation_resources(counters: &OperationDiagnostics) -> EvalResult<()> {
    if counters.operation_q_high_water_bytes > 8 * MIB
        || counters.operation_q_terminal_bytes != 0
        || counters.scratch_high_water_bytes > 8 * MIB
        || counters.plan_scratch_high_water_bytes > 8 * MIB
    {
        return Err(format!(
            "operation resource gate failed: Q high/terminal={}/{}, scratch/plan={}/{}",
            counters.operation_q_high_water_bytes,
            counters.operation_q_terminal_bytes,
            counters.scratch_high_water_bytes,
            counters.plan_scratch_high_water_bytes
        ));
    }
    Ok(())
}
pub(crate) fn verify_direct_read(counters: &OperationDiagnostics) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    if counters.native != Default::default()
        || counters.rope.payload_bytes_written != 0
        || counters.rope.cdc_bytes_scanned != 0
    {
        return Err("direct canonical read performed native/write/CDC work".to_owned());
    }
    Ok(())
}
pub(crate) fn verify_read_only_engine(delta: &EngineDelta) -> EvalResult<()> {
    verify_engine_equations(delta)?;
    if delta.integrity_mode == crate::legacy_full::IntegrityMode::TrustedLocalDev
        && delta.fetched_row_authentication_passes != 0
    {
        return Err(format!(
            "trusted read-only operation performed {} identity authentications",
            delta.fetched_row_authentication_passes
        ));
    }
    if delta.transactions_started != 0
        || delta.transactions_committed != 0
        || delta.publication_commits != 0
    {
        return Err("read-only operation performed a writer transaction/COMMIT".to_owned());
    }
    Ok(())
}
pub(crate) fn verify_state_change(delta: &EngineDelta, expected: u64) -> EvalResult<()> {
    verify_engine_equations(delta)?;
    if delta.transactions_started != expected
        || delta.transactions_committed != expected
        || delta.transactions_rolled_back != 0
        || delta.publication_commits != expected
    {
        return Err(format!(
            "state-change transaction equation failed: started={}, committed={}, rolled_back={}, publication_commits={}, expected={expected}",
            delta.transactions_started,
            delta.transactions_committed,
            delta.transactions_rolled_back,
            delta.publication_commits
        ));
    }
    match delta.integrity_mode {
        crate::legacy_full::IntegrityMode::TrustedLocalDev
            if delta.fetched_row_authentication_passes == 0
                || delta.publication_closure_passes != 0 =>
        {
            return Err(format!(
                "trusted state-change publication-transaction authentication failed: auth={}, publication_commits={}, trusted_closure_passes={}",
                delta.fetched_row_authentication_passes,
                delta.publication_commits,
                delta.publication_closure_passes,
            ));
        }
        crate::legacy_full::IntegrityMode::Verified
            if delta.publication_closure_passes != expected =>
        {
            return Err(format!(
                "verified state-change closure equation failed: closure_passes={}, expected={expected}",
                delta.publication_closure_passes,
            ));
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn verify_logical_locality(
    counters: &OperationDiagnostics,
    replacement_bytes: u64,
) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    let (cdc_bytes_scanned, payload_bytes_read, payload_bytes_written) = content_rope(counters)?;
    if cdc_bytes_scanned > replacement_bytes
        || payload_bytes_read != 0
        || payload_bytes_written > replacement_bytes
        || counters.namespace.nodes_created != 0
        || counters.native != Default::default()
    {
        return Err(format!(
            "logical locality equation failed: cdc={}, payload_read={}, payload_write={}, directory_nodes={}, native={}",
            cdc_bytes_scanned,
            payload_bytes_read,
            payload_bytes_written,
            counters.namespace.nodes_created,
            native_json(counters)
        ));
    }
    Ok(())
}
pub(crate) fn verify_native_edit_shape(
    counters: &OperationDiagnostics,
    case: &EditCase,
) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    let (cdc_bytes_scanned, payload_bytes_read, payload_bytes_written) = content_rope(counters)?;
    if cdc_bytes_scanned > case.replacement.len() as u64
        || payload_bytes_read != 0
        || payload_bytes_written > case.replacement.len() as u64
        || counters.namespace.nodes_created != 0
    {
        return Err(format!("{} canonical locality equation failed", case.id));
    }
    let count_change = case.delete_len != case.replacement.len() as u64;
    if !count_change {
        if !matches!(
            counters.native.route,
            Some(NativeRoute::ClonePatch | NativeRoute::InPlacePatch)
        ) || counters.native.suffix_bytes_shifted != 0
            || counters.native.patch_bytes != case.replacement.len() as u64
        {
            return Err(format!("{} native same-size route mismatch", case.id));
        }
    } else {
        match counters.native.route {
            Some(NativeRoute::InPlaceShift) => {
                let suffix = case
                    .base_len
                    .checked_sub(case.start + case.delete_len)
                    .ok_or_else(|| "native suffix equation underflow".to_owned())?;
                let transfer = suffix
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(case.replacement.len() as u64))
                    .ok_or_else(|| "native suffix equation overflow".to_owned())?;
                if counters.native.suffix_bytes_shifted != suffix
                    || counters.native.bytes_read + counters.native.bytes_written != transfer
                {
                    return Err(format!(
                        "{} in-place shift equation failed: S={suffix}, transfer={transfer}",
                        case.id
                    ));
                }
            }
            Some(NativeRoute::FullFallback) => {
                if counters.full_fallback_files != 1 {
                    return Err(format!("{} full fallback was not counted", case.id));
                }
            }
            route => {
                return Err(format!(
                    "{} count-changing native route is {:?}",
                    case.id, route
                ));
            }
        }
    }
    Ok(())
}
pub(crate) fn verify_exact_noop(
    counters: &OperationDiagnostics,
    engine: &EngineDelta,
) -> EvalResult<()> {
    verify_read_only_engine(engine)?;
    verify_operation_resources(counters)?;
    if counters.native.route != Some(NativeRoute::ExactNoop)
        || counters.rope.payload_bytes_read != 0
        || counters.rope.payload_bytes_written != 0
        || counters.rope.cdc_bytes_scanned != 0
        || counters.native.bytes_read != 0
        || counters.native.bytes_written != 0
        || engine.object_bytes_read != 0
        || engine.object_bytes_written != 0
    {
        return Err("managed exact no-op performed payload/native/CDC/write work".to_owned());
    }
    Ok(())
}
pub(crate) fn content_rope(counters: &OperationDiagnostics) -> EvalResult<(u64, u64, u64)> {
    Ok((
        counters
            .rope
            .cdc_bytes_scanned
            .checked_sub(counters.metadata_rope.cdc_bytes_scanned)
            .ok_or_else(|| "metadata CDC counter exceeds aggregate rope counter".to_owned())?,
        counters
            .rope
            .payload_bytes_read
            .checked_sub(counters.metadata_rope.payload_bytes_read)
            .ok_or_else(|| "metadata read counter exceeds aggregate rope counter".to_owned())?,
        counters
            .rope
            .payload_bytes_written
            .checked_sub(counters.metadata_rope.payload_bytes_written)
            .ok_or_else(|| "metadata write counter exceeds aggregate rope counter".to_owned())?,
    ))
}
pub(crate) fn locality_evidence_json() -> &'static str {
    "{\"unaffected_suffix_payload_reads\":0,\"unaffected_suffix_payload_writes\":0,\"derivation\":\"total-content-payload-read-zero-and-content-payload-write-bounded-by-replacement\"}"
}
