use super::super::engine_counters::EngineDelta;
use super::super::row_physical::history_root_indices;
use super::super::schedule_model::ScheduledRow;
pub(super) fn synthetic_engine(
    scheduled: &ScheduledRow,
    transition: Option<u8>,
    operation_present: bool,
) -> Option<EngineDelta> {
    operation_present.then(|| {
        if let Some(session) = scheduled.history_session {
            let roots = history_root_indices(session).unwrap().len() as u64;
            let probes = roots * 3;
            let fetched = probes + roots * 4;
            EngineDelta {
                statements: fetched,
                objects_validated: fetched,
                fetched_rows: fetched,
                fetched_row_authentication_passes: fetched,
                fetched_row_role_decode_passes: fetched,
                payload_batch_queries: probes,
                payload_batch_references: probes,
                payload_batch_maximum: 1,
                retained_union_scrubs: 1,
                scratch_tables: 2,
                scratch_statements: 2,
                scratch_rows: 2,
                scratch_high_water_bytes: 4_096,
                ..EngineDelta::default()
            }
        } else if scheduled.row_id == "C08-001" {
            EngineDelta {
                retained_union_scrubs: 1,
                scratch_tables: 2,
                scratch_statements: 2,
                scratch_rows: 2,
                scratch_high_water_bytes: 4_096,
                ..EngineDelta::default()
            }
        } else if transition.is_some() {
            EngineDelta {
                transactions_started: 1,
                transactions_committed: 1,
                publication_transactions_started: 1,
                publication_commits: 1,
                ..EngineDelta::default()
            }
        } else {
            EngineDelta::default()
        }
    })
}
