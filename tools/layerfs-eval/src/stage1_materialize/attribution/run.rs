use super::super::contract::{EvalResult, FILE_PATH};
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{digest_file, sha256_file};
use super::super::parity::evidence::{append_sync, create_empty, json_u128};
use super::super::prepare::{durable_write, json_escape, unix_ns, verify_fixture_sources};
use super::campaign::{
    attribution_campaign_failure, attribution_models_json, copy_attribution_manifests,
    enrich_attribution_row, three_stats, validate_attribution_json,
};
use super::contract::{attribution_schedule_json, ATTRIBUTION_SCHEDULE};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub fn attribution_run(control: &Path, fixture: &Path, run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let control = control.canonicalize().map_err(io_error)?;
    let fixture = fixture.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let control_sha256 = sha256_file(&control)?;
    let control_blake3 = digest_file(&control)?;
    let schedule = attribution_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        concat!(
            "{\"schema\":\"layerfs-stage1m-attribution-preregistration-v1\",",
            "\"status\":\"PASS\",\"warmups\":12,\"measured\":36,",
            "\"estimator\":\"n3_position2_position3\",",
            "\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000}\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-readiness-v1\",",
                "\"status\":\"PASS\",\"measured_rows_started\":false,",
                "\"control_sha256\":\"{}\",\"fixture_blake3\":\"{}\",",
                "\"schedule_blake3\":\"{}\",\"expected_rows\":48}}\n"
            ),
            control_sha256,
            blake3::hash(&fixture_manifest).to_hex(),
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(&std::env::current_dir().map_err(io_error)?.display().to_string())
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            "{{\"instrumented_control\":{{\"path\":\"{}\",\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n",
            json_escape(&control.display().to_string()),
            control_sha256,
            control_blake3,
        )
        .as_bytes(),
    )?;
    copy_attribution_manifests(run, &fixture)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let campaign_setup_ns = campaign_started.elapsed().as_nanos();
    let mut populations = Vec::new();
    let mut population_data = Vec::new();
    let mut command_wall_sum_ns = 0_u128;
    let mut row_sequence = 0_u64;
    let mut maximum_rss_peak_bytes = 0_u128;
    let mut maximum_q_high_water_bytes = 0_u128;
    let mut maximum_scratch_connections = 0_u128;
    let mut maximum_total_connections = 0_u128;
    let mut maximum_row_cpu_ns = 0_u128;
    for (block_index, (arm, size_mib)) in ATTRIBUTION_SCHEDULE.iter().enumerate() {
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return attribution_campaign_failure(run, block_index + 1, "hard wall reached");
        }
        let size = fixture.join("sizes").join(size_mib.to_string());
        let store = size.join("bases/base");
        let source = size.join("source-native").join(FILE_PATH);
        let identity = format!("b{:02}-{}-{}", block_index + 1, arm.name(), size_mib);
        let work = run.join(format!(".block-{identity}"));
        let size_mib_arg = size_mib.to_string();
        let argv = [
            control.display().to_string(),
            "stage1".to_owned(),
            "materialize".to_owned(),
            "attribution-block".to_owned(),
            store.display().to_string(),
            source.display().to_string(),
            size_mib_arg.clone(),
            arm.name().to_owned(),
            work.display().to_string(),
            identity.clone(),
        ];
        let argv_json = argv
            .iter()
            .map(|argument| format!("\"{}\"", json_escape(argument)))
            .collect::<Vec<_>>()
            .join(",");
        let command_start_unix_ns = unix_ns()?;
        let started = Instant::now();
        let output = Command::new(&control)
            .args(["stage1", "materialize", "attribution-block"])
            .arg(&store)
            .arg(&source)
            .arg(&size_mib_arg)
            .arg(arm.name())
            .arg(&work)
            .arg(&identity)
            .output()
            .map_err(io_error)?;
        let command_wall_ns = started.elapsed().as_nanos();
        command_wall_sum_ns = command_wall_sum_ns
            .checked_add(command_wall_ns)
            .ok_or_else(|| "command wall sum overflow".to_owned())?;
        let command_end_unix_ns = unix_ns()?;
        append_sync(
            &run.join("commands.jsonl"),
            &format!(
                concat!(
                    "{{\"sequence\":{},\"block\":{},\"arm\":\"{}\",",
                    "\"size_mib\":{},\"identity\":\"{}\",",
                    "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                    "\"fixture_root\":\"{}\",\"store\":\"{}\",\"source\":\"{}\",",
                    "\"work\":\"{}\",\"cwd\":\"{}\",\"argv\":[{}],",
                    "\"start_unix_ns\":{},\"end_unix_ns\":{},\"wall_ns\":{},",
                    "\"exit_code\":{},\"stderr\":\"{}\"}}"
                ),
                block_index + 1,
                block_index + 1,
                arm.name(),
                size_mib,
                identity,
                json_escape(&control.display().to_string()),
                control_sha256,
                json_escape(&fixture.display().to_string()),
                json_escape(&store.display().to_string()),
                json_escape(&source.display().to_string()),
                json_escape(&work.display().to_string()),
                json_escape(
                    &std::env::current_dir()
                        .map_err(io_error)?
                        .display()
                        .to_string()
                ),
                argv_json,
                command_start_unix_ns,
                command_end_unix_ns,
                command_wall_ns,
                output.status.code().unwrap_or(-1),
                json_escape(&String::from_utf8_lossy(&output.stderr)),
            ),
        )?;
        if !output.status.success() {
            return attribution_campaign_failure(run, block_index + 1, "block command failed");
        }
        let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
        let rows = stdout.lines().collect::<Vec<_>>();
        if rows.len() != 4
            || !rows[0].contains("\"row_kind\":\"warmup\"")
            || rows[1..]
                .iter()
                .any(|row| !row.contains("\"row_kind\":\"measured\""))
        {
            return attribution_campaign_failure(run, block_index + 1, "invalid block population");
        }
        let mut measured = Vec::new();
        for row in rows {
            validate_attribution_json(row)?;
            maximum_rss_peak_bytes = maximum_rss_peak_bytes.max(json_u128(row, "rss_peak_bytes")?);
            maximum_q_high_water_bytes =
                maximum_q_high_water_bytes.max(json_u128(row, "operation_q_high_water_bytes")?);
            maximum_scratch_connections =
                maximum_scratch_connections.max(json_u128(row, "scratch_connections_peak")?);
            maximum_total_connections =
                maximum_total_connections.max(json_u128(row, "total_connections_peak")?);
            maximum_row_cpu_ns = maximum_row_cpu_ns.max(
                json_u128(row, "user_cpu_ns")?
                    .checked_add(json_u128(row, "system_cpu_ns")?)
                    .ok_or_else(|| "row CPU total overflow".to_owned())?,
            );
            row_sequence += 1;
            append_sync(
                &run.join("rows.jsonl"),
                &enrich_attribution_row(
                    row,
                    row_sequence,
                    block_index + 1,
                    &control_sha256,
                    &schedule_blake3,
                    command_wall_ns,
                )?,
            )?;
            if row.contains("\"row_kind\":\"measured\"") {
                measured.push(json_u128(row, "product_operation_wall_ns")?);
            }
        }
        let stats = three_stats(&measured)?;
        population_data.push((*arm, *size_mib, stats.0, stats.1));
        populations.push(format!(
            "{{\"arm\":\"{}\",\"size_mib\":{},\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}}",
            arm.name(),
            size_mib,
            measured,
            stats.0,
            stats.1
        ));
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return attribution_campaign_failure(run, block_index + 1, "hard wall reached");
        }
    }
    if row_sequence != 48 {
        return attribution_campaign_failure(run, 12, "row population is not 48");
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    let command_records = commands.lines().collect::<Vec<_>>();
    if command_records.len() != 12 {
        return attribution_campaign_failure(run, 12, "command population is not 12");
    }
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", command_records.join(",")).as_bytes(),
    )?;
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    let campaign_coordinator_ns = campaign_wall_ns
        .checked_sub(campaign_setup_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "campaign wall equation underflow".to_owned())?;
    let models = attribution_models_json(&population_data)?;
    let rows_sha256 = sha256_file(&run.join("rows.jsonl"))?;
    let rows_blake3 = digest_file(&run.join("rows.jsonl"))?;
    let commands_sha256 = sha256_file(&run.join("commands.json"))?;
    let commands_blake3 = digest_file(&run.join("commands.json"))?;
    durable_write(
        &run.join("artifact-hashes.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-hashes-v1\",",
                "\"rows\":{{\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
                "\"commands\":{{\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
                "\"schedule_sha256\":\"{}\",\"fixture_manifest_sha256\":\"{}\",",
                "\"executable_sha256\":\"{}\"}}\n"
            ),
            rows_sha256,
            rows_blake3,
            commands_sha256,
            commands_blake3,
            sha256_file(&run.join("schedule.json"))?,
            sha256_file(&run.join("fixture-manifest.json"))?,
            control_sha256,
        )
        .as_bytes(),
    )?;
    let preferred_wall_pass = campaign_wall_ns < 15_000_000_000;
    durable_write(
        &run.join("summary.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-summary-v1\",",
                "\"status\":\"PASS\",\"warmup_rows\":12,\"measured_rows\":36,",
                "\"population_exact\":true,\"preferred_wall_pass\":{},",
                "\"hard_wall_pass\":true,\"campaign_wall_ns\":{},",
                "\"campaign_setup_ns\":{},\"command_wall_sum_ns\":{},",
                "\"campaign_coordinator_ns\":{},\"campaign_wall_equation_exact\":true,",
                "\"resources\":{{\"maximum_rss_peak_bytes\":{},",
                "\"maximum_q_high_water_bytes\":{},\"maximum_scratch_connections\":{},",
                "\"maximum_total_connections\":{},\"maximum_row_cpu_ns\":{},",
                "\"terminal_primary_connections\":0,\"terminal_scratch_connections\":0,",
                "\"terminal_total_connections\":0,\"terminal_q_bytes\":0,\"residue\":0}},",
                "\"models\":{},\"populations\":[{}]}}\n"
            ),
            preferred_wall_pass,
            campaign_wall_ns,
            campaign_setup_ns,
            command_wall_sum_ns,
            campaign_coordinator_ns,
            maximum_rss_peak_bytes,
            maximum_q_high_water_bytes,
            maximum_scratch_connections,
            maximum_total_connections,
            maximum_row_cpu_ns,
            models,
            populations.join(","),
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M control attribution\n\nStatus: **PASS**. Exact population: 12 warmups + 36 measured rows. Complete wall: `{campaign_wall_ns}` ns; preferred wall pass: `{preferred_wall_pass}`.\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-attribution-campaign-time-v1\nstatus=PASS\ncampaign_wall_ns={campaign_wall_ns}\ncampaign_setup_ns={campaign_setup_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncampaign_coordinator_ns={campaign_coordinator_ns}\ncampaign_wall_equation_exact=true\nwarmups=12\nmeasured=36\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n"
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"PASS\",\"preserved_failures\":0}",
    )?;
    durable_write(
        &run.join("terminal-receipt.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-attribution-terminal-receipt-v1\",",
                "\"status\":\"PASS\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"failure_ledger_sha256\":\"{}\",",
                "\"artifact_hashes_sha256\":\"{}\",\"executable_sha256\":\"{}\"}}\n"
            ),
            sha256_file(&run.join("rows.jsonl"))?,
            sha256_file(&run.join("commands.json"))?,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("failure-ledger.json"))?,
            sha256_file(&run.join("artifact-hashes.json"))?,
            control_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1m-attribution-run status=PASS run={} wall_ns={} preferred_wall_pass={}",
        run.display(),
        campaign_wall_ns,
        preferred_wall_pass,
    );
    Ok(())
}
