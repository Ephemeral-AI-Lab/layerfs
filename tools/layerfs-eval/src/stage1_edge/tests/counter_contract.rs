use super::super::engine_counters::{EngineDelta, PhaseCounterDelta};
use super::super::operation_json::{counters_json, joined_scratch_counts};
use super::super::receipt_json::phase_counter_json;

#[test]
fn operation_only_phase_preserves_peak_and_actual_connections() {
    let operation = crate::legacy_full::OperationDiagnostics {
        scratch_statements: 6,
        scratch_high_water_bytes: 33_304,
        ..Default::default()
    };
    let native = PhaseCounterDelta::operation_only("native_edit", &operation, 1);
    let cleanup = PhaseCounterDelta::operation_only("explicit_cleanup", &operation, 0);
    assert_eq!(native.active_connections, 1);
    assert_eq!(cleanup.active_connections, 0);
    assert_eq!(native.operation_scratch_statements, 6);
    assert_eq!(native.operation_scratch_high_water_bytes, 33_304);
    let json = phase_counter_json(&native);
    assert!(json.contains("\"active_connections\":1"));
    assert!(json.contains("\"operation_scratch_high_water_bytes\":33304"));
}
#[test]
fn row_join_adds_disjoint_engine_and_vfs_scratch_but_maxes_peak() {
    let engine = EngineDelta {
        scratch_tables: 2,
        scratch_statements: 20_242,
        scratch_rows: 62_540,
        scratch_high_water_bytes: 90_000,
        ..EngineDelta::default()
    };
    let operation = crate::legacy_full::OperationDiagnostics {
        scratch_tables: 1,
        scratch_statements: 21,
        scratch_rows: 4,
        scratch_high_water_bytes: 33_304,
        ..Default::default()
    };
    assert_eq!(
        joined_scratch_counts(engine, operation).unwrap(),
        (3, 20_263, 62_544, 90_000)
    );
    let json = counters_json(Some(engine), Some(&operation)).unwrap();
    assert!(json.contains("\"scratch_tables\":3"));
    assert!(json.contains("\"scratch_statements\":20263"));
    assert!(json.contains("\"scratch_rows\":62544"));
    assert!(json.contains("\"scratch_high_water_bytes\":90000"));
}
