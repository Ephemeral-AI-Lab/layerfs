use super::super::artifact::durable_replace;
use super::super::campaign_write::edit_cases;
use super::super::counter_validation::{
    content_rope, verify_engine_equations, verify_read_only_engine, verify_state_change,
};
use super::super::model::{Campaign, CampaignData, MIB, RESET_COUNT};
use super::super::readiness::append_only_readiness_artifact;
use super::super::resource_evidence::{append_a16, append_failure_a16, TerminalResources};
use super::super::root_validation::edit_result_len;
use super::super::summary_evidence::statistics;
use super::support::valid_json;
use crate::legacy_full::OperationDiagnostics;
use crate::stage1_fixture::{FILE_BYTES, RANDOM_RANGE_BYTES};
use std::fs::{self, OpenOptions};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[test]
fn frozen_statistics_use_one_based_order_rules() {
    let heavy = statistics(&[30, 10, 20]).unwrap();
    assert_eq!((heavy.p50, heavy.p95), (20, 30));
    let reopen = statistics(&(1..=11).collect::<Vec<_>>()).unwrap();
    assert_eq!((reopen.p50, reopen.p95), (6, 11));
    let random = statistics(&(1..=300).collect::<Vec<_>>()).unwrap();
    assert_eq!((random.p50, random.p95), (150, 285));
}

#[test]
fn fetched_row_equation_is_explicit_per_integrity_mode() {
    let mut verified = super::super::model::EngineDelta {
        integrity_mode: crate::legacy_full::IntegrityMode::Verified,
        fetched_rows: 1,
        fetched_row_authentication_passes: 1,
        fetched_row_role_decode_passes: 1,
        ..Default::default()
    };
    assert!(verify_engine_equations(&verified).is_ok());
    verified.fetched_row_authentication_passes = 0;
    assert!(verify_engine_equations(&verified).is_err());

    let mut trusted = super::super::model::EngineDelta {
        integrity_mode: crate::legacy_full::IntegrityMode::TrustedLocalDev,
        fetched_rows: 1,
        fetched_row_authentication_passes: 0,
        fetched_row_role_decode_passes: 1,
        ..Default::default()
    };
    assert!(verify_engine_equations(&trusted).is_ok());
    assert!(verify_read_only_engine(&trusted).is_ok());
    trusted.fetched_row_authentication_passes = 1;
    assert!(verify_engine_equations(&trusted).is_ok());
    assert!(verify_read_only_engine(&trusted).is_err());
    trusted.transactions_started = 1;
    trusted.transactions_committed = 1;
    trusted.publication_commits = 1;
    assert!(verify_state_change(&trusted, 1).is_ok());
    trusted.publication_closure_passes = 1;
    assert!(verify_state_change(&trusted, 1).is_err());
}
#[test]
fn frozen_reset_and_edit_populations_are_exact() {
    assert_eq!(3 + 3 + 3 + 3 + 30 + 3 + 3 + 1 + 1 + 3 + 1, RESET_COUNT);
    let cases = edit_cases();
    assert_eq!(cases.len(), 5);
    assert_eq!(edit_result_len(&cases[1]).unwrap(), FILE_BYTES);
    assert_eq!(edit_result_len(&cases[3]).unwrap(), FILE_BYTES);
    assert!(cases
        .iter()
        .all(|case| edit_result_len(case).unwrap() <= FILE_BYTES));
}
#[test]
fn content_locality_excludes_observed_metadata_ropes() {
    let mut counters = OperationDiagnostics::default();
    counters.rope.cdc_bytes_scanned = 4_112;
    counters.rope.payload_bytes_written = 4_112;
    counters.metadata_rope.cdc_bytes_scanned = 16;
    counters.metadata_rope.payload_bytes_written = 16;
    assert_eq!(content_rope(&counters).unwrap(), (4_096, 0, 4_096));
}
#[test]
fn random_ranges_are_globally_non_overlapping() {
    let blocks = FILE_BYTES / RANDOM_RANGE_BYTES;
    let offsets = (0..300_u64)
        .map(|index| ((index * 521 + 0x51) % blocks) * RANDOM_RANGE_BYTES)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(offsets.len(), 300);
}
#[test]
fn readiness_artifacts_are_append_only() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-readiness-artifacts-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let first = append_only_readiness_artifact(&root, "failure", "first\n").unwrap();
    let second = append_only_readiness_artifact(&root, "failure", "second\n").unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read_to_string(first).unwrap(), "first\n");
    assert_eq!(fs::read_to_string(second).unwrap(), "second\n");
    let canonical = root.join("summary.json");
    fs::write(&canonical, "fail\n").unwrap();
    durable_replace(&canonical, "pass\n").unwrap();
    assert_eq!(fs::read_to_string(canonical).unwrap(), "pass\n");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 3);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn failing_a16_is_appended_before_the_gate_error() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-a16-artifact-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let rows_path = root.join("rows.jsonl");
    fs::write(&rows_path, "").unwrap();
    let rows = OpenOptions::new().append(true).open(&rows_path).unwrap();
    let mut data = CampaignData {
        last_q_terminal_bytes: Some(0),
        ..CampaignData::default()
    };
    let mut campaign = Campaign {
        run: &root,
        started: Instant::now(),
        rows,
        data: &mut data,
    };
    let terminal = TerminalResources {
        observed: true,
        fd_baseline: 5,
        fd_terminal: 5,
        current_rss_bytes: 65 * MIB,
        maximum_rss_bytes: 65 * MIB,
        ..TerminalResources::default()
    };
    let error = append_a16(&mut campaign, &terminal, true).unwrap().unwrap();
    campaign.rows.sync_all().unwrap();
    drop(campaign);
    let row = fs::read_to_string(&rows_path).unwrap();
    let lines = row.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    #[cfg(target_os = "macos")]
    valid_json(lines[0]);
    assert!(error.contains("peak RSS"));
    assert!(row.contains("\"id\":\"A16\""));
    assert!(row.contains("\"gate_status\":\"FAIL\""));
    assert!(row.contains("\"operation_q_bytes\":0"));
    assert!(row.contains("\"process_peak_rss_bytes\":68157440"));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn earlier_failure_appends_an_unavailable_a16_row() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-a16-early-failure-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("rows.jsonl"), "").unwrap();
    let mut data = CampaignData::default();
    let terminal = append_failure_a16(&root, Instant::now(), &mut data, None).unwrap();
    assert!(!terminal.observed);
    let row = fs::read_to_string(root.join("rows.jsonl")).unwrap();
    let lines = row.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    #[cfg(target_os = "macos")]
    valid_json(lines[0]);
    assert!(row.contains("\"id\":\"A16\""));
    assert!(row.contains("\"gate_status\":\"FAIL\""));
    assert!(row.contains("\"observed\":false"));
    assert!(row.contains("\"fd_baseline\":\"Unavailable\""));
    fs::remove_dir_all(root).unwrap();
}
