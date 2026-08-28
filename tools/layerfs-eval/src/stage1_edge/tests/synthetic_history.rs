use super::super::engine_counters::EngineDelta;
use super::super::receipt_model::HistoryProbeReceipt;
use super::super::row_physical::history_root_indices;
use super::super::schedule::oracle_snapshots;
use super::super::schedule_model::{FrozenSchedule, ScheduledRow};
pub(super) fn synthetic_history(
    schedule: &FrozenSchedule,
    scheduled: &ScheduledRow,
) -> Vec<HistoryProbeReceipt> {
    let snapshots = oracle_snapshots(schedule).unwrap();
    let history_probes = scheduled.history_session.map_or_else(Vec::new, |session| {
        history_root_indices(session)
            .unwrap()
            .iter()
            .flat_map(|root_index| {
                let logical_length = snapshots[*root_index].logical_length;
                (1..=3).map(move |ordinal| {
                    let first = ordinal == 1;
                    let fetched = 1 + u64::from(first) * 4;
                    let mut operation = crate::legacy_full::OperationDiagnostics::default();
                    operation.namespace.nodes_read = u64::from(first);
                    operation.inode_table.nodes_read = u64::from(first);
                    operation.rope.nodes_read = 1 + u64::from(first) * 2;
                    operation.rope.payload_bytes_read = 65_536;
                    let start = match ordinal {
                        1 => 0,
                        2 => logical_length / 2 - 32_768,
                        3 => logical_length - 65_536,
                        _ => unreachable!(),
                    };
                    HistoryProbeReceipt {
                        root_index: *root_index,
                        ordinal,
                        start,
                        length: 65_536,
                        wall_ns: 1,
                        engine: EngineDelta {
                            statements: fetched,
                            objects_validated: fetched,
                            fetched_rows: fetched,
                            fetched_row_authentication_passes: fetched,
                            fetched_row_role_decode_passes: fetched,
                            payload_batch_queries: 1,
                            payload_batch_references: 1,
                            payload_batch_maximum: 1,
                            ..EngineDelta::default()
                        },
                        operation,
                    }
                })
            })
            .collect()
    });
    history_probes
}
