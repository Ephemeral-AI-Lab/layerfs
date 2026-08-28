use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::parity::evidence::{append_sync, json_i128, json_u128};
use super::super::prepare::{durable_write, json_escape};
use super::contract::AttributionArm;
use std::fs;
use std::path::Path;

pub(in crate::stage1_materialize) fn attribution_campaign_failure(
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
        "attribution campaign stopped at block {block}: {reason}"
    ))
}

pub(in crate::stage1_materialize) fn copy_attribution_manifests(
    run: &Path,
    fixture: &Path,
) -> EvalResult<()> {
    let target = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?;
    let source = target.join("layerfs-stage1m-custody/source-manifests");
    for name in [
        "source-manifest-historical.json",
        "source-manifest-historical-harness.json",
        "source-manifest-control.json",
    ] {
        durable_write(
            &run.join(name),
            &fs::read(source.join(name)).map_err(io_error)?,
        )?;
    }
    durable_write(
        &run.join("source-manifest-candidate.json"),
        b"{\"status\":\"NotApplicable\",\"reason\":\"candidate does not exist during M2 control attribution\"}\n",
    )
}

pub(in crate::stage1_materialize) fn enrich_attribution_row(
    row: &str,
    sequence: u64,
    block: usize,
    executable_sha256: &str,
    schedule_blake3: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{body},\"sequence\":{sequence},\"block\":{block},\"executable_sha256\":\"{executable_sha256}\",\"schedule_blake3\":\"{schedule_blake3}\",\"command_wall_ns\":{command_wall_ns}}}"
    ))
}

pub(in crate::stage1_materialize) fn validate_attribution_json(row: &str) -> EvalResult<()> {
    for exact in [
        "\"status\":\"PASS\"",
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
        "\"trust_work_exact\":true",
        "\"resource_gates_pass\":true",
        "\"byte_equations_pass\":true",
    ] {
        if !row.contains(exact) {
            return Err(format!("attribution row does not prove {exact}"));
        }
    }
    validate_row_wall_json(row)
}

pub(in crate::stage1_materialize) fn validate_row_wall_json(row: &str) -> EvalResult<()> {
    let product = json_u128(row, "product_operation_wall_ns")?;
    let oracle = json_u128(row, "oracle_wall_ns")?;
    let cleanup = json_u128(row, "cleanup_wall_ns")?;
    let expected_row_wall = product
        .checked_add(oracle)
        .and_then(|value| value.checked_add(cleanup))
        .ok_or_else(|| "attribution row wall overflow".to_owned())?;
    let residual = json_i128(row, "row_wall_residual_ns")?;
    let observed_row_wall =
        i128::try_from(json_u128(row, "row_wall_ns")?).map_err(display_error)?;
    let expected_row_wall = i128::try_from(expected_row_wall).map_err(display_error)?;
    if residual < 0 || observed_row_wall != expected_row_wall + residual {
        return Err("row wall = product + oracle + cleanup + residual".to_owned());
    }
    Ok(())
}

pub(in crate::stage1_materialize) fn three_stats(values: &[u128]) -> EvalResult<(u128, u128)> {
    if values.len() != 3 {
        return Err("n=3 statistic requires three values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok((sorted[1], sorted[2]))
}

pub(in crate::stage1_materialize) fn attribution_models_json(
    populations: &[(AttributionArm, u64, u128, u128)],
) -> EvalResult<String> {
    let mut models = Vec::new();
    for arm in [
        AttributionArm::Complete,
        AttributionArm::Null,
        AttributionArm::Digest,
        AttributionArm::Native,
    ] {
        let time = |size| {
            populations
                .iter()
                .find(|(candidate, candidate_size, _, _)| {
                    *candidate == arm && *candidate_size == size
                })
                .map(|(_, _, p50, _)| *p50)
                .ok_or_else(|| format!("missing {} {size} MiB population", arm.name()))
        };
        let t0 = time(0)? as f64;
        let t24 = time(24)? as f64;
        let t96 = time(96)? as f64;
        let slope = (t96 - t24) / 72.0;
        if slope <= 0.0 {
            return Err(format!("{} fitted slope is not positive", arm.name()));
        }
        let modeled24 = t0 + 24.0 * slope;
        let modeled96 = t0 + 96.0 * slope;
        let residual24 = t24 - modeled24;
        let residual96 = t96 - modeled96;
        let valid = residual24.abs() <= 2_000_000_f64.max(t24 * 0.05)
            && residual96.abs() <= 2_000_000_f64.max(t96 * 0.05);
        models.push(format!(
            concat!(
                "{{\"arm\":\"{}\",\"fixed_cost_ns\":{},",
                "\"slope_ns_per_mib\":{},\"sustained_bandwidth_mib_per_s\":{},",
                "\"residual_24_ns\":{},\"residual_96_ns\":{},",
                "\"predicted_t100_ns\":{},\"model_valid\":{}}}"
            ),
            arm.name(),
            t0,
            slope,
            1_000_000_000_f64 / slope,
            residual24,
            residual96,
            t0 + 100.0 * slope,
            valid,
        ));
    }
    Ok(format!("[{}]", models.join(",")))
}
