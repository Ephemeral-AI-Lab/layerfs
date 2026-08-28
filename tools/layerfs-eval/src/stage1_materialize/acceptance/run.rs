use super::super::contract::{EvalResult, FILE_PATH};
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{digest_file, sha256_file};
use super::super::parity::evidence::{append_sync, create_empty, json_u128};
use super::super::prepare::{durable_write, json_escape, unix_ns, verify_fixture_sources};
use super::campaign::{
    acceptance_campaign_failure, acceptance_semantic_signature, copy_acceptance_manifests,
    enrich_acceptance_row, validate_acceptance_row,
};
use super::contract::{acceptance_schedule_json, AcceptanceSample, ACCEPTANCE_SCHEDULE};
use super::disposition::acceptance_disposition;
use super::summary::acceptance_summary_json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub fn acceptance_run(
    control: &Path,
    candidate: &Path,
    fixture: &Path,
    run: &Path,
) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("run directory already exists: {}", run.display()));
    }
    let control = control.canonicalize().map_err(io_error)?;
    let candidate = candidate.canonicalize().map_err(io_error)?;
    let fixture = fixture.canonicalize().map_err(io_error)?;
    crate::stage1_fixture::verify_sealed(&fixture)?;
    verify_fixture_sources(&fixture)?;
    let fixture_manifest = fs::read(fixture.join("fixture-manifest.json")).map_err(io_error)?;
    let control_sha256 = sha256_file(&control)?;
    let candidate_sha256 = sha256_file(&candidate)?;
    let control_blake3 = digest_file(&control)?;
    let candidate_blake3 = digest_file(&candidate)?;
    if control_sha256 == candidate_sha256 {
        return Err("control and candidate executables are identical".to_owned());
    }
    let manifest_directory = fixture
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "fixture is not below the target directory".to_owned())?
        .join("layerfs-stage1m-custody/source-manifests");
    for (name, executable_sha256) in [
        ("source-manifest-control.json", control_sha256.as_str()),
        ("source-manifest-candidate.json", candidate_sha256.as_str()),
    ] {
        let manifest = fs::read_to_string(manifest_directory.join(name)).map_err(io_error)?;
        if !manifest.contains(executable_sha256)
            || !manifest.contains("\"dirty_tree\":false")
            || !manifest.contains("\"status\":\"PASS\"")
        {
            return Err(format!("{name} does not bind its clean executable"));
        }
    }
    let schedule = acceptance_schedule_json();
    let schedule_blake3 = blake3::hash(schedule.as_bytes()).to_hex().to_string();
    fs::create_dir(run).map_err(io_error)?;
    let campaign_started = Instant::now();
    durable_write(&run.join("schedule.json"), schedule.as_bytes())?;
    durable_write(
        &run.join("preregistration.json"),
        concat!(
            "{\"schema\":\"layerfs-stage1m-acceptance-preregistration-v1\",",
            "\"status\":\"PASS\",\"paired_warmups\":24,\"measured\":24,",
            "\"p50\":\"mean_positions_2_3\",\"p95\":\"position_4\",",
            "\"preferred_wall_ns\":15000000000,\"hard_wall_ns\":30000000000,",
            "\"wins_required_24\":3,\"wins_required_96\":3,",
            "\"fixed_cost_allowance_ns\":1000000,\"p95_allowance_ns\":1000000}\n"
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("readiness.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-acceptance-readiness-v1\",",
                "\"status\":\"PASS\",\"measured_rows_started\":false,",
                "\"control_sha256\":\"{}\",\"candidate_sha256\":\"{}\",",
                "\"fixture_blake3\":\"{}\",\"schedule_blake3\":\"{}\",",
                "\"expected_rows\":48,\"control_resource_derivations\":",
                "{{\"q_high_water_bytes\":\"source_bound_8_mib\",",
                "\"scratch_connections_peak\":\"scratch.tables\",",
                "\"total_connections_peak\":\"active_connections_plus_scratch.tables\"}}}}\n"
            ),
            control_sha256,
            candidate_sha256,
            blake3::hash(&fixture_manifest).to_hex(),
            schedule_blake3,
        )
        .as_bytes(),
    )?;
    durable_write(&run.join("fixture-manifest.json"), &fixture_manifest)?;
    let cwd = std::env::current_dir().map_err(io_error)?;
    durable_write(
        &run.join("environment.json"),
        format!(
            "{{\"schema\":\"layerfs-stage1m-environment-v1\",\"network\":0,\"rows_serial\":true,\"cwd\":\"{}\"}}\n",
            json_escape(&cwd.display().to_string())
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("executables.json"),
        format!(
            concat!(
                "{{\"control\":{{\"path\":\"{}\",\"sha256\":\"{}\",",
                "\"blake3\":\"{}\"}},\"candidate\":{{\"path\":\"{}\",",
                "\"sha256\":\"{}\",\"blake3\":\"{}\"}}}}\n"
            ),
            json_escape(&control.display().to_string()),
            control_sha256,
            control_blake3,
            json_escape(&candidate.display().to_string()),
            candidate_sha256,
            candidate_blake3,
        )
        .as_bytes(),
    )?;
    copy_acceptance_manifests(run, &fixture)?;
    create_empty(&run.join("rows.jsonl"))?;
    create_empty(&run.join("commands.jsonl"))?;
    append_sync(
        &run.join("failure-ledger.json"),
        "{\"sequence\":1,\"state\":\"OPEN\",\"preserved_failures\":0}",
    )?;

    let setup_wall_ns = campaign_started.elapsed().as_nanos();
    let mut command_wall_sum_ns = 0_u128;
    let mut row_sequence = 0_u64;
    let mut command_sequence = 0_u64;
    let mut samples = Vec::with_capacity(24);
    for (block_index, block) in ACCEPTANCE_SCHEDULE.iter().enumerate() {
        let mut signatures = Vec::with_capacity(2);
        for (order_index, operand) in block.order.iter().enumerate() {
            if campaign_started.elapsed().as_nanos() >= 30_000_000_000 {
                return acceptance_campaign_failure(run, block_index + 1, "hard wall reached");
            }
            command_sequence += 1;
            let (executable, executable_sha256, source_role) = if *operand == 'A' {
                (&control, &control_sha256, "control")
            } else {
                (&candidate, &candidate_sha256, "candidate")
            };
            let size = fixture.join("sizes").join(block.size_mib.to_string());
            let store = size.join("bases/base");
            let source = size.join("source-native").join(FILE_PATH);
            let identity = format!(
                "p{}-s{}-o{}-{}",
                block.pair,
                block.size_mib,
                order_index + 1,
                operand
            );
            let work = run.join(format!(".sample-{identity}"));
            let size_arg = block.size_mib.to_string();
            let argv = [
                executable.display().to_string(),
                "stage1".to_owned(),
                "materialize".to_owned(),
                "parity-row".to_owned(),
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
            let start_unix_ns = unix_ns()?;
            let started = Instant::now();
            let output = Command::new(executable)
                .args(["stage1", "materialize", "parity-row"])
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
                .ok_or_else(|| "acceptance command wall overflow".to_owned())?;
            append_sync(
                &run.join("commands.jsonl"),
                &format!(
                    concat!(
                        "{{\"sequence\":{},\"block\":{},\"pair\":{},",
                        "\"pair_size_order\":{},\"operand\":\"{}\",",
                        "\"source_role\":\"{}\",\"size_mib\":{},",
                        "\"executable\":\"{}\",\"executable_sha256\":\"{}\",",
                        "\"fixture_root\":\"{}\",\"store\":\"{}\",",
                        "\"source\":\"{}\",\"work\":\"{}\",\"cwd\":\"{}\",",
                        "\"argv\":[{}],\"start_unix_ns\":{},\"end_unix_ns\":{},",
                        "\"wall_ns\":{},\"exit_code\":{},\"stderr\":\"{}\"}}"
                    ),
                    command_sequence,
                    block_index + 1,
                    block.pair,
                    order_index + 1,
                    operand,
                    source_role,
                    block.size_mib,
                    json_escape(&executable.display().to_string()),
                    executable_sha256,
                    json_escape(&fixture.display().to_string()),
                    json_escape(&store.display().to_string()),
                    json_escape(&source.display().to_string()),
                    json_escape(&work.display().to_string()),
                    json_escape(&cwd.display().to_string()),
                    argv_json,
                    start_unix_ns,
                    unix_ns()?,
                    command_wall_ns,
                    output.status.code().unwrap_or(-1),
                    json_escape(&String::from_utf8_lossy(&output.stderr)),
                ),
            )?;
            let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
            let rows = stdout.lines().collect::<Vec<_>>();
            for (child_index, row) in rows.iter().enumerate() {
                row_sequence += 1;
                append_sync(
                    &run.join("rows.jsonl"),
                    &enrich_acceptance_row(
                        row,
                        row_sequence,
                        block_index + 1,
                        block,
                        order_index + 1,
                        *operand,
                        source_role,
                        executable_sha256,
                        &schedule_blake3,
                        command_wall_ns,
                    )?,
                )?;
                if output.status.success() {
                    if let Err(error) = validate_acceptance_row(row, *operand == 'B') {
                        return acceptance_campaign_failure(
                            run,
                            block_index + 1,
                            &format!("row validation failed: {error}"),
                        );
                    }
                    if child_index == 1 {
                        let measured = (|| {
                            let signature = acceptance_semantic_signature(row)?;
                            let cpu_ns = json_u128(row, "user_cpu_ns")?
                                .checked_add(json_u128(row, "system_cpu_ns")?)
                                .ok_or_else(|| "acceptance CPU overflow".to_owned())?;
                            let wall_ns = json_u128(row, "product_operation_wall_ns")?;
                            Ok::<_, String>((
                                signature,
                                cpu_ns,
                                wall_ns,
                                json_u128(row, "rss_peak_bytes")?,
                                if *operand == 'A' {
                                    8 * 1024 * 1024
                                } else {
                                    json_u128(row, "operation_q_high_water_bytes")?
                                },
                                json_u128(row, "fd_before")?.max(json_u128(row, "fd_after")?),
                                json_u128(row, "active_connections")?,
                                if *operand == 'A' {
                                    json_u128(row, "tables")?
                                } else {
                                    json_u128(row, "scratch_connections_peak")?
                                },
                                if *operand == 'A' {
                                    json_u128(row, "active_connections")?
                                        .checked_add(json_u128(row, "tables")?)
                                        .ok_or_else(|| {
                                            "control connection peak overflow".to_owned()
                                        })?
                                } else {
                                    json_u128(row, "total_connections_peak")?
                                },
                                json_u128(row, "sync_calls")?,
                                json_u128(row, "residue")?,
                            ))
                        })();
                        let (
                            signature,
                            cpu_ns,
                            wall_ns,
                            rss_bytes,
                            q_bytes,
                            fd_peak,
                            primary_connections,
                            scratch_connections,
                            total_connections,
                            sync_calls,
                            residue,
                        ) = match measured {
                            Ok(measured) => measured,
                            Err(error) => {
                                return acceptance_campaign_failure(
                                    run,
                                    block_index + 1,
                                    &format!("measured row parsing failed: {error}"),
                                );
                            }
                        };
                        signatures.push(signature);
                        samples.push(AcceptanceSample {
                            pair: block.pair,
                            size_mib: block.size_mib,
                            operand: *operand,
                            wall_ns,
                            cpu_ns,
                            rss_bytes,
                            q_bytes,
                            fd_peak,
                            primary_connections,
                            scratch_connections,
                            total_connections,
                            sync_calls,
                            residue,
                        });
                    }
                }
            }
            if !output.status.success() {
                return acceptance_campaign_failure(
                    run,
                    block_index + 1,
                    "operand command failed; partial rows preserved",
                );
            }
            if rows.len() != 2
                || !rows[0].contains("\"row_kind\":\"warmup\"")
                || !rows[1].contains("\"row_kind\":\"measured\"")
            {
                return acceptance_campaign_failure(
                    run,
                    block_index + 1,
                    "operand population is not one warmup plus one measured",
                );
            }
        }
        if signatures.len() != 2 || signatures[0] != signatures[1] {
            return acceptance_campaign_failure(
                run,
                block_index + 1,
                "adjacent semantic work differs",
            );
        }
    }
    if row_sequence != 48 || command_sequence != 24 || samples.len() != 24 {
        return acceptance_campaign_failure(run, 12, "acceptance population mismatch");
    }
    let commands = fs::read_to_string(run.join("commands.jsonl")).map_err(io_error)?;
    durable_write(
        &run.join("commands.json"),
        format!("[{}]\n", commands.lines().collect::<Vec<_>>().join(",")).as_bytes(),
    )?;
    let disposition = match acceptance_disposition(&samples) {
        Ok(disposition) => disposition,
        Err(error) => {
            return acceptance_campaign_failure(run, 12, &format!("disposition failed: {error}"));
        }
    };
    let campaign_wall_ns = campaign_started.elapsed().as_nanos();
    if campaign_wall_ns >= 30_000_000_000 {
        return acceptance_campaign_failure(run, 12, "hard wall reached");
    }
    let coordinator_wall_ns = campaign_wall_ns
        .checked_sub(setup_wall_ns)
        .and_then(|wall| wall.checked_sub(command_wall_sum_ns))
        .ok_or_else(|| "acceptance campaign wall equation underflow".to_owned())?;
    let rows_sha256 = sha256_file(&run.join("rows.jsonl"))?;
    let commands_sha256 = sha256_file(&run.join("commands.json"))?;
    durable_write(
        &run.join("summary.json"),
        acceptance_summary_json(
            &disposition,
            campaign_wall_ns,
            setup_wall_ns,
            command_wall_sum_ns,
            coordinator_wall_ns,
        )?
        .as_bytes(),
    )?;
    durable_write(
        &run.join("summary.md"),
        format!(
            "# Stage 1.1M paired acceptance\n\nStatus: **{}**. Exact population: 24 paired warmups + 24 measured complete-public rows. 24/96 wins: `{}/{}`. Complete wall: `{campaign_wall_ns}` ns.\n",
            disposition.status, disposition.wins24, disposition.wins96
        )
        .as_bytes(),
    )?;
    durable_write(
        &run.join("campaign-time.txt"),
        format!(
            "schema=layerfs-stage1m-acceptance-campaign-time-v1\nstatus={}\ncampaign_wall_ns={campaign_wall_ns}\nsetup_wall_ns={setup_wall_ns}\ncommand_wall_sum_ns={command_wall_sum_ns}\ncoordinator_wall_ns={coordinator_wall_ns}\ncampaign_wall_equation_exact=true\npaired_warmups=24\nmeasured=24\npreferred_wall_ns=15000000000\nhard_wall_ns=30000000000\n",
            disposition.status
        )
        .as_bytes(),
    )?;
    append_sync(
        &run.join("failure-ledger.json"),
        &format!(
            "{{\"sequence\":2,\"state\":\"CLOSE\",\"status\":\"{}\",\"preserved_failures\":0}}",
            disposition.status
        ),
    )?;
    durable_write(
        &run.join("terminal-receipt.json"),
        format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1m-acceptance-terminal-receipt-v1\",",
                "\"status\":\"{}\",\"rows_sha256\":\"{}\",",
                "\"commands_sha256\":\"{}\",\"summary_sha256\":\"{}\",",
                "\"campaign_time_sha256\":\"{}\",\"failure_ledger_sha256\":\"{}\",",
                "\"schedule_sha256\":\"{}\",\"environment_sha256\":\"{}\",",
                "\"fixture_manifest_sha256\":\"{}\",\"executables_sha256\":\"{}\",",
                "\"control_manifest_sha256\":\"{}\",\"candidate_manifest_sha256\":\"{}\",",
                "\"control_sha256\":\"{}\",\"candidate_sha256\":\"{}\"}}\n"
            ),
            disposition.status,
            rows_sha256,
            commands_sha256,
            sha256_file(&run.join("summary.json"))?,
            sha256_file(&run.join("campaign-time.txt"))?,
            sha256_file(&run.join("failure-ledger.json"))?,
            sha256_file(&run.join("schedule.json"))?,
            sha256_file(&run.join("environment.json"))?,
            sha256_file(&run.join("fixture-manifest.json"))?,
            sha256_file(&run.join("executables.json"))?,
            sha256_file(&run.join("source-manifest-control.json"))?,
            sha256_file(&run.join("source-manifest-candidate.json"))?,
            control_sha256,
            candidate_sha256,
        )
        .as_bytes(),
    )?;
    println!(
        "stage1m-acceptance-run status={} run={} wall_ns={} wins24={} wins96={}",
        disposition.status,
        run.display(),
        campaign_wall_ns,
        disposition.wins24,
        disposition.wins96,
    );
    if disposition.status == "PASS" {
        Ok(())
    } else {
        Err("paired acceptance requires repair; complete evidence preserved".to_owned())
    }
}
