use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::prepare::{durable_write, json_escape};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[allow(clippy::too_many_arguments)]
pub(in crate::stage1_materialize) fn append_command(
    run: &Path,
    sequence: u64,
    pair: usize,
    order: usize,
    operand: &str,
    executable_sha256: &str,
    wall_ns: u128,
    output: &std::process::Output,
) -> EvalResult<()> {
    append_sync(
        &run.join("commands.json"),
        &format!(
            concat!(
                "{{\"sequence\":{},\"pair\":{},\"order\":{},\"operand\":\"{}\",",
                "\"executable_sha256\":\"{}\",\"wall_ns\":{},\"status\":{},",
                "\"stderr\":\"{}\"}}"
            ),
            sequence,
            pair,
            order,
            operand,
            executable_sha256,
            wall_ns,
            output.status.code().unwrap_or(-1),
            json_escape(&String::from_utf8_lossy(&output.stderr)),
        ),
    )
}

pub(in crate::stage1_materialize) fn finish_parity(
    run: &Path,
    campaign_wall_ns: u128,
    historical_walls: Vec<u128>,
    instrumented_walls: Vec<u128>,
) -> EvalResult<()> {
    let historical = four_stats(&historical_walls)?;
    let instrumented = four_stats(&instrumented_walls)?;
    let p50_allowance = 1_000_000_u128.max(historical.0 * 3 / 100);
    let p50_pass = instrumented.0 <= historical.0 + p50_allowance;
    let p95_pass = instrumented.1 <= historical.1 + 1_000_000;
    let wall_pass = campaign_wall_ns < 10_000_000_000;
    let status = if p50_pass && p95_pass && wall_pass {
        "PASS"
    } else {
        "REVISE"
    };
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-summary-v1\",\"status\":\"{}\",",
            "\"warmup_rows\":8,\"measured_rows\":8,\"legacy_work_exact\":true,",
            "\"historical\":{{\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}},",
            "\"instrumented\":{{\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}},",
            "\"p50_allowance_ns\":{},\"p50_pass\":{},\"p95_pass\":{},",
            "\"campaign_wall_ns\":{},\"hard_wall_pass\":{}}}\n"
        ),
        status,
        historical_walls,
        historical.0,
        historical.1,
        instrumented_walls,
        instrumented.0,
        instrumented.1,
        p50_allowance,
        p50_pass,
        p95_pass,
        campaign_wall_ns,
        wall_pass,
    );
    durable_write(&run.join("summary.json"), summary.as_bytes())?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M parity\n\nStatus: **{status}**\n\nHistorical p50/p95: `{}/{}` ns. Instrumented p50/p95: `{}/{}` ns. Complete wall: `{campaign_wall_ns}` ns. All 16 rows and exact legacy work retained.\n",
            historical.0, historical.1, instrumented.0, instrumented.1,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-parity-campaign-time-v1\nstatus={status}\ncampaign_wall_ns={campaign_wall_ns}\nwarmups=8\nmeasured=8\nhard_wall_ns=10000000000\n"
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"{}\",\"preserved_failures\":0}}",
            status
        ),
    )?;
    println!(
        "stage1m-parity-run status={} run={} wall_ns={} p50_pass={} p95_pass={}",
        status,
        run.display(),
        campaign_wall_ns,
        p50_pass,
        p95_pass
    );
    if status == "PASS" {
        Ok(())
    } else {
        Err("instrumented parity requires repair".to_owned())
    }
}

pub(in crate::stage1_materialize) fn find_fixture_manifest(source: &Path) -> EvalResult<PathBuf> {
    source
        .ancestors()
        .map(|ancestor| ancestor.join("fixture-manifest.json"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "fixture-manifest.json not found above source".to_owned())
}

pub(in crate::stage1_materialize) fn copy_global_manifests(run: &Path) -> EvalResult<()> {
    let source = crate::stage1_fixture::workspace_root()
        .join("target/layerfs-stage1m-custody/source-manifests");
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
        b"{\"status\":\"NotApplicable\",\"reason\":\"candidate does not exist during M1 parity\"}\n",
    )
}

