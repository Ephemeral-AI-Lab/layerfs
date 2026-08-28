use super::super::engine_counters::{EngineDelta, PhaseCounterDelta};
use super::super::schedule_model::ScheduledRow;
pub(super) fn synthetic_phase_counters(
    scheduled: &ScheduledRow,
    engine: Option<EngineDelta>,
    operation: Option<&crate::legacy_full::OperationDiagnostics>,
    active_store_connections: u64,
) -> Vec<PhaseCounterDelta> {
    let phase_names: &[&str] = match scheduled.row_group {
        "C02" => &[
            "store_open",
            "storage_observation",
            "materialization",
            "storage_observation",
        ],
        "C03" | "C07" => &[
            "native_edit",
            "checkpoint",
            "canonical_witness",
            "storage_observation",
        ],
        "C04" | "C06" => &[
            "verified_open",
            "storage_observation",
            "history_read",
            "storage_observation",
        ],
        "C05" => &[
            "logical_edit",
            "apfs_refresh",
            "canonical_witness",
            "storage_observation",
        ],
        "C08" => &[
            "verified_open",
            "storage_observation",
            "materialization",
            "storage_observation",
        ],
        "C09" => &["explicit_cleanup"],
        _ => &[],
    };
    let phase_counters = engine.map_or_else(Vec::new, |engine| {
        phase_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let phase_engine = if scheduled.history_session.is_some() {
                    if *name == "verified_open" {
                        EngineDelta {
                            retained_union_scrubs: 1,
                            scratch_tables: 2,
                            scratch_statements: 2,
                            scratch_rows: 2,
                            scratch_high_water_bytes: 4_096,
                            ..EngineDelta::default()
                        }
                    } else if *name == "history_read" {
                        EngineDelta {
                            retained_union_scrubs: 0,
                            scratch_tables: 0,
                            scratch_statements: 0,
                            scratch_rows: 0,
                            scratch_high_water_bytes: 0,
                            ..engine
                        }
                    } else {
                        EngineDelta::default()
                    }
                } else if index == usize::from(matches!(scheduled.row_group, "C03" | "C07")) {
                    engine
                } else {
                    EngineDelta::default()
                };
                let operation_scratch_owner = matches!(
                    (scheduled.row_group, *name),
                    ("C02" | "C08", "materialization")
                        | ("C03" | "C07", "native_edit")
                        | ("C05", "apfs_refresh")
                        | ("C09", "explicit_cleanup")
                );
                let operation_scratch = operation.as_ref().filter(|_| operation_scratch_owner);
                PhaseCounterDelta {
                    name,
                    engine: phase_engine,
                    q_before_bytes: 0,
                    q_after_bytes: 0,
                    q_high_water_bytes: crate::legacy_full::OPERATION_Q_BOUND_BYTES,
                    active_connections: active_store_connections,
                    operation_scratch_tables: operation_scratch
                        .map_or(0, |operation| operation.scratch_tables),
                    operation_scratch_statements: operation_scratch
                        .map_or(0, |operation| operation.scratch_statements),
                    operation_scratch_rows: operation_scratch
                        .map_or(0, |operation| operation.scratch_rows),
                    operation_scratch_high_water_bytes: operation_scratch
                        .map_or(0, |operation| operation.scratch_high_water_bytes),
                }
            })
            .collect()
    });
    phase_counters
}
