use super::super::limits::{FIXTURE_MODE, FIXTURE_MTIME_SECONDS};
use super::super::oracle::metadata_receipt_json;
use super::super::receipt_model::{OracleReceipt, Phase, ResourceObservation, RowReceipt};
use super::super::resources::unavailable_defaults;
use super::super::row_parse::scheduled_lengths;
use super::super::row_physical::history_custody_json;
use super::super::schedule_model::{FrozenSchedule, ScheduledRow};
use super::synthetic_engine::synthetic_engine;
use super::synthetic_history::synthetic_history;
use super::synthetic_operation::synthetic_operation;
use super::synthetic_phase_counters::synthetic_phase_counters;
use super::synthetic_phases::synthetic_phases;
use super::synthetic_routing::synthetic_routing;
use super::synthetic_storage::synthetic_storage;
use crate::legacy_full::{NativeMetadata, RefState, RootId};
pub(super) fn synthetic_root(index: u8) -> RootId {
    RootId::from_bytes(blake3::hash(&[index]).as_bytes()).unwrap()
}
pub(super) fn synthetic_ref(index: u8) -> RefState {
    RefState {
        name: "main".to_owned(),
        generation: u64::from(index) + 1,
        root: synthetic_root(index),
    }
}
pub(super) fn synthetic_root_digest(index: u8) -> String {
    blake3::hash(&[index]).to_hex().to_string()
}
pub(super) fn synthetic_phase(name: &'static str) -> Phase {
    Phase { name, wall_ns: 10 }
}
pub(super) fn synthetic_metadata(root: u8) -> NativeMetadata {
    NativeMetadata {
        mode: FIXTURE_MODE,
        mtime_seconds: FIXTURE_MTIME_SECONDS as i64 + i64::from(root) + 1,
        mtime_nanoseconds: u32::from(root) + 1,
        xattrs: crate::legacy_full::NativeXattrs::new(),
        acl: None,
        bsd_flags: 0,
    }
}
pub(super) fn synthetic_pass_row(
    schedule: &FrozenSchedule,
    scheduled: &ScheduledRow,
) -> RowReceipt {
    let edit = scheduled
        .edit_index
        .map(|index| schedule.edits[index].clone());
    let transition = scheduled.transition_root;
    let (before_bytes, after_bytes) = scheduled_lengths(schedule, scheduled).unwrap();
    let phases = synthetic_phases(scheduled);
    let row_wall_ns = phases.iter().map(|phase| phase.wall_ns).sum::<u128>();
    let (operation, sub_edits) = synthetic_operation(schedule, scheduled, edit.as_ref());
    let engine = synthetic_engine(scheduled, transition, operation.is_some());
    let storage = synthetic_storage(scheduled, transition, edit.as_ref(), operation.is_some());
    let active_store_connections = match scheduled.row_group {
        "C00" | "C01" | "C09" => 0,
        "C04" | "C06" | "C08" => 2,
        _ => 1,
    };
    let resources = ResourceObservation {
        rss_current_bytes: Some(20_000_000),
        rss_peak_bytes: 20_000_000,
        fd_current: 5,
        active_store_connections,
        child_processes: 0,
        owned_temp_entries: (scheduled.row_group == "C09").then_some(0),
        residue_entries: 0,
    };
    let (pre_ref, post_ref, native_route) = synthetic_routing(scheduled, transition, edit.as_ref());
    let phase_counters = synthetic_phase_counters(
        scheduled,
        engine,
        operation.as_ref(),
        active_store_connections,
    );
    let history_probes = synthetic_history(schedule, scheduled);
    RowReceipt {
            schedule: scheduled.clone(),
            status: "PASS",
            before_bytes,
            after_bytes,
            edit,
            sub_edits,
            history_probes,
            pre_ref,
            post_ref,
            native_route: native_route.to_owned(),
            tree_level_before: matches!(scheduled.row_group, "C03" | "C05").then_some(1),
            phases,
            phase_counters,
            row_wall_ns,
            row_residual_ns: 0,
            engine,
            operation,
            storage_before: storage.as_ref().map(|value| value.0),
            storage_after: storage.as_ref().map(|value| value.1),
            resources,
            oracle: OracleReceipt {
                logical_length: after_bytes,
                content_digest: if scheduled.row_group == "C09" {
                    String::new()
                } else {
                    synthetic_root_digest(
                        scheduled
                            .transition_root
                            .or(scheduled.milestone_root)
                            .or_else(|| scheduled.history_session.map(|session| session * 5))
                            .unwrap_or(if scheduled.row_group == "C02" { 0 } else { 34 }),
                    )
                },
                physical_bytes_exact: matches!(scheduled.row_group, "C03" | "C05" | "C07" | "C08").then_some(true),
                canonical_bytes_exact: matches!(scheduled.row_group, "C02" | "C03" | "C05" | "C07" | "C08").then_some(true),
                metadata_exact: matches!(scheduled.row_group, "C02" | "C03" | "C05" | "C07" | "C08").then_some(true),
                historical_roots_exact: matches!(scheduled.row_group, "C04" | "C06" | "C08").then_some(true),
                route_exact: (scheduled.row_group != "C09").then_some(true),
            },
            unavailable: unavailable_defaults(),
            error: None,
            custody: match scheduled.row_group {
                "C04" | "C06" => Some(
                    history_custody_json(scheduled.history_session.unwrap()).unwrap(),
                ),
                "C08" => {
                    let root = scheduled.milestone_root.unwrap();
                    let metadata = metadata_receipt_json(&synthetic_metadata(root));
                    Some(format!(
                        concat!(
                            "{{\"milestone_root\":\"R{}\",\"extra_user_files\":0,",
                            "\"fresh_extra_user_files\":0,\"live_extra_user_files\":{},",
                            "\"cleanup_residue_entries\":0,\"metadata\":{},",
                            "\"retained_metadata\":{},\"fresh_metadata\":{},",
                            "\"live_metadata\":{}}}"
                        ),
                        root,
                        if root == 34 { "0" } else { "null" },
                        metadata,
                        metadata,
                        metadata,
                        if root == 34 { metadata.as_str() } else { "null" },
                    ))
                }
                "C09" => Some("{\"pre_cleanup_active_store_connections\":0,\"pre_cleanup_fd_count\":5,\"pre_cleanup_child_processes\":0,\"pre_cleanup_residue_entries\":0,\"post_cleanup_active_store_connections\":0,\"post_cleanup_fd_count\":5,\"post_cleanup_child_processes\":0,\"post_cleanup_residue_entries\":0,\"fixture_unchanged\":true}".to_owned()),
                _ => None,
            },
        }
}