pub(in crate::stage1_materialize) fn create_empty(path: &Path) -> EvalResult<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

pub(in crate::stage1_materialize) fn append_sync(path: &Path, line: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(line.as_bytes()).map_err(io_error)?;
    file.write_all(b"\n").map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

pub(in crate::stage1_materialize) fn enrich_row(
    row: &str,
    pair: usize,
    order: usize,
    operand: &str,
    executable_sha256: &str,
    command_wall_ns: u128,
) -> EvalResult<String> {
    let body = row
        .strip_suffix('}')
        .ok_or_else(|| "child row is not a JSON object".to_owned())?;
    Ok(format!(
        "{body},\"pair\":{pair},\"order\":{order},\"operand\":\"{operand}\",\"executable_sha256\":\"{executable_sha256}\",\"command_wall_ns\":{command_wall_ns}}}"
    ))
}

#[derive(Eq, PartialEq)]
pub(in crate::stage1_materialize) struct ComparableRow(Vec<u128>);

pub(in crate::stage1_materialize) fn comparable_row(row: &str) -> EvalResult<ComparableRow> {
    [
        "logical_bytes",
        "statements",
        "fetched_rows",
        "authentication_passes",
        "role_decode_passes",
        "object_bytes_read",
        "payload_batch_queries",
        "payload_batch_references",
        "payload_batch_maximum",
        "publication_commits",
        "tables",
        "rows",
        "high_water_bytes",
        "bytes_written",
        "temp_calls",
        "sync_calls",
        "replace_calls",
        "metadata_calls",
        "operation_q_terminal_bytes",
        "residue",
    ]
    .into_iter()
    .map(|key| json_u128(row, key))
    .collect::<EvalResult<Vec<_>>>()
    .map(ComparableRow)
}

pub(in crate::stage1_materialize) fn validate_instrumented_row(row: &str) -> EvalResult<()> {
    for truth in [
        "\"engine_sql_exact\":true",
        "\"scratch_sql_exact\":true",
        "\"fetched_auth_decode_exact\":true",
    ] {
        if !row.contains(truth) {
            return Err(format!("instrumented row does not prove {truth}"));
        }
    }
    let wall = json_u128(row, "product_operation_wall_ns")?;
    let residual = json_i128(row, "operation_residual_ns")?.unsigned_abs();
    if residual > 500_000_u128.max(wall / 100) {
        return Err(format!(
            "instrumented operation residual {residual} exceeds tolerance"
        ));
    }
    Ok(())
}

pub(in crate::stage1_materialize) fn four_stats(values: &[u128]) -> EvalResult<(u128, u128)> {
    if values.len() != 4 {
        return Err("n=4 statistic requires four values".to_owned());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Ok(((sorted[1] + sorted[2]) / 2, sorted[3]))
}

pub(in crate::stage1_materialize) fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    json_number_text(json, key)?.parse().map_err(display_error)
}

pub(in crate::stage1_materialize) fn json_i128(json: &str, key: &str) -> EvalResult<i128> {
    json_number_text(json, key)?.parse().map_err(display_error)
}

pub(in crate::stage1_materialize) fn json_string_value(
    json: &str,
    key: &str,
) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let rest = json
        .find(&needle)
        .and_then(|offset| json.get(offset + needle.len()..))
        .ok_or_else(|| format!("missing JSON string {key}"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?;
    rest.get(..end)
        .map(str::to_owned)
        .ok_or_else(|| format!("invalid JSON string {key}"))
}

fn json_number_text<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":");
    let rest = json
        .find(&needle)
        .and_then(|offset| json.get(offset + needle.len()..))
        .ok_or_else(|| format!("missing JSON number {key}"))?;
    let end = rest
        .find(|character: char| !character.is_ascii_digit() && character != '-')
        .unwrap_or(rest.len());
    rest.get(..end)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("invalid JSON number {key}"))
}
