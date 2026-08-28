use super::campaign::{attribution_models_json, three_stats, validate_row_wall_json};
use super::contract::{attribution_schedule_json, AttributionArm, ATTRIBUTION_SCHEDULE};
use super::projection::{projection_facts_exact, successful_projection_facts_exact};
use crate::legacy_full::{ProjectionFacts, ProjectionSyncFacts};

#[test]
fn attribution_row_wall_requires_product_oracle_cleanup_and_residual() {
    let valid = concat!(
        "{\"schema\":\"layerfs-stage1m-attribution-row-v2\",",
        "\"product_operation_wall_ns\":100,\"oracle_wall_ns\":20,",
        "\"cleanup_wall_ns\":30,\"row_wall_residual_ns\":10,",
        "\"row_wall_ns\":160}"
    );
    assert!(validate_row_wall_json(valid).is_ok());
    assert!(
        validate_row_wall_json(&valid.replace("\"row_wall_ns\":160", "\"row_wall_ns\":159"))
            .is_err()
    );
    assert!(validate_row_wall_json(
        &valid.replace("\"row_wall_residual_ns\":10", "\"row_wall_residual_ns\":-1")
    )
    .is_err());
}

#[test]
fn projection_fact_mutation_rejects_hidden_or_missing_syncs() {
    let mut one_sync = ProjectionSyncFacts::available();
    one_sync.attempts = 1;
    one_sync.successes = 1;
    one_sync.requested.process_crash_reconciled = 1;
    one_sync.achieved.process_crash_reconciled = 1;

    let mut facts = ProjectionFacts::available();
    facts.recovery_marker_file_sync = one_sync;
    facts.content_temp_file_sync = one_sync;
    facts.regular_file_sync = one_sync;
    facts.regular_file_sync.attempts = 2;
    facts.regular_file_sync.successes = 2;
    facts.regular_file_sync.requested.process_crash_reconciled = 2;
    facts.regular_file_sync.achieved.process_crash_reconciled = 2;
    for owner in [
        &mut facts.staging_directory_sync,
        &mut facts.root_parent_directory_sync,
        &mut facts.dirty_tree_directory_sync,
        &mut facts.final_root_directory_sync,
    ] {
        *owner = one_sync;
    }
    facts.directory_sync = one_sync;
    facts.directory_sync.attempts = 4;
    facts.directory_sync.successes = 4;
    facts.directory_sync.requested.process_crash_reconciled = 4;
    facts.directory_sync.achieved.process_crash_reconciled = 4;

    assert!(projection_facts_exact(facts));
    facts.regular_file_sync.attempts -= 1;
    assert!(!projection_facts_exact(facts));
    facts.regular_file_sync.attempts += 1;
    facts
        .content_temp_file_sync
        .requested
        .process_crash_reconciled = 0;
    assert!(!projection_facts_exact(facts));
    facts
        .content_temp_file_sync
        .requested
        .process_crash_reconciled = 1;
    facts
        .content_temp_file_sync
        .achieved
        .process_crash_reconciled = 0;
    assert!(!projection_facts_exact(facts));
    facts
        .content_temp_file_sync
        .achieved
        .process_crash_reconciled = 1;
    facts.content_write.bytes = 1;
    assert!(!projection_facts_exact(facts));
    facts.content_write.bytes = 0;
    facts.content_temp_file_sync.wall.nanoseconds = 1;
    assert!(!projection_facts_exact(facts));
    facts.content_temp_file_sync.wall.nanoseconds = 0;
    facts.cleanup.attempts = 1;
    facts.cleanup.failures = 1;
    facts.cleanup.residue = 1;
    assert!(projection_facts_exact(facts));
    assert!(!successful_projection_facts_exact(facts));
}

#[test]
fn attribution_schedule_is_the_frozen_interleaved_population() {
    let actual = ATTRIBUTION_SCHEDULE
        .iter()
        .map(|(arm, size)| (arm.name(), *size))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            ("complete", 24),
            ("null", 0),
            ("digest", 96),
            ("native", 24),
            ("null", 96),
            ("digest", 24),
            ("native", 0),
            ("complete", 96),
            ("digest", 0),
            ("native", 96),
            ("complete", 0),
            ("null", 24),
        ]
    );
    let schedule = attribution_schedule_json();
    assert!(schedule.contains("\"warmups\":12"));
    assert!(schedule.contains("\"measured\":36"));
    assert!(schedule.contains("\"rows\":48"));
}

#[test]
fn attribution_uses_the_frozen_n3_estimator_and_taxonomy() {
    assert_eq!(three_stats(&[30, 10, 20]).unwrap(), (20, 30));
    assert_eq!(
        AttributionArm::Complete.operation_label(),
        "same_open_warmed_source_fresh_destination"
    );
    assert_eq!(
        AttributionArm::Null.operation_label(),
        "warm_authenticated_null_sink"
    );
    assert_eq!(
        AttributionArm::Digest.operation_label(),
        "warm_authenticated_digest"
    );
    assert_eq!(
        AttributionArm::Native.operation_label(),
        "native_durable_output"
    );
}

#[test]
fn attribution_model_uses_t0_and_the_frozen_24_to_96_slope() {
    let populations = [
        AttributionArm::Complete,
        AttributionArm::Null,
        AttributionArm::Digest,
        AttributionArm::Native,
    ]
    .into_iter()
    .flat_map(|arm| {
        [
            (arm, 0, 10_000_000, 10_000_000),
            (arm, 24, 34_000_000, 34_000_000),
            (arm, 96, 106_000_000, 106_000_000),
        ]
    })
    .collect::<Vec<_>>();
    let models = attribution_models_json(&populations).unwrap();
    assert!(models.contains("\"fixed_cost_ns\":10000000"));
    assert!(models.contains("\"slope_ns_per_mib\":1000000"));
    assert!(models.contains("\"sustained_bandwidth_mib_per_s\":1000"));
    assert!(models.contains("\"residual_24_ns\":0"));
    assert!(models.contains("\"residual_96_ns\":0"));
    assert!(models.contains("\"model_valid\":true"));
}
