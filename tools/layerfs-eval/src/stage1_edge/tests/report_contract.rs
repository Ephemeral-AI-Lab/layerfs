use super::super::artifact::{durable_write, json_string, json_u128, unix_ns};
use super::super::campaign::{append_failed_row, enforce_campaign_limit};
use super::super::context::{begin_failure_context, set_failure_phase, Disposition};
use super::super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::super::failure_artifacts::write_failure_artifacts;
use super::super::limits::INITIAL_BYTES;
use super::super::report_disposition::{
    derive_disposition, validate_campaign_time, validate_timer_equation,
};
use super::super::row_parse::ParsedRow;
use super::super::schedule::frozen_schedule;
use super::super::statistics::statistics;
use super::super::summary_markdown_contract::{validate_summary_headings, SUMMARY_HEADINGS};
use super::synthetic::synthetic_pass_row;
use crate::legacy_full::Diagnostics;
use std::fs::{self, File};
use std::time::{Duration, Instant};

#[test]
fn nearest_rank_statistics_retain_raw_and_sorted_arrays() {
    for (n, p50, p95) in [
        (3, 2, 3),
        (4, 2, 4),
        (5, 3, 5),
        (6, 3, 6),
        (12, 6, 12),
        (15, 8, 15),
        (19, 10, 19),
        (51, 26, 49),
    ] {
        let raw = (1..=n).rev().map(|value| value as u128).collect::<Vec<_>>();
        let stats = statistics(raw.clone()).unwrap();
        assert_eq!(stats.raw_ns, raw);
        assert_eq!(
            stats.sorted_ns,
            (1..=n).map(|value| value as u128).collect::<Vec<_>>()
        );
        assert_eq!(stats.p50_ns, p50);
        assert_eq!(stats.p95_ns, p95);
    }
}
#[test]
fn report_heading_and_campaign_timer_contracts_are_exact() {
    let markdown = SUMMARY_HEADINGS.join("\n\n");
    validate_summary_headings(&markdown).unwrap();
    let timer = concat!(
        "schema=layerfs-stage1.1-campaign-time-v1\n",
        "status=PASS\n",
        "started_unix_ns=1\n",
        "completed_unix_ns=11\n",
        "complete_wall_ns=10\n",
        "row_wall_sum_ns=6\n",
        "outside_rows_wall_ns=4\n",
        "timer_residual_ns=0\n",
        "hard_limit_ns=60000000000\n",
        "rows_expected=47\n",
        "rows_valid=47\n",
        "edit_suboperations_expected=51\n",
        "edit_suboperations_observed=51\n",
        "transitions_expected=34\n",
        "transitions_observed=34\n",
    );
    validate_campaign_time(timer).unwrap();
    assert!(validate_campaign_time(
        &timer.replace("outside_rows_wall_ns=4", "outside_rows_wall_ns=5")
    )
    .is_err());
}
#[test]
fn hard_gate_failures_cannot_be_promoted() {
    let row = |status: &str| ParsedRow {
        json: String::new(),
        row_id: "test".to_owned(),
        row_group: "C00".to_owned(),
        operation: "admission".to_owned(),
        size_band: "not-applicable".to_owned(),
        native_route: "NotApplicable".to_owned(),
        status: status.to_owned(),
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        row_wall_ns: 0,
        row_residual_ns: 0,
    };
    assert_eq!(
        derive_disposition(&[row("FAIL"), row("REVISE")]),
        Disposition::Fail
    );
    assert_eq!(derive_disposition(&[row("REVISE")]), Disposition::Revise);
    assert_eq!(derive_disposition(&[row("PASS")]), Disposition::Pass);
}
#[test]
fn failed_rows_and_failure_reports_are_schema_valid_and_append_only() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-failure-contract-{}-{}",
        std::process::id(),
        unix_ns().unwrap()
    ));
    fs::create_dir(&run).unwrap();
    File::create(run.join("rows.jsonl")).unwrap();
    durable_write(&run.join("stderr.txt"), "first equation\n").unwrap();
    begin_failure_context("C00-001", "admission");
    set_failure_phase("fixture_custody");
    append_failed_row(&run, "first equation", &run.join("stderr.txt")).unwrap();
    let rows = fs::read_to_string(run.join("rows.jsonl")).unwrap();
    let complete_wall_ns = json_u128(&rows, "row_wall_ns").unwrap() + 10;
    write_failure_artifacts(&run, "first equation", 1, complete_wall_ns).unwrap();
    assert_eq!(rows.lines().count(), 1);
    assert_eq!(json_string(&rows, "status").unwrap(), "FAIL");
    assert_eq!(
        json_string(&rows, "first_failed_equation").unwrap(),
        "first equation"
    );
    assert_eq!(json_string(&rows, "phase").unwrap(), "fixture_custody");
    let summary = fs::read_to_string(run.join("summary.json")).unwrap();
    assert_eq!(json_string(&summary, "status").unwrap(), "FAIL");
    let timer = fs::read_to_string(run.join("campaign-time.txt")).unwrap();
    validate_timer_equation(&timer).unwrap();
    assert!(validate_timer_equation(&timer.replacen(
        "outside_rows_wall_ns=10",
        "outside_rows_wall_ns=11",
        1
    ))
    .is_err());
    let markdown = fs::read_to_string(run.join("summary.md")).unwrap();
    validate_summary_headings(&markdown).unwrap();
    fs::remove_dir_all(run).unwrap();
}
#[test]
fn between_rows_budget_failure_does_not_fabricate_the_next_row() {
    let run = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-between-rows-{}-{}",
        std::process::id(),
        unix_ns().unwrap()
    ));
    fs::create_dir(&run).unwrap();
    let schedule = frozen_schedule().unwrap();
    fs::write(
        run.join("rows.jsonl"),
        synthetic_pass_row(&schedule, &schedule.rows[0])
            .json()
            .unwrap(),
    )
    .unwrap();
    durable_write(&run.join("stderr.txt"), "time budget\n").unwrap();
    begin_failure_context("__between_rows__", "time_budget");
    append_failed_row(&run, "time budget", &run.join("stderr.txt")).unwrap();
    assert_eq!(
        fs::read_to_string(run.join("rows.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(enforce_campaign_limit(Instant::now()).is_ok());
    assert!(enforce_campaign_limit(Instant::now() - Duration::from_secs(61)).is_err());
    fs::remove_dir_all(run).unwrap();
}
#[test]
fn storage_observation_is_a_disjoint_phase_counter_owner() {
    let before = Diagnostics::default();
    let product = Diagnostics {
        statements: 5,
        primary_read_statements: 5,
        ..Diagnostics::default()
    };
    let after = Diagnostics {
        statements: 8,
        primary_read_statements: 8,
        ..Diagnostics::default()
    };
    let product_phase = PhaseCounterDelta::between("product", &before, &product).unwrap();
    let storage_phase =
        PhaseCounterDelta::between("storage_observation", &product, &after).unwrap();
    assert_eq!(storage_phase.engine.statements, 3);
    assert!(verify_phase_partition(
        &[product_phase, storage_phase],
        EngineDelta::between(&before, &after).unwrap()
    )
    .is_ok());
    assert!(verify_phase_partition(
        &[product_phase],
        EngineDelta::between(&before, &after).unwrap()
    )
    .is_err());
}
