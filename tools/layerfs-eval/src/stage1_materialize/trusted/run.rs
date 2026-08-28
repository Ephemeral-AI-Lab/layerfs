use super::super::attribution::campaign::{
    enrich_attribution_row, three_stats, validate_attribution_json,
};
use super::super::contract::{EvalResult, FILE_PATH};
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{digest_file, sha256_bytes, sha256_file};
use super::super::manifest::{clean_head_custody, source_build_manifest_json};
use super::super::parity::evidence::{append_sync, create_empty, json_string_value, json_u128};
use super::super::prepare::{durable_write, json_escape, unix_ns, verify_fixture_sources};
use super::contract::{trusted_schedule_json, TRUSTED_SCHEDULE};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub fn trusted_run(fixture: &Path, source_manifest: &Path, run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let fixture = fixture.canonicalize().map_err(io_error)?;
    let source_manifest = source_manifest.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let executable = std::env::current_exe()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let executable_sha256 = sha256_file(&executable)?;
    let executable_blake3 = digest_file(&executable)?;
    let source_manifest_bytes = fs::read(&source_manifest).map_err(io_error)?;
    let source_manifest_text =
        std::str::from_utf8(&source_manifest_bytes).map_err(display_error)?;
    let (head, _) = clean_head_custody()?;
    let build_target = PathBuf::from(json_string_value(source_manifest_text, "build_target")?);
    let build_log = PathBuf::from(json_string_value(source_manifest_text, "build_log_path")?)
        .canonicalize()
        .map_err(io_error)?;
    let build_log_bytes = fs::read(&build_log).map_err(io_error)?;
    let expected_manifest = source_build_manifest_json(
        "stage1t-trusted-measurement-final",
        &head,
        &executable,
        &build_target,
        &build_log,
    )?;
    if source_manifest_bytes != expected_manifest.as_bytes() {
        return Err(
            "source manifest bytes do not exactly match clean HEAD, build log, and running executable"
                .to_owned()
        );
    }
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let schedule = trusted_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();

    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        b"{\"schema\":\"layerfs-stage1t-preregistration-v1\",\"status\":\"PASS\",\"integrity_mode\":\"TrustedLocalDev\",\"warmups\":3,\"measured\":9,\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000}\n",
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    durable_write(&run.join("source-manifest.json"), &source_manifest_bytes)?;
    durable_write(&run.join("build.log"), &build_log_bytes)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1t-environment-v1\",",
                "\"integrity_mode\":\"TrustedLocalDev\",\"network\":0,",
                "\"rows_serial\":true,\"cwd\":\"{}\",",
                "\"git_commit\":\"{}\",\"dirty_tree\":false,",
                "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                "\"executable_blake3\":\"{}\",\"build_log_sha256\":\"{}\"}}\n"
            ),
            json_escape(
                &std::env::current_dir()
                    .map_err(io_error)?
                    .display()
                    .to_string()
            ),
            head,
            json_escape(&executable.display().to_string()),
            executable_sha256,
            executable_blake3,
            sha256_bytes(&build_log_bytes)?,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1t-readiness-v1\",\"status\":\"PASS\",",
                "\"measured_rows_started\":false,\"integrity_mode\":\"TrustedLocalDev\",",
                "\"expected_rows\":12,\"warmups\":3,\"measured\":9,",
                "\"git_commit\":\"{}\",\"dirty_tree\":false,",
                "\"executable_sha256\":\"{}\",\"source_manifest_sha256\":\"{}\",",
                "\"build_log_sha256\":\"{}\",",
                "\"fixture_manifest_sha256\":\"{}\",\"schedule_blake3\":\"{}\"}}\n"
            ),
            head,
            executable_sha256,
            sha256_bytes(&source_manifest_bytes)?,
            sha256_bytes(&build_log_bytes)?,
            sha256_bytes(&fixture_manifest)?,
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let campaign_setup_ns = campaign_started.elapsed().as_nanos();
    let mut populations = Vec::new();
    let mut statistics = Vec::new();
    let mut command_wall_sum_ns = 0_u128;
    let mut sequence = 0_u64;
    let mut maximum_rss_bytes = 0_u128;
    let mut maximum_q_bytes = 0_u128;
    let mut maximum_total_connections = 0_u128;
    let mut maximum_scratch_connections = 0_u128;
    let mut maximum_fd = 0_u128;
    let mut maximum_cpu_ns = 0_u128;

    for (block_index, size_mib) in TRUSTED_SCHEDULE.iter().enumerate() {
        if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
            return Err("TrustedLocalDev campaign hard wall reached".to_owned());
        }
        let size = fixture.join("sizes").join(size_mib.to_string());
        let store = size.join("bases/base");
        let source = size.join("source-native").join(FILE_PATH);
        let identity = format!("trusted-b{:02}-{size_mib}", block_index + 1);
        let work = run.join(format!(".block-{identity}"));
        let size_arg = size_mib.to_string();
        let argv = [
            executable.display().to_string(),
            "stage1".to_owned(),
            "materialize".to_owned(),
            "trusted-block".to_owned(),
            store.display().to_string(),
            source.display().to_string(),
            size_arg.clone(),
            work.display().to_string(),
            identity.clone(),
        ];
        let argv_json = argv
            .iter()
            .map(|argument| format!("\"{}\"", json_escape(argument)))
            .collect::<Vec<_>>()
            .join(",");
        let started_unix_ns = unix_ns()?;
        let started = Instant::now();
        let output = Command::new(&executable)
            .args(["stage1", "materialize", "trusted-block"])
            .arg(&store)
            .arg(&source)
            .arg(&size_arg)
            .arg(&work)
            .arg(&identity)
            .output()
            .map_err(io_error)?;
        let command_wall_ns = started.elapsed().as_nanos();
        command_wall_sum_ns = command_wall_sum_ns
            .checked_add(command_wall_ns)
            .ok_or_else(|| "command wall sum overflow".to_owned())?;
        let completed_unix_ns = unix_ns()?;
        append_sync(
            &run.join("commands.jsonl"),
            &format!(
                concat!(
                    "{{\"sequence\":{},\"block\":{},\"integrity_mode\":\"TrustedLocalDev\",",
                    "\"size_mib\":{},\"executable_sha256\":\"{}\",\"argv\":[{}],",
                    "\"start_unix_ns\":{},\"end_unix_ns\":{},\"wall_ns\":{},",
                    "\"exit_code\":{},\"stderr\":\"{}\"}}"
                ),
                block_index + 1,
                block_index + 1,
                size_mib,
                executable_sha256,
                argv_json,
                started_unix_ns,
                completed_unix_ns,
                command_wall_ns,
                output.status.code().unwrap_or(-1),
                json_escape(&String::from_utf8_lossy(&output.stderr)),
            ),
        )?;
        if !output.status.success() {
            append_sync(
                &run.join("failure-ledger.json"),
                &format!(
                    "{{\"sequence\":2,\"state\":\"FAIL\",\"block\":{},\"reason\":\"trusted block failed\"}}",
                    block_index + 1
                ),
            )?;
            return Err(format!("TrustedLocalDev block {} failed", block_index + 1));
        }
        let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
        let rows = stdout.lines().collect::<Vec<_>>();
        if rows.len() != 4
            || !rows[0].contains("\"row_kind\":\"warmup\"")
            || rows[1..]
                .iter()
                .any(|row| !row.contains("\"row_kind\":\"measured\""))
        {
            return Err("TrustedLocalDev block population is not 1+3".to_owned());
        }
        let terminal = rows[3];
        if json_u128(terminal, "fd_terminal")? != json_u128(terminal, "process_fd_baseline")?
            || json_u128(terminal, "connections_terminal")? != 0
            || json_u128(terminal, "scratch_connections_terminal")? != 0
            || json_u128(terminal, "total_connections_terminal")? != 0
            || json_u128(terminal, "operation_q_terminal_bytes")? != 0
            || json_u128(terminal, "residue")? != 0
        {
            return Err("TrustedLocalDev block terminal resources did not close".to_owned());
        }
        let mut measured = Vec::new();
        for row in rows {
            validate_attribution_json(row)?;
            if !row.contains("\"integrity_mode\":\"TrustedLocalDev\"")
                || json_u128(row, "authentication_passes")? != 0
                || json_u128(row, "identity_authentication_ns")? != 0
                || json_u128(row, "fetched_rows")? != json_u128(row, "role_decode_passes")?
            {
                return Err("TrustedLocalDev row trust equation failed".to_owned());
            }
            maximum_rss_bytes = maximum_rss_bytes.max(json_u128(row, "rss_peak_bytes")?);
            maximum_q_bytes = maximum_q_bytes.max(json_u128(row, "operation_q_high_water_bytes")?);
            maximum_total_connections =
                maximum_total_connections.max(json_u128(row, "total_connections_peak")?);
            maximum_scratch_connections =
                maximum_scratch_connections.max(json_u128(row, "scratch_connections_peak")?);
            maximum_fd = maximum_fd
                .max(json_u128(row, "fd_before")?)
                .max(json_u128(row, "fd_after")?);
            maximum_cpu_ns = maximum_cpu_ns.max(
                json_u128(row, "user_cpu_ns")?
                    .checked_add(json_u128(row, "system_cpu_ns")?)
                    .ok_or_else(|| "CPU total overflow".to_owned())?,
            );
            sequence += 1;
            append_sync(
                &run.join("rows.jsonl"),
                &enrich_attribution_row(
                    row,
                    sequence,
                    block_index + 1,
                    &executable_sha256,
                    &schedule_blake3,
                    command_wall_ns,
                )?,
            )?;
            if row.contains("\"row_kind\":\"measured\"") {
                measured.push(json_u128(row, "product_operation_wall_ns")?);
            }
        }
        let (p50, p95) = three_stats(&measured)?;
        statistics.push((*size_mib, p50, p95));
        populations.push(format!(
            "{{\"size_mib\":{},\"raw_ns\":{:?},\"p50_ns\":{},\"p95_ns\":{}}}",
            size_mib, measured, p50, p95
        ));
    }
    if sequence != 12 {
        return Err("TrustedLocalDev campaign row population is not 12".to_owned());
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    let command_records = commands.lines().collect::<Vec<_>>();
    if command_records.len() != 3 {
        return Err("TrustedLocalDev command population is not 3".to_owned());
    }
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", command_records.join(",")).as_bytes(),
    )?;

    let time = |size| {
        statistics
            .iter()
            .find(|(candidate, _, _)| *candidate == size)
            .map(|(_, p50, p95)| (*p50, *p95))
            .ok_or_else(|| format!("missing TrustedLocalDev {size} MiB population"))
    };
    let (t0, t0_p95) = time(0)?;
    let (t24, t24_p95) = time(24)?;
    let (t96, t96_p95) = time(96)?;
    let slope = (t96 as f64 - t24 as f64) / 72.0;
    if slope <= 0.0 {
        return Err("TrustedLocalDev fitted slope is not positive".to_owned());
    }
    let fitted_intercept = t24 as f64 - 24.0 * slope;
    let sustained_mib_s = 1_000_000_000_f64 / slope;
    let residual0 = t0 as f64 - fitted_intercept;
    let model_valid = residual0.abs() <= 2_000_000_f64.max(t0 as f64 * 0.05);
    let fixed_target_pass = fitted_intercept < 20_000_000.0;
    let sustained_target_pass = sustained_mib_s >= 500.0;
    let p50_24_mib_s = 24_000_000_000_f64 / t24 as f64;
    let p95_24_mib_s = 24_000_000_000_f64 / t24_p95 as f64;
    let p50_96_mib_s = 96_000_000_000_f64 / t96 as f64;
    let p95_96_mib_s = 96_000_000_000_f64 / t96_p95 as f64;
    let primary_target_pass = p50_24_mib_s >= 450.0
        && p95_24_mib_s >= 24_000.0 / 58.667
        && p50_96_mib_s >= 450.0
        && p95_96_mib_s >= 96_000.0 / 234.667;
    let resource_gates_pass = maximum_rss_bytes < 32 * 1024 * 1024
        && maximum_q_bytes < 8 * 1024 * 1024
        && maximum_scratch_connections <= 1
        && maximum_total_connections <= 2;
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    let campaign_coordinator_ns = campaign_wall_ns
        .checked_sub(campaign_setup_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "campaign wall equation underflow".to_owned())?;
    if campaign_wall_ns >= 30_000_000_000 {
        return Err("TrustedLocalDev campaign exceeded the hard wall".to_owned());
    }
    let preferred_wall_pass = campaign_wall_ns < 15_000_000_000;
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1t-summary-v1\",\"status\":\"PASS\",",
            "\"integrity_mode\":\"TrustedLocalDev\",\"warmup_rows\":3,",
            "\"measured_rows\":9,\"population_exact\":true,",
            "\"campaign_wall_ns\":{},\"campaign_setup_ns\":{},",
            "\"command_wall_sum_ns\":{},\"campaign_coordinator_ns\":{},",
            "\"campaign_wall_equation_exact\":true,\"preferred_wall_pass\":{},",
            "\"hard_wall_pass\":true,\"primary_target_pass\":{},",
            "\"fixed_target_pass\":{},\"sustained_target_pass\":{},",
            "\"performance\":{{\"zero_p50_ns\":{},\"zero_p95_ns\":{},",
            "\"24_mib\":{{\"p50_ns\":{},\"p95_ns\":{},\"p50_mib_s\":{},\"p95_mib_s\":{}}},",
            "\"96_mib\":{{\"p50_ns\":{},\"p95_ns\":{},\"p50_mib_s\":{},\"p95_mib_s\":{}}},",
            "\"fitted_intercept_ns\":{},\"slope_ns_per_mib\":{},",
            "\"fitted_sustained_mib_s\":{},\"zero_residual_ns\":{},",
            "\"model_valid\":{}}},",
            "\"resources\":{{\"maximum_rss_bytes\":{},\"maximum_q_bytes\":{},",
            "\"maximum_scratch_connections\":{},\"maximum_total_connections\":{},",
            "\"maximum_fd\":{},\"maximum_row_cpu_ns\":{},",
            "\"terminal_primary_connections\":0,\"terminal_scratch_connections\":0,",
            "\"terminal_total_connections\":0,\"terminal_q_bytes\":0,",
            "\"residue\":0,\"resource_gates_pass\":{}}},",
            "\"populations\":[{}]}}\n"
        ),
        campaign_wall_ns,
        campaign_setup_ns,
        command_wall_sum_ns,
        campaign_coordinator_ns,
        preferred_wall_pass,
        primary_target_pass,
        fixed_target_pass,
        sustained_target_pass,
        t0,
        t0_p95,
        t24,
        t24_p95,
        p50_24_mib_s,
        p95_24_mib_s,
        t96,
        t96_p95,
        p50_96_mib_s,
        p95_96_mib_s,
        fitted_intercept,
        slope,
        sustained_mib_s,
        residual0,
        model_valid,
        maximum_rss_bytes,
        maximum_q_bytes,
        maximum_scratch_connections,
        maximum_total_connections,
        maximum_fd,
        maximum_cpu_ns,
        resource_gates_pass,
        populations.join(","),
    );
    durable_write(&run.join("summary.json"), summary.as_bytes())?;
    durable_write(
        &run.join("summary.md"),
        format!(
            concat!(
                "# Stage 1.1T TrustedLocalDev materialization\n\n",
                "Status: **PASS evidence population**. This is not Verified.\n\n",
                "| Size | p50 ms | p95 ms | p50 MiB/s | p95 MiB/s |\n",
                "|---:|---:|---:|---:|---:|\n",
                "| 0 MiB | {:.6} | {:.6} | N/A | N/A |\n",
                "| 24 MiB | {:.6} | {:.6} | {:.3} | {:.3} |\n",
                "| 96 MiB | {:.6} | {:.6} | {:.3} | {:.3} |\n\n",
                "Fitted intercept: `{:.6} ms`; fitted sustained: `{:.3} MiB/s`. ",
                "Primary target pass: `{}`. Campaign wall: `{}` ns.\n"
            ),
            t0 as f64 / 1_000_000.0,
            t0_p95 as f64 / 1_000_000.0,
            t24 as f64 / 1_000_000.0,
            t24_p95 as f64 / 1_000_000.0,
            p50_24_mib_s,
            p95_24_mib_s,
            t96 as f64 / 1_000_000.0,
            t96_p95 as f64 / 1_000_000.0,
            p50_96_mib_s,
            p95_96_mib_s,
            fitted_intercept / 1_000_000.0,
            sustained_mib_s,
            primary_target_pass,
            campaign_wall_ns,
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1t-campaign-time-v1\nstatus=PASS\ncampaign_wall_ns={campaign_wall_ns}\ncampaign_setup_ns={campaign_setup_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncampaign_coordinator_ns={campaign_coordinator_ns}\ncampaign_wall_equation_exact=true\nwarmups=3\nmeasured=9\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n"
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
                "{{\"schema\":\"layerfs-stage1t-terminal-receipt-v1\",\"status\":\"PASS\",",
                "\"integrity_mode\":\"TrustedLocalDev\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"source_manifest_sha256\":\"{}\",",
                "\"build_log_sha256\":\"{}\",\"git_commit\":\"{}\",",
                "\"executable_sha256\":\"{}\"}}\n"
            ),
            sha256_file(&run.join("rows.jsonl"))?,
            sha256_file(&run.join("commands.json"))?,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("source-manifest.json"))?,
            sha256_file(&run.join("build.log"))?,
            head,
            executable_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1t-trusted-run status=PASS run={} wall_ns={} primary_target_pass={}",
        run.display(),
        campaign_wall_ns,
        primary_target_pass,
    );
    Ok(())
}
