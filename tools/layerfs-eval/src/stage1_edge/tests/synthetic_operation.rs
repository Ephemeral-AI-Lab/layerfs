use super::super::limits::INITIAL_BYTES;
use super::super::locality::ContentCounters;
use super::super::receipt_model::SubEditReceipt;
use super::super::row_physical::history_root_indices;
use super::super::schedule_model::{EditKind, EditSpec, FrozenSchedule, ScheduledRow};
pub(super) fn synthetic_operation(
    schedule: &FrozenSchedule,
    scheduled: &ScheduledRow,
    edit: Option<&EditSpec>,
) -> (
    Option<crate::legacy_full::OperationDiagnostics>,
    Vec<SubEditReceipt>,
) {
    let mut operation = (!matches!(scheduled.row_group, "C00" | "C01"))
        .then(crate::legacy_full::OperationDiagnostics::default);
    if let Some(value) = operation.as_mut() {
        value.operation_q_current_bytes = crate::legacy_full::OPERATION_Q_BOUND_BYTES;
        value.operation_q_high_water_bytes = crate::legacy_full::OPERATION_Q_BOUND_BYTES;
        if scheduled.row_group == "C02" {
            value.workspace_materializations = 1;
            value.native.bytes_written = INITIAL_BYTES;
            value.scratch_tables = 3;
            value.scratch_statements = 55;
            value.scratch_rows = 6;
            value.scratch_high_water_bytes = 87_088;
        } else if scheduled.row_group == "C08" {
            value.scratch_tables = 1;
            value.scratch_statements = 21;
            value.scratch_rows = 4;
            value.scratch_high_water_bytes = 33_304;
        } else if scheduled.row_group == "C09" {
            value.scratch_statements = 1;
            value.scratch_derived_setup_statements = 1;
        }
        if let Some(edit) = edit {
            value.rope.cdc_bytes_scanned = edit.insert_bytes;
            value.rope.payload_bytes_written = edit.insert_bytes;
            value.rope.nodes_read = 1;
            value.rope.nodes_created = 1;
            value.rope.tree_level_before = Some(1);
            if scheduled.row_group == "C03" {
                value.workspace_reuses = 1;
                value.descriptor_resets = 1;
                value.native.route = Some(if edit.kind == EditKind::Overwrite {
                    crate::legacy_full::NativeRoute::ClonePatch
                } else {
                    crate::legacy_full::NativeRoute::InPlaceShift
                });
                value.scratch_statements = 3;
                value.scratch_rows = 3;
                value.scratch_high_water_bytes = 33_304;
                value.scratch_operation_statements = 3;
            } else if scheduled.row_group == "C05" {
                value.workspace_reuses = 1;
                value.scratch_tables = 1;
                value.scratch_statements = 11;
                value.scratch_rows = 6;
                value.scratch_high_water_bytes = 33_304;
                value.native.route = Some(if edit.kind == EditKind::Overwrite {
                    value.native.bytes_written = edit.insert_bytes;
                    value.native.patch_bytes = edit.insert_bytes;
                    value.native.clone_attempts = 1;
                    value.native.clone_successes = 1;
                    crate::legacy_full::NativeRoute::ClonePatch
                } else {
                    let suffix = edit.before_bytes - edit.offset - edit.delete_bytes;
                    value.native.bytes_read = suffix;
                    value.native.bytes_written = suffix + edit.insert_bytes;
                    value.native.patch_bytes = edit.insert_bytes;
                    value.native.suffix_bytes_shifted = suffix;
                    value.native.clone_attempts = 1;
                    value.native.clone_successes = 1;
                    crate::legacy_full::NativeRoute::CloneShift
                });
            }
        }
        if let Some(session) = scheduled.history_session {
            let roots = history_root_indices(session).unwrap().len() as u64;
            let probes = roots * 3;
            value.namespace.nodes_read = roots;
            value.inode_table.nodes_read = roots;
            value.rope.nodes_read = probes + roots * 2;
            value.rope.payload_bytes_read = probes * 65_536;
        }
    }
    let mut sub_edits = Vec::new();
    if let Some(index) = scheduled.burst_index {
        let burst = &schedule.bursts[index];
        let replacement = burst
            .edits
            .iter()
            .map(|edit| edit.insert_bytes)
            .sum::<u64>();
        let value = operation.as_mut().unwrap();
        value.rope.cdc_bytes_scanned = replacement;
        value.rope.payload_bytes_written = replacement;
        value.rope.nodes_read = burst.edits.len() as u64;
        value.rope.nodes_created = burst.edits.len() as u64;
        value.workspace_reuses = 1;
        value.descriptor_resets = 1;
        value.scratch_statements = burst.edits.len() as u64 * 3;
        value.scratch_rows = burst.edits.len() as u64 * 3;
        value.scratch_high_water_bytes = 33_304;
        value.scratch_operation_statements = burst.edits.len() as u64 * 3;
        for edit in &burst.edits {
            let suffix = edit.before_bytes - edit.offset - edit.delete_bytes;
            let patch = edit.kind == EditKind::Overwrite;
            value.native.bytes_read += if patch { 0 } else { suffix };
            value.native.bytes_written += if patch {
                edit.insert_bytes
            } else {
                suffix + edit.insert_bytes
            };
            value.native.patch_bytes += edit.insert_bytes;
            value.native.suffix_bytes_shifted += if patch { 0 } else { suffix };
            value.native.clone_attempts += u64::from(patch);
            value.native.clone_successes += u64::from(patch);
            sub_edits.push(SubEditReceipt {
                edit: edit.clone(),
                native_wall_ns: 10,
                physical_oracle_wall_ns: 10,
                native_route: if edit.kind == EditKind::Overwrite {
                    "ClonePatch".to_owned()
                } else {
                    "InPlaceShift".to_owned()
                },
                native_bytes_read: if patch { 0 } else { suffix },
                native_bytes_written: if patch {
                    edit.insert_bytes
                } else {
                    suffix + edit.insert_bytes
                },
                native_patch_bytes: edit.insert_bytes,
                native_suffix_bytes_shifted: if patch { 0 } else { suffix },
                native_clone_attempts: u64::from(patch),
                native_clone_successes: u64::from(patch),
                native_clone_fallbacks: 0,
                native_full_fallback_files: 0,
                tree_level_before: Some(1),
                locality: Some(ContentCounters {
                    cdc_bytes_scanned: edit.insert_bytes,
                    payload_bytes_written: edit.insert_bytes,
                    rope_nodes_read: 1,
                    rope_nodes_emitted: 1,
                    ..ContentCounters::default()
                }),
            });
        }
    }
    (operation, sub_edits)
}
