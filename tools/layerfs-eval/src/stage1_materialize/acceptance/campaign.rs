use super::super::attribution::campaign::validate_row_wall_json;
use super::super::contract::EvalResult;
use super::super::error::io_error;
use super::super::parity::evidence::{append_sync, json_u128, validate_instrumented_row};
use super::super::prepare::{durable_write, json_escape};
use super::contract::AcceptanceBlock;
use std::fs;
use std::path::Path;

pub(in crate::stage1_materialize) fn acceptance_campaign_failure(
    run: &Path,
    block: usize,
    reason: &str,
) -> EvalResult<()> {
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"FAIL\",\"block\":{block},\"reason\":\"{}\"}}",
            json_escape(reason)
        ),
    )?;
    Err(format!(
        "acceptance campaign stopped at block {block}: {reason}"
    ))
}

pub(in crate::stage1_materialize) fn copy_acceptance_manifests(
    run: &Path,
    fixture: &Path,
) -> EvalResult<()> {
    let source = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?
        .join("layerfs-stage1m-custody/source-manifests");
    for name in [
        "source-manifest-historical.json",
        "source-manifest-historical-harness.json",
        "source-manifest-control.json",
        "source-manifest-candidate.json",
    ] {
        durable_write(
            &run.join(name),
            &fs::read(source.join(name)).map_err(io_error)?,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::stage1_materialize) fn enrich_acceptance_row(
    row: &str,
    sequence: u64,
    block_index: usize,
    block: &AcceptanceBlock,
    order: usize,
    operand: char,
    source_role: &str,
    executable_sha256: &str,
    schedule_blake3: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{},\"sequence\":{},\"block\":{},\"pair\":{},\"pair_size_order\":{},\"operand\":\"{}\",\"source_role\":\"{}\",\"executable_sha256\":\"{}\",\"schedule_blake3\":\"{}\",\"command_wall_ns\":{}}}",
        body,
        sequence,
        block_index,
        block.pair,
        order,
        operand,
        source_role,
        executable_sha256,
        schedule_blake3,
        command_wall_ns,
    ))
}

pub(in crate::stage1_materialize) fn validate_acceptance_row(
    row: &str,
    candidate: bool,
) -> EvalResult<()> {
    validate_instrumented_row(row)?;
    validate_row_wall_json(row)?;
    for exact in [
        "\"schema\":\"layerfs-stage1m-attribution-row-v2\"",
        "\"status\":\"PASS\"",
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
        "\"operation_q_terminal_bytes\":0",
        "\"residue\":0",
    ] {
        if !row.contains(exact) {
            return Err(format!("acceptance row does not prove {exact}"));
        }
    }
    let resources_pass = json_u128(row, "rss_peak_bytes")? <= 32 * 1024 * 1024
        && json_u128(row, "rss_current_bytes")? <= 32 * 1024 * 1024
        && json_u128(row, "fd_before")? <= 24
        && json_u128(row, "fd_after")? <= 24
        && (!candidate
            || json_u128(row, "operation_q_high_water_bytes")? < 8 * 1024 * 1024
                && json_u128(row, "owned_temp_terminal")? == 0
                && json_u128(row, "descriptor_spool_bytes_terminal")? == 0
                && json_u128(row, "scratch_connections_peak")? <= 1
                && json_u128(row, "total_connections_peak")? <= 2);
    if !resources_pass {
        return Err("acceptance row resource gate failed".to_owned());
    }
    if row.contains("\"row_kind\":\"measured\"")
        && (json_u128(row, "fd_terminal")? != json_u128(row, "process_fd_baseline")?
            || json_u128(row, "connections_terminal")? != 0
            || candidate
                && (json_u128(row, "scratch_connections_terminal")? != 0
                    || json_u128(row, "total_connections_terminal")? != 0))
    {
        return Err("acceptance row terminal resource closure failed".to_owned());
    }
    Ok(())
}

pub(in crate::stage1_materialize) fn acceptance_semantic_signature(
    row: &str,
) -> EvalResult<Vec<u128>> {
    [
        "logical_bytes",
        "fetched_rows",
        "authentication_passes",
        "role_decode_passes",
        "object_bytes_read",
        "payload_batch_references",
        "payload_batch_maximum",
        "publication_commits",
        "bytes_written",
        "temp_calls",
        "replace_calls",
        "operation_q_terminal_bytes",
        "residue",
    ]
    .into_iter()
    .map(|key| json_u128(row, key))
    .collect()
}
