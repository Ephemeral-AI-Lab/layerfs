use super::super::artifact::{durable_write, json_string, json_u128, unix_ns};
use super::super::authentication::{json_array_objects, validate_authentication};
use super::super::campaign::Campaign;
use super::super::context::Disposition;
use super::super::engine_counters::FixtureMaster;
use super::super::fixture::SourceIdentity;
use super::super::limits::{FIXTURE_MTIME_SECONDS, INITIAL_BYTES};
use super::super::markdown_helpers::format_ms;
use super::super::report_disposition::campaign_time;
use super::super::row_parse::{
    json_all_u128, json_object, parse_rows, phase_wall, row_u128, validate_ref_chain,
};
use super::super::schedule::frozen_schedule;
use super::super::summary_json::summary_json;
use super::super::summary_json_contract::validate_summary_json_contract;
use super::super::summary_json_parse::validate_named_wall_equation;
use super::super::summary_markdown::summary_markdown;
use super::super::summary_markdown_contract::{validate_summary_headings, validate_summary_pair};
use super::super::validate_availability::{validate_availability_rows, validate_refresh_rows};
use super::super::validate_history::validate_history_rows;
use super::super::validate_locality::{validate_locality_rows, validate_phase_counter_rows};
use super::synthetic::{synthetic_pass_row, synthetic_root, synthetic_root_digest};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
fn generated_pass_summary_is_valid_row_derived_and_rejects_resource_mutation() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-summary-contract-{}-{}",
        std::process::id(),
        unix_ns().unwrap()
    ));
    fs::create_dir(&run).unwrap();
    for name in [
        "environment.json",
        "master.json",
        "readiness.json",
        "schedule.json",
        "campaign-time.txt",
    ] {
        durable_write(&run.join(name), "{}\n").unwrap();
    }
    let schedule = frozen_schedule().unwrap();
    let mut rows_file = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(run.join("rows.jsonl"))
        .unwrap();
    let mut row_wall_sum_ns = 0_u128;
    for scheduled in &schedule.rows {
        let row = synthetic_pass_row(&schedule, scheduled);
        row_wall_sum_ns += row.row_wall_ns;
        rows_file.write_all(row.json().unwrap().as_bytes()).unwrap();
    }
    rows_file.sync_all().unwrap();
    let rows = parse_rows(&run.join("rows.jsonl"), &schedule).unwrap();
    let executable_path = std::env::current_exe().unwrap();
    let source = SourceIdentity {
        git_commit: "0".repeat(40),
        dirty_tree: true,
        tree_blake3: "1".repeat(64),
        manifest_sha256: "2".repeat(64),
        executable_path,
        executable_sha256: "3".repeat(64),
        executable_blake3: "4".repeat(64),
    };
    let master = FixtureMaster {
        raw_digest: "5".repeat(64),
        root: synthetic_root(0),
        generation: 1,
        store_id: "6".repeat(64),
        profile: "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0".to_owned(),
        apfs_identity: "synthetic-apfs".to_owned(),
        fixture_blake3: "7".repeat(64),
        preparation_wall_ns: 1,
    };
    let campaign = Campaign {
        run: &run,
        started: Instant::now(),
        started_unix_ns: 1,
        rows: rows_file,
        schedule: &schedule,
        next_row: 47,
        row_wall_sum_ns,
        fd_baseline: 5,
        rss_peak_bytes: 20_000_000,
        q_high_water_bytes: crate::legacy_full::OPERATION_Q_BOUND_BYTES,
        q_maximum_terminal_bytes: 0,
        store_connection_high_water: 2,
        physical_oracles: 51,
        canonical_transitions: 34,
        workspace_materializations: 1,
        rematerializations: 0,
        root_digests: (0..35).map(synthetic_root_digest).collect(),
    };
    let complete = row_wall_sum_ns + 1_000;
    let summary = summary_json(
        &campaign,
        &rows,
        &source,
        &master,
        complete,
        &"8".repeat(64),
    )
    .unwrap();
    let optimized_r34 = json_object(
        json_object(&summary, "optimization").unwrap(),
        "verified_open_by_root",
    )
    .and_then(|roots| json_object(roots, "R34"))
    .unwrap();
    assert_eq!(
        json_u128(optimized_r34, "before_ns").unwrap(),
        1_406_344_708
    );
    assert_eq!(
        json_u128(optimized_r34, "after_ns").unwrap(),
        phase_wall(
            &rows
                .iter()
                .find(|row| row.row_id == "C08-001")
                .unwrap()
                .json,
            "verified_open"
        )
        .unwrap()
    );
    let failures = json_array_objects(&summary, "failures").unwrap();
    for (attempt, field) in [
        ("attempt-010", "optimization.verified_open_by_root.R34"),
        ("attempt-011", "tests.eof_post_visibility_conflict"),
    ] {
        let receipt = failures
            .iter()
            .find(|receipt| {
                json_string(receipt, "artifact").is_ok_and(|path| path.ends_with(attempt))
            })
            .unwrap();
        assert_eq!(json_string(receipt, "field").unwrap(), field);
        assert!(!json_string(receipt, "reason").unwrap().is_empty());
    }
    assert!(!summary.contains("\"count_change_amplification\":{}"));
    assert!(!summary.contains("\"by_root\":{}"));
    assert!(!summary.contains("\"by_root_range\":{}"));
    assert!(validate_summary_json_contract(&summary.replacen(
        "\"source\":",
        "\"source_missing\":",
        1
    ))
    .is_err());
    let mut child = Command::new("/usr/bin/ruby")
        .args(["-rjson", "-e", "JSON.parse(STDIN.read)"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(summary.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    let markdown = summary_markdown(&campaign, &rows, &source, &master, complete).unwrap();
    validate_summary_headings(&markdown).unwrap();
    validate_summary_pair(&summary, &markdown).unwrap();
    assert!(markdown.contains(&format!(
        "| Complete wall | `PASS` | `{} ms < 60 s` |",
        format_ms(complete)
    )));
    assert!(markdown.contains(&format!("mtime=`{}.", FIXTURE_MTIME_SECONDS + 35)));
    assert!(validate_named_wall_equation(&summary.replacen(
        "\"admission\":",
        "\"admission\":1",
        1
    ))
    .is_err());
    let mut mutated = rows.clone();
    mutated[0].json = mutated[0].json.replacen(
        "\"rss_peak_bytes\":20000000",
        "\"rss_peak_bytes\":40000000",
        1,
    );
    assert!(summary_json(
        &campaign,
        &mutated,
        &source,
        &master,
        complete,
        &"8".repeat(64)
    )
    .is_err());
    let mut bad_r34_scrub = rows.clone();
    let r34_scrub = bad_r34_scrub
        .iter_mut()
        .find(|row| row.row_id == "C08-001")
        .unwrap();
    r34_scrub.json = r34_scrub.json.replacen(
        "\"name\":\"verified_open\",\"transactions_started\":0",
        "\"name\":\"verified_open\",\"retained_union_scrubs\":0,\"transactions_started\":0",
        1,
    );
    assert!(summary_json(
        &campaign,
        &bad_r34_scrub,
        &source,
        &master,
        complete,
        &"8".repeat(64)
    )
    .is_err());
    let transition = rows.iter().position(|row| row.row_id == "C03-001").unwrap();
    let mut bad_ref = rows.clone();
    bad_ref[transition].json = bad_ref[transition].json.replacen(
        "\"pre_ref\":{\"name\":\"main\",\"generation\":1",
        "\"pre_ref\":{\"name\":\"main\",\"generation\":2",
        1,
    );
    assert!(validate_ref_chain(&bad_ref, &schedule).is_err());
    let mut bad_authentication = rows.clone();
    let counters = json_object(&bad_authentication[transition].json, "counters")
        .unwrap()
        .to_owned();
    let mutated = counters.replacen(
        "\"fetched_row_authentication_passes\":0",
        "\"fetched_row_authentication_passes\":1",
        1,
    );
    bad_authentication[transition].json = bad_authentication[transition]
        .json
        .replacen(&counters, &mutated, 1);
    assert!(validate_authentication(&bad_authentication).is_err());
    let mut bad_insert_equation = rows.clone();
    let counters = json_object(&bad_insert_equation[transition].json, "counters")
        .unwrap()
        .to_owned();
    let mutated = counters.replacen(
        "\"put_insert_statements\":0",
        "\"put_insert_statements\":1",
        1,
    );
    bad_insert_equation[transition].json = bad_insert_equation[transition]
        .json
        .replacen(&counters, &mutated, 1);
    assert!(validate_authentication(&bad_insert_equation).is_err());
    let mut bad_phase_partition = rows.clone();
    bad_phase_partition[transition].json = bad_phase_partition[transition].json.replacen(
        "\"name\":\"checkpoint\",\"transactions_started\":1",
        "\"name\":\"checkpoint\",\"transactions_started\":2",
        1,
    );
    assert!(validate_phase_counter_rows(&bad_phase_partition).is_err());
    let history_row = rows.iter().position(|row| row.row_id == "C04-001").unwrap();
    let mut bad_retained_root_equation = rows.clone();
    let verified_phase = json_array_objects(
        &bad_retained_root_equation[history_row].json,
        "phase_counters",
    )
    .unwrap()[0]
        .to_owned();
    let mutated_phase = verified_phase.replacen(
        "\"retained_roots_validated\":0",
        "\"retained_roots_validated\":1",
        1,
    );
    bad_retained_root_equation[history_row].json = bad_retained_root_equation[history_row]
        .json
        .replacen(&verified_phase, &mutated_phase, 1);
    let counters = json_object(&bad_retained_root_equation[history_row].json, "counters")
        .unwrap()
        .to_owned();
    let mutated_counters = counters.replacen(
        "\"retained_roots_validated\":0",
        "\"retained_roots_validated\":1",
        1,
    );
    bad_retained_root_equation[history_row].json = bad_retained_root_equation[history_row]
        .json
        .replacen(&counters, &mutated_counters, 1);
    assert!(validate_phase_counter_rows(&bad_retained_root_equation).is_err());
    let c02 = rows.iter().position(|row| row.row_id == "C02-001").unwrap();
    let mut bad_operation_scratch = rows.clone();
    bad_operation_scratch[c02].json = bad_operation_scratch[c02].json.replacen(
        "\"operation_scratch_tables\":3",
        "\"operation_scratch_tables\":2",
        1,
    );
    assert!(validate_phase_counter_rows(&bad_operation_scratch).is_err());
    let mut bad_availability = rows.clone();
    bad_availability[0].json = bad_availability[0].json.replacen(
            "{\"field\":\"counters.transactions_started\",\"availability\":\"NotApplicable\",\"reason\":\"row has no product operation\"},",
            "",
            1,
        );
    assert!(validate_availability_rows(&bad_availability).is_err());
    let mut bad_tree_availability = rows.clone();
    let record = json_array_objects(&bad_tree_availability[0].json, "unavailable")
        .unwrap()
        .into_iter()
        .find(|record| json_string(record, "field").as_deref() == Ok("tree_level_before"))
        .unwrap()
        .to_owned();
    bad_tree_availability[0].json = bad_tree_availability[0].json.replacen(&record, "{}", 1);
    assert!(validate_availability_rows(&bad_tree_availability).is_err());
    let mut bad_rss_availability = rows.clone();
    bad_rss_availability[0].json = bad_rss_availability[0].json.replacen(
        "\"rss_current_bytes\":20000000",
        "\"rss_current_bytes\":null",
        1,
    );
    assert!(validate_availability_rows(&bad_rss_availability).is_err());
    let mut bad_locality = rows.clone();
    bad_locality[transition].json = bad_locality[transition].json.replacen(
        "\"rope_nodes_read\":1",
        "\"rope_nodes_read\":33",
        1,
    );
    assert!(validate_locality_rows(&bad_locality).is_err());
    let mut bad_payload_read = rows.clone();
    let counters = json_object(&bad_payload_read[transition].json, "counters")
        .unwrap()
        .to_owned();
    let mutated = counters.replacen(
        "\"unaffected_payload_reads\":0",
        "\"unaffected_payload_reads\":1",
        1,
    );
    bad_payload_read[transition].json = bad_payload_read[transition]
        .json
        .replacen(&counters, &mutated, 1);
    assert!(validate_locality_rows(&bad_payload_read).is_err());
    let mut bad_payload_write = rows.clone();
    let counters = json_object(&bad_payload_write[transition].json, "counters")
        .unwrap()
        .to_owned();
    let written = json_u128(&counters, "payload_bytes_written").unwrap();
    let mutated = counters.replacen(
        &format!("\"payload_bytes_written\":{written}"),
        &format!("\"payload_bytes_written\":{}", written + 1),
        1,
    );
    bad_payload_write[transition].json = bad_payload_write[transition]
        .json
        .replacen(&counters, &mutated, 1);
    assert!(validate_locality_rows(&bad_payload_write).is_err());
    let burst = rows.iter().position(|row| row.row_id == "C07-001").unwrap();
    let mut bad_burst_native_aggregate = rows.clone();
    let native = json_object(&bad_burst_native_aggregate[burst].json, "native")
        .unwrap()
        .to_owned();
    let bytes_read = json_u128(&native, "bytes_read").unwrap();
    let mutated = native.replacen(
        &format!("\"bytes_read\":{bytes_read}"),
        &format!("\"bytes_read\":{}", bytes_read + 1),
        1,
    );
    bad_burst_native_aggregate[burst].json = bad_burst_native_aggregate[burst]
        .json
        .replacen(&native, &mutated, 1);
    assert!(validate_locality_rows(&bad_burst_native_aggregate).is_err());
    let logical_insert = rows.iter().position(|row| row.row_id == "C05-002").unwrap();
    let mut bad_refresh_route = rows.clone();
    bad_refresh_route[logical_insert].native_route = "FullFallback".to_owned();
    bad_refresh_route[logical_insert].json = bad_refresh_route[logical_insert].json.replacen(
        "\"native_route\":\"CloneShift\"",
        "\"native_route\":\"FullFallback\"",
        1,
    );
    assert!(validate_refresh_rows(&bad_refresh_route).is_err());
    let history = rows.iter().position(|row| row.row_id == "C04-001").unwrap();
    let mut bad_history = rows.clone();
    bad_history[history].json =
        bad_history[history]
            .json
            .replacen("\"head\":\"R5\"", "\"head\":\"R6\"", 1);
    assert!(validate_history_rows(&bad_history).is_err());
    let mut bad_history_digest = rows.clone();
    let digest = json_string(
        json_object(&bad_history_digest[history].json, "oracle").unwrap(),
        "content_digest",
    )
    .unwrap();
    bad_history_digest[history].json = bad_history_digest[history].json.replacen(
        &format!("\"content_digest\":\"{digest}\""),
        &format!("\"content_digest\":\"{}\"", "f".repeat(64)),
        1,
    );
    assert!(validate_history_rows(&bad_history_digest).is_err());
    let mut bad_history_probe = rows.clone();
    bad_history_probe[history].json = bad_history_probe[history].json.replacen(
        "\"root\":\"R0\",\"ordinal\":1",
        "\"root\":\"R0\",\"ordinal\":2",
        1,
    );
    assert!(validate_history_rows(&bad_history_probe).is_err());
    let milestone = rows.iter().position(|row| row.row_id == "C08-003").unwrap();
    let mut bad_terminal_length = rows.clone();
    let oracle = json_object(&bad_terminal_length[milestone].json, "oracle")
        .unwrap()
        .to_owned();
    let mutated_oracle = oracle.replacen(
        &format!("\"logical_length\":{INITIAL_BYTES}"),
        &format!("\"logical_length\":{}", INITIAL_BYTES + 1),
        1,
    );
    bad_terminal_length[milestone].json =
        bad_terminal_length[milestone]
            .json
            .replacen(&oracle, &mutated_oracle, 1);
    assert!(summary_json(
        &campaign,
        &bad_terminal_length,
        &source,
        &master,
        complete,
        &"8".repeat(64)
    )
    .is_err());
    let mut bad_milestone = rows.clone();
    bad_milestone[milestone].json = bad_milestone[milestone].json.replacen(
        "\"metadata_exact\":true",
        "\"metadata_exact\":false",
        1,
    );
    assert!(validate_history_rows(&bad_milestone).is_err());
    let mut bad_cleanup = rows.clone();
    bad_cleanup[milestone].json = bad_cleanup[milestone].json.replacen(
        "\"cleanup_residue_entries\":0",
        "\"cleanup_residue_entries\":1",
        1,
    );
    assert!(validate_history_rows(&bad_cleanup).is_err());
    let mut bad_live_inventory = rows.clone();
    bad_live_inventory[milestone].json = bad_live_inventory[milestone].json.replacen(
        "\"live_extra_user_files\":0",
        "\"live_extra_user_files\":1",
        1,
    );
    assert!(validate_history_rows(&bad_live_inventory).is_err());
    let mut bad_mtime = rows.clone();
    let expected_mtime = FIXTURE_MTIME_SECONDS + 35;
    bad_mtime[milestone].json = bad_mtime[milestone].json.replacen(
        &format!("\"fresh_metadata\":{{\"mode\":420,\"mtime_seconds\":{expected_mtime}"),
        &format!(
            "\"fresh_metadata\":{{\"mode\":420,\"mtime_seconds\":{}",
            expected_mtime + 1
        ),
        1,
    );
    assert!(validate_history_rows(&bad_mtime).is_err());
    let mut retained_live_residue = rows.clone();
    let first_milestone = retained_live_residue
        .iter()
        .position(|row| row.row_id == "C08-001")
        .unwrap();
    retained_live_residue[first_milestone].json = retained_live_residue[first_milestone]
        .json
        .replacen("\"residue_entries\":0", "\"residue_entries\":7", 1);
    let milestone_markdown = summary_markdown(
        &campaign,
        &retained_live_residue,
        &source,
        &master,
        complete,
    )
    .unwrap();
    assert!(milestone_markdown.lines().any(|line| {
        line.starts_with("| R15 | Physical-chain milestone")
            && line.ends_with("| `PASS` | `PASS` | `PASS` |")
    }));
    let mut revise = rows.clone();
    revise[0].status = "REVISE".to_owned();
    let revise_summary = summary_json(
        &campaign,
        &revise,
        &source,
        &master,
        complete,
        &"8".repeat(64),
    )
    .unwrap();
    assert_eq!(json_string(&revise_summary, "status").unwrap(), "REVISE");
    assert!(
        summary_markdown(&campaign, &revise, &source, &master, complete)
            .unwrap()
            .contains("Disposition: `REVISE`")
    );
    assert!(campaign_time(&campaign, complete, Disposition::Revise).contains("status=REVISE\n"));
    let burst = rows.iter().find(|row| row.row_id == "C07-001").unwrap();
    assert_eq!(row_u128(burst, "rope_nodes_read").unwrap(), 8);
    assert_eq!(
        json_all_u128(&burst.json, "rope_nodes_read").unwrap().len(),
        9
    );
    fs::remove_dir_all(run).unwrap();
}
