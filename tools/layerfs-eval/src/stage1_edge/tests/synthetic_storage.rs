use super::super::schedule_model::{EditSpec, ScheduledRow};
use crate::legacy_full::Diagnostics;
pub(super) fn synthetic_storage(
    scheduled: &ScheduledRow,
    transition: Option<u8>,
    edit: Option<&EditSpec>,
    operation_present: bool,
) -> Option<(Diagnostics, Diagnostics)> {
    let serial = scheduled.row_index as u64;
    let storage = operation_present
        .then_some(())
        .filter(|_| scheduled.row_group != "C09")
        .map(|_| {
            let before = Diagnostics {
                database_bytes: Some(1_000_000 + serial * 4_096),
                logical_engine_bytes: Some(900_000 + serial * 2_048),
                object_bytes_written: serial * 1_024,
                ..Diagnostics::default()
            };
            let delta = u64::from(transition.is_some()) * 4_096;
            let after = Diagnostics {
                database_bytes: before.database_bytes.map(|value| value + delta),
                logical_engine_bytes: before.logical_engine_bytes.map(|value| value + delta / 2),
                object_bytes_written: before.object_bytes_written
                    + transition.map_or(0, |_| {
                        edit.as_ref().map_or(4_096, |edit| edit.insert_bytes.max(1))
                    }),
                ..Diagnostics::default()
            };
            (before, after)
        });
    storage
}
