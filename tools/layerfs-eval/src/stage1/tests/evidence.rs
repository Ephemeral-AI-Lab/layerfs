use super::super::artifact::json_u128;
use super::super::counter_validation::locality_evidence_json;
use super::super::environment::environment_json;
use super::super::model::{
    CampaignData, EngineDelta, Environment, ProcessResources, Readiness, MIB, STORE_CACHE_PAGES,
    STORE_CACHE_SPILL_PAGES, STORE_PAGE_SIZE,
};
use super::super::operation_evidence::{clone_json, counters_json, engine_json};
use super::super::readiness::{readiness_json, schedule_json, validate_store_sqlite_profile};
use super::super::resource_evidence::{summary_json, TerminalResources};
use super::super::single_file_run::incomplete_summary_json;
use super::support::valid_json;
use crate::legacy_full::{Diagnostics, NativeRoute, OperationDiagnostics};
use crate::stage1_fixture::{CloneReceipt, FILE_BYTES};
use std::collections::BTreeMap;

#[test]
fn failure_summary_retains_partial_statistics_targets_and_resources() {
    let mut data = CampaignData {
        reset_count: 1,
        reset_wall_ns: 10,
        operation_wall_ns: 30,
        artifact_wall_ns: 5,
        last_q_terminal_bytes: Some(0),
        ..CampaignData::default()
    };
    data.metrics.insert("A01".to_owned(), vec![30]);
    data.bytes_per_observation
        .insert("A01".to_owned(), FILE_BYTES);
    data.process_resources.push(ProcessResources {
        operation: "campaign-baseline".to_owned(),
        observed: true,
        current_rss_bytes: 50 * MIB,
        process_peak_rss_bytes: 50 * MIB,
    });
    data.process_resources.push(ProcessResources {
        operation: "A01".to_owned(),
        observed: true,
        current_rss_bytes: 60 * MIB,
        process_peak_rss_bytes: 65 * MIB,
    });
    let terminal = TerminalResources {
        observed: true,
        current_rss_bytes: 60 * MIB,
        maximum_rss_bytes: 65 * MIB,
        ..TerminalResources::default()
    };
    let summary = summary_json(
        "FAIL",
        Some("resource gate"),
        &data,
        100,
        "start",
        "final",
        &terminal,
    )
    .unwrap();
    assert!(summary.contains("\"schema\":\"layerfs-stage1-summary-v2\""));
    assert!(summary.contains("\"status\":\"FAIL\""));
    assert!(summary.contains("\"statistics\":{\"A01\":"));
    assert!(summary.contains("\"targets\":{\"A01\":"));
    assert!(summary.contains("\"campaign_equation\":{"));
    assert!(summary.contains("\"first_64_mib_crossing\":{\"sequence\":1,\"operation\":\"A01\""));
}
#[test]
fn sdk_q_and_native_call_observations_are_not_zeroed_by_serialization() {
    let mut observed = OperationDiagnostics {
        operation_q_current_bytes: 4 * MIB,
        operation_q_high_water_bytes: 4 * MIB,
        operation_q_terminal_bytes: 0,
        ..OperationDiagnostics::default()
    };
    observed.native.route = Some(NativeRoute::MaterializeStream);
    observed.native.temp_calls = 1;
    observed.native.sync_calls = 2;
    observed.native.replace_calls = 1;
    let json = counters_json(&observed);
    assert!(json.contains("\"operation_q_current_bytes\":4194304"));
    assert!(json.contains("\"operation_q_high_water_bytes\":4194304"));
    assert!(json.contains("\"temp_calls\":1"));
    assert!(json.contains("\"sync_calls\":2"));
    assert!(json.contains("\"replace_calls\":1"));
    assert!(json.contains("\"rematerializations\":0"));
}
#[cfg(target_os = "macos")]
#[test]
fn artifact_json_is_valid() {
    let environment = Environment {
        git_commit: "a".repeat(40),
        dirty_tree_blake3: "b".repeat(64),
        source_tree_blake3: "c".repeat(64),
        source_file_count: 1,
        source_files: vec!["tools/layerfs-eval/src/stage1.rs".to_owned()],
        cargo_lock_blake3: "d".repeat(64),
        executable_blake3: "e".repeat(64),
        build_profile: "release",
        debug_assertions: false,
        uname: "Darwin".to_owned(),
        macos: "macOS".to_owned(),
        apfs_identity: "APFS".to_owned(),
    };
    let mut diagnostics = Diagnostics {
        page_size: STORE_PAGE_SIZE,
        cache_pages: STORE_CACHE_PAGES,
        cache_spill_pages: STORE_CACHE_SPILL_PAGES,
        ..Diagnostics::default()
    };
    let store_sqlite_profile = validate_store_sqlite_profile(&diagnostics).unwrap();
    diagnostics.cache_pages -= 1;
    assert!(validate_store_sqlite_profile(&diagnostics).is_err());
    let readiness = Readiness {
        environment: environment.clone(),
        master_digest: "master".to_owned(),
        reset_observations_ns: vec![1, 1, 1],
        reset_upper_ns: 1,
        forecast_reset_wall_ns: 54,
        forecast_campaign_wall_ns: 55,
        apfs_identity: "APFS".to_owned(),
        store_database_bytes: BTreeMap::new(),
        store_sqlite_profile,
    };
    let readiness = readiness_json(&readiness);
    valid_json(&readiness);
    assert_eq!(json_u128(&readiness, "page_size").unwrap(), 4_096);
    assert_eq!(json_u128(&readiness, "cache_pages").unwrap(), 1_280);
    assert_eq!(json_u128(&readiness, "cache_spill_pages").unwrap(), 1_280);
    valid_json(&environment_json(&environment));
    valid_json(&schedule_json(true));
    valid_json(&incomplete_summary_json(0));
    valid_json(locality_evidence_json());
    let mut data = CampaignData::default();
    let populations = [
        ("A01", 3),
        ("A02", 300),
        ("A03a", 3),
        ("A03b", 3),
        ("A09", 3),
        ("A10", 3),
        ("A11", 3),
        ("A12", 3),
        ("A13", 11),
        ("A14/edit", 4),
        ("A15", 3),
        ("A17/checkpoint", 100),
        ("A17/edit-plus-checkpoint", 100),
    ];
    for (name, count) in populations {
        data.metrics.insert(name.to_owned(), vec![1; count]);
    }
    for id in ["A04", "A05", "A06", "A07", "A08"] {
        data.metrics.insert(format!("{id}/logical"), vec![1; 3]);
        data.metrics
            .insert(format!("{id}/native-edit-plus-checkpoint"), vec![1; 3]);
    }
    let terminal = TerminalResources::default();
    valid_json(&summary_json("PASS", None, &data, 1_000, "master", "master", &terminal).unwrap());
    valid_json(&format!(
            "{{\"schema\":\"layerfs-stage1-row-v1\",\"operation_counters\":{},\"engine_delta\":{},\"clone\":{}}}",
            counters_json(&OperationDiagnostics::default()),
            engine_json(&EngineDelta::default()),
            clone_json(&CloneReceipt {
                wall_ns: 1,
                clone_wall_ns: 1,
                source_logical_bytes: 1,
                destination_logical_bytes: 1,
                source_allocated_bytes: 1,
                destination_allocated_bytes: 1,
                distinct_regular_inodes: 1,
                clone_id: 1,
            })
        ));
}
