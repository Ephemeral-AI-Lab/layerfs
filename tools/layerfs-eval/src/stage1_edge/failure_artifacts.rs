use super::artifact::{
    display_error, durable_replace, json_escape, json_string, json_u128, sha256_file,
};
use super::campaign::{null_map, optional_artifact_sha256};
use super::context::failure_observation;
use super::report_disposition::validate_timer_equation;
use super::schedule::frozen_schedule;
use super::summary_json_contract::validate_summary_json_contract;
use super::summary_markdown_contract::{
    validate_summary_markdown_contract, SUMMARY_HEADINGS, SUMMARY_TABLE_HEADERS,
};
use crate::stage1_fixture::EvalResult;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;
pub(crate) fn failure_summary_json(
    run: &Path,
    error: &str,
    phase: &str,
    population: (usize, usize, usize),
    walls: (u128, u128, u128),
) -> EvalResult<String> {
    let (rows_valid, edit_suboperations_observed, transitions_observed) = population;
    let (complete_wall_ns, row_wall_sum_ns, outside_rows_wall_ns) = walls;
    let kinds = null_map(&["overwrite", "insert", "delete", "append", "truncate"]);
    let bands = null_map(&["near-8-kib", "near-16-kib", "near-32-kib"]);
    let selected_roots = null_map(&[
        "R0", "R5", "R10", "R15", "R20", "R25", "R30", "R31", "R32", "R33", "R34",
    ]);
    let by_row_group = null_map(&[
        "C00", "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09",
    ]);
    let authentication = format!(
        concat!(
            "{{\"fetched_authentication_failures\":null,",
            "\"fetched_role_decode_failures\":null,",
            "\"new_object_equation_failures\":null,",
            "\"incumbent_equation_failures\":null,",
            "\"payload_batch_maximum\":null,\"phase_attribution\":{}}}"
        ),
        null_map(&[
            "store_open",
            "materialization",
            "checkpoint",
            "logical_edit",
            "apfs_refresh",
            "canonical_witness",
            "verified_open",
            "history_read",
            "storage_observation",
        ])
    );
    let optimization = format!(
        concat!(
            "{{\"baseline_run\":null,\"baseline_rows_sha256\":null,",
            "\"baseline_summary_sha256\":null,\"complete_wall\":null,",
            "\"counter_snapshot_wall\":null,\"history_read_wall\":null,",
            "\"verified_open_by_root\":{},\"append_truncate_refresh\":null,",
            "\"milestone_materialization\":null,\"shift_routes\":null}}"
        ),
        null_map(&["R5", "R15", "R30", "R34"])
    );
    let artifacts = format!(
        concat!(
            "\"environment_sha256\":{},\"master_sha256\":{},",
            "\"readiness_sha256\":{},\"schedule_sha256\":{},",
            "\"rows_sha256\":\"{}\",\"rows_line_count\":{},",
            "\"campaign_time_sha256\":\"{}\",",
            "\"release_executable_sha256\":null,\"release_executable_blake3\":null,",
            "\"source_tree_blake3\":null,\"source_manifest_sha256\":null"
        ),
        optional_artifact_sha256(&run.join("environment.json"))?,
        optional_artifact_sha256(&run.join("master.json"))?,
        optional_artifact_sha256(&run.join("readiness.json"))?,
        optional_artifact_sha256(&run.join("schedule.json"))?,
        sha256_file(&run.join("rows.jsonl"))?,
        rows_valid,
        sha256_file(&run.join("campaign-time.txt"))?,
    );
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-summary-v1\",\"status\":\"FAIL\",",
            "\"source\":{},\"fixture\":{},",
            "\"population\":{{\"expected_rows\":47,\"valid_rows\":{},",
            "\"expected_edit_suboperations\":51,\"observed_edit_suboperations\":{},",
            "\"expected_transitions\":34,\"observed_transitions\":{},",
            "\"measured_workflows\":1}},\"roots\":{},",
            "\"walls_ns\":{{\"complete_wall\":{},\"row_wall_sum\":{},",
            "\"outside_rows_wall\":{},\"timer_residual\":0,",
            "\"admission\":null,\"reset\":null,\"store_open\":null,",
            "\"initial_materialization\":null,\"physical_phase\":null,",
            "\"physical_history_phase\":null,\"logical_refresh_phase\":null,",
            "\"logical_history_phase\":null,\"burst_phase\":null,",
            "\"milestone_materialization_phase\":null,\"cleanup\":null,",
            "\"artifact_write\":null}},",
            "\"physical_to_logical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"native_edit\":null,\"durable_checkpoint\":null,",
            "\"edit_plus_checkpoint\":null,\"count_change_amplification\":{},",
            "\"physical_oracle\":null}},",
            "\"logical_to_physical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"direct_logical_edit\":null,\"changed_root_refresh\":null,",
            "\"logical_edit_plus_refresh\":null,\"physical_oracle\":null}},",
            "\"refresh_routes\":{},",
            "\"bursts\":{{\"by_root\":{},\"aggregate\":null,",
            "\"suboperation_count\":null,\"checkpoint_count\":null,",
            "\"transaction_count\":null}},",
            "\"history\":{},",
            "\"materialization\":{{\"initial\":null,\"by_root\":{},",
            "\"milestone_aggregate\":null,\"live_workspace_materializations\":null,",
            "\"witness_materializations\":null,\"workspace_reuses\":null,",
            "\"rematerializations\":null}},",
            "\"canonical_locality\":{},\"transactions\":{},",
            "\"authentication\":{},",
            "\"storage\":{{\"initial_database_bytes\":null,",
            "\"terminal_database_bytes\":null,\"initial_logical_engine_bytes\":null,",
            "\"terminal_logical_engine_bytes\":null,",
            "\"canonical_object_bytes_written\":null,\"database_growth_bytes\":null,",
            "\"maximum_transition_database_growth_bytes\":null,",
            "\"physical_to_canonical_amplification\":null,",
            "\"scratch_high_water_bytes\":null,\"rollback_journal_bytes\":null,",
            "\"terminal_sidecars\":null,\"by_root_range\":{}}},",
            "\"resources\":{},",
            "\"timer_closure\":{{\"by_row_group\":{},",
            "\"maximum_row_residual_ns\":null,\"row_residual_sum_ns\":null,",
            "\"complete_wall_ns\":{},\"row_wall_sum_ns\":{},",
            "\"outside_rows_wall_ns\":{},\"timer_residual_ns\":0,",
            "\"hard_limit_ns\":60000000000}},",
            "\"correctness\":{{\"physical_oracles_expected\":51,",
            "\"physical_oracles_passed\":null,\"canonical_transitions_expected\":34,",
            "\"canonical_transitions_passed\":null,\"save_bursts_expected\":4,",
            "\"save_bursts_passed\":null,\"selected_history_roots_expected\":8,",
            "\"selected_history_roots_passed\":null,\"route_labels_exact\":null,",
            "\"terminal_length_exact\":null,\"fixture_unchanged\":null}},",
            "\"optimization\":{},",
            "\"unavailable\":[{{\"field\":\"summary.remaining_observations\",",
            "\"availability\":\"Unavailable\",",
            "\"reason\":\"campaign stopped at the first failed equation\"}}],",
            "\"failures\":[{{\"phase\":\"{}\",",
            "\"first_failed_equation\":\"{}\"}}],",
            "\"artifacts\":{{{}}},\"disposition_reason\":\"{}\"}}\n"
        ),
        null_map(&[
            "git_commit",
            "dirty_tree",
            "tree_blake3",
            "manifest_sha256",
            "release_executable_path",
            "release_executable_sha256",
            "release_executable_blake3",
        ]),
        null_map(&[
            "master_path",
            "master_sha256",
            "fixture_blake3",
            "apfs_identity",
            "initial_bytes",
            "maximum_bytes",
            "terminal_bytes",
            "master_unchanged",
        ]),
        rows_valid,
        edit_suboperations_observed,
        transitions_observed,
        selected_roots,
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        kinds,
        bands,
        null_map(&["insert", "delete", "append", "truncate"]),
        kinds,
        bands,
        null_map(&[
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
        ]),
        null_map(&["R31", "R32", "R33", "R34"]),
        null_map(&[
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
        ]),
        null_map(&["R15", "R30", "R34"]),
        null_map(&[
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
        ]),
        null_map(&[
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
        ]),
        authentication,
        null_map(&["R0-R15", "R15-R30", "R30-R34"]),
        null_map(&[
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
        ]),
        by_row_group,
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        optimization,
        json_escape(phase),
        json_escape(error),
        artifacts,
        json_escape(error),
    );
    validate_summary_json_contract(&summary)?;
    Ok(summary)
}
pub(crate) fn unavailable_markdown_table(header: &str) -> String {
    let columns = header.matches('|').count().saturating_sub(1);
    let separator = format!("|{}|", vec!["---"; columns].join("|"));
    let mut values = vec!["—"; columns];
    if let Some(first) = values.first_mut() {
        *first = "Unavailable";
    }
    format!("{header}\n{separator}\n|{}|", values.join("|"))
}
pub(crate) fn failure_summary_markdown(error: &str, phase: &str) -> EvalResult<String> {
    let tables: [&[usize]; 16] = [
        &[0, 1],
        &[2],
        &[3, 4],
        &[5, 6],
        &[7],
        &[8],
        &[9],
        &[10],
        &[11, 12],
        &[13],
        &[14, 15],
        &[16, 17],
        &[18],
        &[19],
        &[20],
        &[21, 22],
    ];
    let mut markdown = format!("{}\n\nDisposition: `FAIL`\n\n", SUMMARY_HEADINGS[0]);
    for (heading, table_indices) in SUMMARY_HEADINGS[1..].iter().zip(tables) {
        writeln!(
            markdown,
            "{heading}\n\nFailed in `{}` before terminal PASS: `{}`.\n",
            phase, error
        )
        .map_err(display_error)?;
        for index in table_indices {
            writeln!(
                markdown,
                "{}\n",
                unavailable_markdown_table(SUMMARY_TABLE_HEADERS[*index])
            )
            .map_err(display_error)?;
        }
    }
    validate_summary_markdown_contract(&markdown)?;
    Ok(markdown)
}
pub(crate) fn write_failure_artifacts(
    run: &Path,
    error: &str,
    started_unix_ns: u128,
    complete_wall_ns: u128,
) -> EvalResult<()> {
    let rows_path = run.join("rows.jsonl");
    let contents = fs::read_to_string(&rows_path).unwrap_or_default();
    let rows_valid = contents.lines().count();
    let (_, context_phase, _) = failure_observation();
    let first_failed_phase = contents
        .lines()
        .rev()
        .find(|row| json_string(row, "status").as_deref() == Ok("FAIL"))
        .map(|row| json_string(row, "phase"))
        .transpose()?
        .unwrap_or_else(|| context_phase.to_owned());
    let row_wall_sum_ns = contents
        .lines()
        .map(|row| json_u128(row, "row_wall_ns"))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .sum::<u128>();
    let outside_rows_wall_ns = complete_wall_ns
        .checked_sub(row_wall_sum_ns)
        .ok_or_else(|| "failure timer row_wall_sum_ns <= complete_wall_ns".to_owned())?;
    let schedule = frozen_schedule()?;
    let edit_suboperations_observed = schedule
        .rows
        .iter()
        .zip(contents.lines())
        .filter(|(_, receipt)| json_string(receipt, "status").as_deref() == Ok("PASS"))
        .map(|(row, _)| {
            if row.edit_index.is_some() {
                1
            } else {
                row.burst_index
                    .map_or(0, |index| schedule.bursts[index].edits.len())
            }
        })
        .sum::<usize>();
    let transitions_observed = contents
        .lines()
        .filter(|row| {
            matches!(
                json_string(row, "row_group").as_deref(),
                Ok("C03" | "C05" | "C07")
            ) && !row.contains("\"post_ref\":null")
        })
        .count();
    let time = format!(
        concat!(
            "schema=layerfs-stage1.1-campaign-time-v1\nstatus=FAIL\n",
            "started_unix_ns={}\ncompleted_unix_ns={}\ncomplete_wall_ns={}\n",
            "row_wall_sum_ns={}\noutside_rows_wall_ns={}\ntimer_residual_ns=0\n",
            "hard_limit_ns=60000000000\nrows_expected=47\nrows_valid={}\n",
            "edit_suboperations_expected=51\nedit_suboperations_observed={}\n",
            "transitions_expected=34\ntransitions_observed={}\n"
        ),
        started_unix_ns,
        started_unix_ns.saturating_add(complete_wall_ns),
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        rows_valid,
        edit_suboperations_observed,
        transitions_observed,
    );
    validate_timer_equation(&time)?;
    durable_replace(&run.join("campaign-time.txt"), &time)?;
    let summary = failure_summary_json(
        run,
        error,
        &first_failed_phase,
        (
            rows_valid,
            edit_suboperations_observed,
            transitions_observed,
        ),
        (complete_wall_ns, row_wall_sum_ns, outside_rows_wall_ns),
    )?;
    durable_replace(&run.join("summary.json"), &summary)?;
    let markdown = failure_summary_markdown(error, &first_failed_phase)?;
    durable_replace(&run.join("summary.md"), &markdown)
}
