use super::super::receipt_model::Phase;
use super::super::schedule_model::ScheduledRow;
use super::synthetic::synthetic_phase;
pub(super) fn synthetic_phases(scheduled: &ScheduledRow) -> Vec<Phase> {
    match scheduled.row_group {
        "C00" => vec![synthetic_phase("admission")],
        "C01" => vec![synthetic_phase("reset")],
        "C02" => vec![
            synthetic_phase("store_open"),
            synthetic_phase("cold_materialization"),
            synthetic_phase("live_physical_oracle"),
        ],
        "C03" => vec![
            synthetic_phase("native_edit"),
            synthetic_phase("live_physical_oracle"),
            synthetic_phase("durable_checkpoint"),
            synthetic_phase("canonical_witness"),
            synthetic_phase("counter_snapshot"),
        ],
        "C04" | "C06" => vec![
            synthetic_phase("verified_open"),
            Phase {
                name: "history_read",
                wall_ns: 20,
            },
        ],
        "C05" => vec![
            synthetic_phase("direct_logical_edit"),
            synthetic_phase("changed_root_refresh"),
            synthetic_phase("live_physical_oracle"),
            synthetic_phase("canonical_witness"),
            synthetic_phase("counter_snapshot"),
        ],
        "C07" => vec![
            synthetic_phase("native_edit"),
            synthetic_phase("live_physical_oracle"),
            synthetic_phase("durable_checkpoint"),
            synthetic_phase("canonical_witness"),
            synthetic_phase("counter_snapshot"),
        ],
        "C08" => {
            let mut phases = vec![
                synthetic_phase("verified_open"),
                synthetic_phase("milestone_materialization"),
                synthetic_phase("metadata_oracle"),
                synthetic_phase("explicit_cleanup"),
            ];
            if scheduled.milestone_root == Some(34) {
                phases.insert(0, synthetic_phase("live_physical_oracle"));
            }
            phases
        }
        "C09" => vec![synthetic_phase("explicit_cleanup")],
        _ => unreachable!(),
    }
}
