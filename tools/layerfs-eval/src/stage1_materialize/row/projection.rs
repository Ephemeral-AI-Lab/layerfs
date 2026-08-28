use super::super::contract::EvalResult;
use crate::legacy_full::{
    ProjectionCallFacts, ProjectionCleanupFacts, ProjectionFacts, ProjectionReplaceFacts,
    ProjectionSyncFacts, ProjectionTimer, ProjectionTimerAvailability, ProjectionWriteFacts,
};

pub(in crate::stage1_materialize) fn projection_json(facts: ProjectionFacts) -> String {
    format!(
        concat!(
            "{{\"workspace_setup\":{},\"workspace_root_create_open\":{},",
            "\"staging_create_open\":{},\"recovery_marker_create\":{},",
            "\"name_preflight\":{},\"temp_create\":{},",
            "\"workspace_marker_write\":{},\"content_write\":{},",
            "\"metadata_value_write\":{},\"aggregate_native_write\":{},",
            "\"content_flush\":{},\"metadata_validate\":{},\"metadata_apply\":{},",
            "\"metadata_preinstall_verify\":{},\"metadata_postinstall_verify\":{},",
            "\"root_binding_revalidate\":{},\"regular_file_sync\":{},",
            "\"directory_sync\":{},\"recovery_marker_file_sync\":{},",
            "\"content_temp_file_sync\":{},\"post_hardlink_file_sync\":{},",
            "\"staging_directory_sync\":{},\"root_parent_directory_sync\":{},",
            "\"install_parent_directory_sync\":{},\"dirty_tree_directory_sync\":{},",
            "\"final_root_directory_sync\":{},\"replace\":{},",
            "\"authority_completion\":{},\"cleanup\":{}}}"
        ),
        call_json(facts.workspace_setup),
        call_json(facts.workspace_root_create_open),
        call_json(facts.staging_create_open),
        call_json(facts.recovery_marker_create),
        call_json(facts.name_preflight),
        call_json(facts.temp_create),
        write_json(facts.workspace_marker_write),
        write_json(facts.content_write),
        write_json(facts.metadata_value_write),
        write_json(facts.aggregate_native_write),
        call_json(facts.content_flush),
        call_json(facts.metadata_validate),
        call_json(facts.metadata_apply),
        call_json(facts.metadata_preinstall_verify),
        call_json(facts.metadata_postinstall_verify),
        call_json(facts.root_binding_revalidate),
        sync_json(facts.regular_file_sync),
        sync_json(facts.directory_sync),
        sync_json(facts.recovery_marker_file_sync),
        sync_json(facts.content_temp_file_sync),
        sync_json(facts.post_hardlink_file_sync),
        sync_json(facts.staging_directory_sync),
        sync_json(facts.root_parent_directory_sync),
        sync_json(facts.install_parent_directory_sync),
        sync_json(facts.dirty_tree_directory_sync),
        sync_json(facts.final_root_directory_sync),
        replace_json(facts.replace),
        call_json(facts.authority_completion),
        cleanup_json(facts.cleanup),
    )
}

pub(in crate::stage1_materialize) fn timer_json(timer: ProjectionTimer) -> String {
    match timer.availability {
        ProjectionTimerAvailability::Available => format!(
            "{{\"availability\":\"Available\",\"nanoseconds\":{}}}",
            timer.nanoseconds
        ),
        ProjectionTimerAvailability::Unavailable => {
            "{\"availability\":\"Unavailable\",\"nanoseconds\":null}".to_owned()
        }
    }
}

pub(in crate::stage1_materialize) fn call_json(facts: ProjectionCallFacts) -> String {
    format!(
        "{{\"attempts\":{},\"successes\":{},\"failures\":{},\"wall\":{}}}",
        facts.attempts,
        facts.successes,
        facts.failures,
        timer_json(facts.wall),
    )
}

pub(in crate::stage1_materialize) fn write_json(facts: ProjectionWriteFacts) -> String {
    format!(
        "{{\"attempts\":{},\"successes\":{},\"failures\":{},\"bytes\":{},\"wall\":{}}}",
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.bytes,
        timer_json(facts.wall),
    )
}

pub(in crate::stage1_materialize) fn sync_json(facts: ProjectionSyncFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"requested\":{{\"process_crash_reconciled\":{},",
            "\"host_crash_ordered\":{},\"device_flush_requested\":{},",
            "\"power_loss_qualified\":{}}},",
            "\"achieved\":{{\"process_crash_reconciled\":{},",
            "\"host_crash_ordered\":{},\"device_flush_requested\":{},",
            "\"power_loss_qualified\":{}}},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.requested.process_crash_reconciled,
        facts.requested.host_crash_ordered,
        facts.requested.device_flush_requested,
        facts.requested.power_loss_qualified,
        facts.achieved.process_crash_reconciled,
        facts.achieved.host_crash_ordered,
        facts.achieved.device_flush_requested,
        facts.achieved.power_loss_qualified,
        timer_json(facts.wall),
    )
}

pub(in crate::stage1_materialize) fn replace_json(facts: ProjectionReplaceFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"requested_visible\":{},\"prior_visible\":{},",
            "\"visibility_ambiguous\":{},\"durability_ambiguous\":{},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.requested_visible,
        facts.prior_visible,
        facts.visibility_ambiguous,
        facts.durability_ambiguous,
        timer_json(facts.wall),
    )
}

pub(in crate::stage1_materialize) fn cleanup_json(facts: ProjectionCleanupFacts) -> String {
    format!(
        concat!(
            "{{\"attempts\":{},\"successes\":{},\"failures\":{},",
            "\"residue\":{},\"wall\":{}}}"
        ),
        facts.attempts,
        facts.successes,
        facts.failures,
        facts.residue,
        timer_json(facts.wall),
    )
}

pub(in crate::stage1_materialize) fn delta(after: u64, before: u64, name: &str) -> EvalResult<u64> {
    after
        .checked_sub(before)
        .ok_or_else(|| format!("counter {name} moved backwards"))
}
