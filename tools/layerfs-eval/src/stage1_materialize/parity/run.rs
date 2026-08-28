use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{digest_file, sha256_file};
use super::super::prepare::{durable_write, json_escape};
use super::evidence::{
    append_command, append_sync, comparable_row, copy_global_manifests, create_empty, enrich_row,
    find_fixture_manifest, finish_parity, json_u128, validate_instrumented_row,
};
use super::readiness::parity_schedule_json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub fn parity_run(
    historical: &Path,
    instrumented: &Path,
    store: &Path,
    source: &Path,
    readiness: &Path,
    run: &Path,
) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let historical = historical.canonicalize().map_err(io_error)?;
    let instrumented = instrumented.canonicalize().map_err(io_error)?;
    let store = store.canonicalize().map_err(io_error)?;
    let source = source.canonicalize().map_err(io_error)?;
    let readiness_bytes = fs::read(readiness).map_err(io_error)?;
    let readiness_text = std::str::from_utf8(&readiness_bytes).map_err(display_error)?;
    let historical_sha256 = sha256_file(&historical)?;
    let instrumented_sha256 = sha256_file(&instrumented)?;
    let source_digest = digest_file(&source)?;
    for binding in [
        historical_sha256.as_str(),
        instrumented_sha256.as_str(),
        source_digest.as_str(),
        "\"status\":\"PASS\"",
        "\"measured_rows_started\":false",
    ] {
        if !readiness_text.contains(binding) {
            return Err(format!("readiness does not bind {binding}"));
        }
    }
    let run_parent = run
        .parent()
        .ok_or_else(|| "run directory has no parent".to_owned())?;
    fs::create_dir_all(run_parent).map_err(io_error)?;
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    let schedule = parity_schedule_json();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    let preregistration = concat!(
        "{\"schema\":\"layerfs-stage1m-parity-preregistration-v1\",",
        "\"status\":\"PASS\",\"sizes_mib\":[24],\"pairs\":4,",
        "\"warmups\":8,\"measured\":8,\"p50\":\"mean_positions_2_3\",",
        "\"p95\":\"position_4\",\"preferred_wall_ns\":5000000000,",
        "\"hard_wall_ns\":10000000000}\n"
    );
    durable_write(
        &run.join("preregistration.json"),
        preregistration.as_bytes(),
    )?;
    durable_write(&run.join("readiness.json"), &readiness_bytes)?;
    let fixture_manifest = find_fixture_manifest(&source)?;
    durable_write(
        &run.join("fixture-manifest.json"),
        &fs::read(fixture_manifest).map_err(io_error)?,
    )?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(
                &std::env::current_dir()
                    .map_err(io_error)?
                    .display()
                    .to_string()
            )
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            concat!(
                "{{\"historical_harness\":{{\"path\":\"{}\",\"sha256\":\"{}\",",
                "\"blake3\":\"{}\"}},\"instrumented_control\":{{\"path\":\"{}\",",
                "\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n"
            ),
            json_escape(&historical.display().to_string()),
            historical_sha256,
            digest_file(&historical)?,
            json_escape(&instrumented.display().to_string()),
            instrumented_sha256,
            digest_file(&instrumented)?,
        )
        .as_bytes(),
    )?;
    copy_global_manifests(run)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.json"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let orders = [["H", "I"], ["I", "H"], ["I", "H"], ["H", "I"]];
    let mut historical_walls = Vec::new();
    let mut instrumented_walls = Vec::new();
    let mut command_sequence = 0_u64;
    for (pair_index, pair) in orders.iter().enumerate() {
        let pair_number = pair_index + 1;
        let mut pair_comparable = Vec::new();
        for (order_index, role) in pair.iter().enumerate() {
            command_sequence += 1;
            let order_number = order_index + 1;
            let executable = if *role == "H" {
                &historical
            } else {
                &instrumented
            };
            let executable_sha256 = if *role == "H" {
                &historical_sha256
            } else {
                &instrumented_sha256
            };
            let identity = format!("{role}-p{pair_number}-o{order_number}");
            let work = run.join(format!(".sample-{identity}"));
            let started = Instant::now();
            let output = Command::new(executable)
                .args(["stage1", "materialize", "parity-row"])
                .arg(&store)
                .arg(&source)
                .arg("24")
                .arg(&work)
                .arg(&identity)
                .output()
                .map_err(io_error)?;
            let command_wall_ns = started.elapsed().as_nanos();
            append_command(
                run,
                command_sequence,
                pair_number,
                order_number,
                role,
                executable_sha256,
                command_wall_ns,
                &output,
            )?;
            if !output.status.success() {
                append_sync(
                    &run.join("failure-ledger.json"),
                    &format!(
                        "{{\"sequence\":2,\"state\":\"FAIL\",\"pair\":{},\"order\":{},\"operand\":\"{}\"}}",
                        pair_number, order_number, role
                    ),
                )?;
                return Err(format!("parity operand {identity} failed"));
            }
            let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
            let child_rows = stdout.lines().collect::<Vec<_>>();
            if child_rows.len() != 2 {
                return Err(format!(
                    "parity operand {identity} returned {} rows",
                    child_rows.len()
                ));
            }
            for child_row in &child_rows {
                append_sync(
                    &run.join("rows.jsonl"),
                    &enrich_row(
                        child_row,
                        pair_number,
                        order_number,
                        role,
                        executable_sha256,
                        command_wall_ns,
                    )?,
                )?;
            }
            let measured = child_rows[1];
            let wall = json_u128(measured, "product_operation_wall_ns")?;
            if *role == "H" {
                historical_walls.push(wall);
            } else {
                instrumented_walls.push(wall);
                validate_instrumented_row(measured)?;
            }
            pair_comparable.push(comparable_row(measured)?);
        }
        if pair_comparable[0] != pair_comparable[1] {
            return Err(format!("pair {pair_number} legacy work differs"));
        }
    }
    finish_parity(
        run,
        campaign_started.elapsed().as_nanos(),
        historical_walls,
        instrumented_walls,
    )
}
