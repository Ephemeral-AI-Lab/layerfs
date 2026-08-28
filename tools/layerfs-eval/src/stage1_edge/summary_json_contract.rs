use super::artifact::json_string;
use super::authentication::json_array_objects;
use super::row_parse::json_object;
use super::summary_json_parse::{json_object_member_names, require_json_keys};
use crate::stage1_fixture::EvalResult;
pub(crate) fn validate_summary_json_contract(json: &str) -> EvalResult<()> {
    require_json_keys(
        json,
        None,
        &[
            "schema",
            "status",
            "source",
            "fixture",
            "population",
            "roots",
            "walls_ns",
            "physical_to_logical",
            "logical_to_physical",
            "refresh_routes",
            "bursts",
            "history",
            "materialization",
            "canonical_locality",
            "transactions",
            "authentication",
            "storage",
            "resources",
            "timer_closure",
            "correctness",
            "optimization",
            "unavailable",
            "failures",
            "artifacts",
            "disposition_reason",
        ],
    )?;
    for (object, keys) in [
        (
            "source",
            &[
                "git_commit",
                "dirty_tree",
                "tree_blake3",
                "manifest_sha256",
                "release_executable_path",
                "release_executable_sha256",
                "release_executable_blake3",
            ][..],
        ),
        (
            "fixture",
            &[
                "master_path",
                "master_sha256",
                "fixture_blake3",
                "apfs_identity",
                "initial_bytes",
                "maximum_bytes",
                "terminal_bytes",
                "master_unchanged",
            ],
        ),
        (
            "population",
            &[
                "expected_rows",
                "valid_rows",
                "expected_edit_suboperations",
                "observed_edit_suboperations",
                "expected_transitions",
                "observed_transitions",
                "measured_workflows",
            ],
        ),
        (
            "roots",
            &[
                "R0", "R5", "R10", "R15", "R20", "R25", "R30", "R31", "R32", "R33", "R34",
            ],
        ),
        (
            "walls_ns",
            &[
                "complete_wall",
                "row_wall_sum",
                "outside_rows_wall",
                "timer_residual",
                "admission",
                "reset",
                "store_open",
                "initial_materialization",
                "physical_phase",
                "physical_history_phase",
                "logical_refresh_phase",
                "logical_history_phase",
                "burst_phase",
                "milestone_materialization_phase",
                "cleanup",
                "artifact_write",
            ],
        ),
        (
            "physical_to_logical",
            &[
                "by_kind",
                "by_size_band",
                "native_edit",
                "durable_checkpoint",
                "edit_plus_checkpoint",
                "count_change_amplification",
                "physical_oracle",
            ],
        ),
        (
            "logical_to_physical",
            &[
                "by_kind",
                "by_size_band",
                "direct_logical_edit",
                "changed_root_refresh",
                "logical_edit_plus_refresh",
                "physical_oracle",
            ],
        ),
        (
            "refresh_routes",
            &[
                "clone_patch",
                "in_place_patch",
                "patch_aggregate",
                "clone_shift",
                "in_place_shift",
                "shift_aggregate",
                "insert_shift",
                "delete_shift",
                "append_shift",
                "truncate_shift",
                "full_fallback_count",
            ],
        ),
        (
            "bursts",
            &[
                "by_root",
                "aggregate",
                "suboperation_count",
                "checkpoint_count",
                "transaction_count",
            ],
        ),
        (
            "history",
            &[
                "sessions",
                "aggregate",
                "selected_roots",
                "verified_open_count",
                "probe_count",
                "first_probe",
                "second_probe",
                "third_probe",
                "first_probe_non_payload_rows",
                "warm_probe_non_payload_rows",
            ],
        ),
        (
            "materialization",
            &[
                "initial",
                "by_root",
                "milestone_aggregate",
                "live_workspace_materializations",
                "witness_materializations",
                "workspace_reuses",
                "rematerializations",
            ],
        ),
        (
            "canonical_locality",
            &[
                "physical_checkpoints",
                "direct_logical_edits",
                "save_bursts",
                "total",
                "cdc_bytes_expected",
                "cdc_bytes_observed",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "maximum_rope_nodes_read",
                "maximum_rope_nodes_emitted",
                "content_directory_nodes_emitted",
                "payload_batch_maximum",
            ],
        ),
        (
            "transactions",
            &[
                "expected",
                "observed",
                "committed",
                "rolled_back",
                "publication_commits",
                "publication_transactions_started",
                "publication_transactions_rolled_back",
                "admission_transactions_started",
                "admission_transactions_committed",
                "admission_transactions_rolled_back",
                "admission_statements",
                "integrity_transactions_started",
                "integrity_transactions_committed",
                "integrity_transactions_rolled_back",
                "integrity_statements",
                "retained_roots_validated",
                "generation_increment_failures",
            ],
        ),
        (
            "authentication",
            &[
                "fetched_authentication_failures",
                "fetched_role_decode_failures",
                "new_object_equation_failures",
                "incumbent_equation_failures",
                "payload_batch_maximum",
                "phase_attribution",
            ],
        ),
        (
            "storage",
            &[
                "initial_database_bytes",
                "terminal_database_bytes",
                "initial_logical_engine_bytes",
                "terminal_logical_engine_bytes",
                "canonical_object_bytes_written",
                "database_growth_bytes",
                "maximum_transition_database_growth_bytes",
                "physical_to_canonical_amplification",
                "scratch_high_water_bytes",
                "rollback_journal_bytes",
                "terminal_sidecars",
                "by_root_range",
            ],
        ),
        (
            "resources",
            &[
                "rss_peak_bytes",
                "largest_buffer_bytes",
                "operation_q_high_water_bytes",
                "operation_q_maximum_terminal_bytes",
                "page_size",
                "cache_pages",
                "cache_spill_pages",
                "store_connection_high_water",
                "store_connections_terminal",
                "fd_baseline",
                "fd_terminal",
                "product_child_process_peak",
                "child_processes_terminal",
                "owned_temp_residue_entries",
                "sidecar_residue_entries",
                "live_rematerializations",
                "network_operations",
            ],
        ),
        (
            "timer_closure",
            &[
                "by_row_group",
                "maximum_row_residual_ns",
                "row_residual_sum_ns",
                "complete_wall_ns",
                "row_wall_sum_ns",
                "outside_rows_wall_ns",
                "timer_residual_ns",
                "hard_limit_ns",
            ],
        ),
        (
            "correctness",
            &[
                "physical_oracles_expected",
                "physical_oracles_passed",
                "canonical_transitions_expected",
                "canonical_transitions_passed",
                "save_bursts_expected",
                "save_bursts_passed",
                "selected_history_roots_expected",
                "selected_history_roots_passed",
                "route_labels_exact",
                "terminal_length_exact",
                "fixture_unchanged",
            ],
        ),
        (
            "optimization",
            &[
                "baseline_run",
                "baseline_rows_sha256",
                "baseline_summary_sha256",
                "complete_wall",
                "counter_snapshot_wall",
                "history_read_wall",
                "verified_open_by_root",
                "append_truncate_refresh",
                "milestone_materialization",
                "shift_routes",
            ],
        ),
        (
            "artifacts",
            &[
                "environment_sha256",
                "master_sha256",
                "readiness_sha256",
                "schedule_sha256",
                "rows_sha256",
                "rows_line_count",
                "campaign_time_sha256",
                "release_executable_sha256",
                "release_executable_blake3",
                "source_tree_blake3",
                "source_manifest_sha256",
            ],
        ),
    ] {
        require_json_keys(json, Some(object), keys)?;
    }
    for (parent, map) in [
        (None, "roots"),
        (Some("physical_to_logical"), "by_kind"),
        (Some("physical_to_logical"), "by_size_band"),
        (Some("physical_to_logical"), "count_change_amplification"),
        (Some("logical_to_physical"), "by_kind"),
        (Some("logical_to_physical"), "by_size_band"),
        (Some("bursts"), "by_root"),
        (Some("materialization"), "by_root"),
        (Some("authentication"), "phase_attribution"),
        (Some("optimization"), "verified_open_by_root"),
        (Some("storage"), "by_root_range"),
        (Some("timer_closure"), "by_row_group"),
    ] {
        let scope = parent.map_or(Ok(json), |key| json_object(json, key))?;
        let object = json_object(scope, map)?;
        if json_object_member_names(object)?.is_empty() {
            return Err(format!("summary JSON map {parent:?}.{map} is empty"));
        }
    }
    let phase_attribution = json_object(json_object(json, "authentication")?, "phase_attribution")?;
    if json_object_member_names(phase_attribution)?
        != [
            "store_open",
            "materialization",
            "checkpoint",
            "logical_edit",
            "apfs_refresh",
            "canonical_witness",
            "verified_open",
            "history_read",
            "storage_observation",
        ]
    {
        return Err("summary JSON exact phase attribution population".to_owned());
    }
    let unavailable = json_array_objects(json, "unavailable")?;
    if unavailable.is_empty()
        || unavailable
            .iter()
            .any(|value| json_string(value, "availability").as_deref() != Ok("Unavailable"))
    {
        return Err("summary JSON unavailable availability contract".to_owned());
    }
    Ok(())
}
