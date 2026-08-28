use super::artifact::{json_escape, sha256_file};
use super::campaign::Campaign;
use super::engine_counters::FixtureMaster;
use super::fixture::{fixture_root, SourceIdentity};
use super::limits::{INITIAL_BYTES, MAXIMUM_BYTES};
use super::markdown_helpers::{sum_phase, sum_row_walls};
use super::optimization::{optimization_comparison, optimization_json};
use super::report_disposition::{
    apply_disposition, derive_disposition, maximum_key, maximum_locality_key, sum_key,
    sum_locality_key,
};
use super::report_groups::{
    count_change_amplification_json, history_probe_stats, history_probe_sum, logical_by_kind_json,
    logical_by_size_json, materialization_by_root_json, phase_attribution_json, phase_attributions,
    physical_by_kind_json, physical_by_size_json, root_json, route_stats,
    storage_by_root_range_json,
};
use super::row_parse::{phase_wall, ParsedRow};
use super::statistics::{
    combined_phase_stats, roots_from_rows, row_phase_stats, statistics, stats_json,
};
use super::summary_fragments::{summary_fragments, SummaryFragments};
use super::summary_gate::{validate_summary_gates, SummaryGateFacts};
use super::summary_json_contract::validate_summary_json_contract;
use super::summary_json_parse::validate_named_wall_equation;
use crate::legacy_full::PRODUCT_BUFFER_BOUND_BYTES;
use crate::stage1_fixture::EvalResult;
pub(crate) fn summary_json(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    source: &SourceIdentity,
    master: &FixtureMaster,
    complete_wall_ns: u128,
    campaign_time_sha256: &str,
) -> EvalResult<String> {
    let disposition = derive_disposition(rows);
    let SummaryGateFacts {
        authentication: authentication_validation,
        selected_history_roots_passed,
        row_wall_sum,
        outside_rows,
        cdc_physical,
        cdc_logical,
        cdc_bursts,
        cdc_total,
        transactions,
        commits,
        rollbacks,
        publications,
        initial_database,
        terminal_database,
        database_growth,
        canonical_written,
        initial_logical_engine,
        terminal_logical_engine,
        rss_peak,
        q_high_water,
        q_terminal,
        connection_high_water,
        connections_terminal,
        fd_baseline,
        fd_terminal,
        child_peak,
        child_terminal,
        owned_temp_terminal,
        residue_terminal,
        network_operations,
        live_rematerializations,
        physical_oracles_passed,
        canonical_transitions_passed,
        save_bursts_passed,
        observed_edit_suboperations,
        transition_rows: transition_rows_len,
        burst_suboperations,
        history_session_count,
        witness_materializations,
        live_workspace_materializations,
        workspace_reuses,
        route_labels_exact,
        terminal_length_exact,
        fixture_unchanged,
    } = validate_summary_gates(campaign, rows, complete_wall_ns)?;
    let roots = roots_from_rows(rows)?;
    let physical_native = row_phase_stats(rows, "C03", "native_edit")?;
    let physical_checkpoint = row_phase_stats(rows, "C03", "durable_checkpoint")?;
    let physical_combined =
        combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |_| true)?;
    let logical_edit = row_phase_stats(rows, "C05", "direct_logical_edit")?;
    let logical_refresh = row_phase_stats(rows, "C05", "changed_root_refresh")?;
    let logical_combined = combined_phase_stats(
        rows,
        "C05",
        "direct_logical_edit",
        "changed_root_refresh",
        |_| true,
    )?;
    let c03_oracle = row_phase_stats(rows, "C03", "live_physical_oracle")?;
    let c05_oracle = row_phase_stats(rows, "C05", "live_physical_oracle")?;
    let burst_stats = row_phase_stats(rows, "C07", "durable_checkpoint")?;
    let history = statistics(
        rows.iter()
            .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
            .map(|row| phase_wall(&row.json, "verified_open"))
            .collect::<EvalResult<Vec<_>>>()?,
    )?;
    let first_history_probes = history_probe_stats(rows, 1)?;
    let second_history_probes = history_probe_stats(rows, 2)?;
    let third_history_probes = history_probe_stats(rows, 3)?;
    let phase_attribution = phase_attributions(rows)?;
    let optimization = optimization_comparison(rows, complete_wall_ns)?;
    let milestones = row_phase_stats(rows, "C08", "milestone_materialization")?;
    let patch = route_stats(rows, "Patch", None)?;
    let shift = route_stats(rows, "Shift", None)?;
    let insert_shift = route_stats(rows, "Shift", Some("insert"))?;
    let delete_shift = route_stats(rows, "Shift", Some("delete"))?;
    let append_shift = route_stats(rows, "Shift", Some("append"))?;
    let truncate_shift = route_stats(rows, "Shift", Some("truncate"))?;
    let clone_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "ClonePatch")
        .collect::<Vec<_>>();
    let in_place_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlacePatch")
        .collect::<Vec<_>>();
    let clone_shift_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
        .collect::<Vec<_>>();
    let in_place_shift_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
        .collect::<Vec<_>>();
    let clone_stats = (!clone_rows.is_empty())
        .then(|| {
            statistics(
                clone_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let in_place_stats = (!in_place_rows.is_empty())
        .then(|| {
            statistics(
                in_place_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let clone_shift_stats = (!clone_shift_rows.is_empty())
        .then(|| {
            statistics(
                clone_shift_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let in_place_shift_stats = (!in_place_shift_rows.is_empty())
        .then(|| {
            statistics(
                in_place_shift_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let full_fallback_count = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "FullFallback")
        .count();
    let SummaryFragments {
        artifacts,
        by_row_group,
        max_residual,
        residual_sum,
        by_root,
        admission_wall,
        reset_wall,
        store_open_wall,
        initial_materialization_wall,
        cleanup_wall,
        failure_ledger,
    } = summary_fragments(campaign, rows, source, master, campaign_time_sha256)?;
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-summary-v1\",\"status\":\"PASS\",",
            "\"source\":{{\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"tree_blake3\":\"{}\",\"manifest_sha256\":\"{}\",",
            "\"release_executable_path\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\"}},",
            "\"fixture\":{{\"master_path\":\"{}\",\"master_sha256\":\"{}\",",
            "\"fixture_blake3\":\"{}\",\"apfs_identity\":\"{}\",",
            "\"initial_bytes\":{},\"maximum_bytes\":{},\"terminal_bytes\":{},",
            "\"master_unchanged\":true}},",
            "\"population\":{{\"expected_rows\":47,\"valid_rows\":{},",
            "\"expected_edit_suboperations\":51,\"observed_edit_suboperations\":{},",
            "\"expected_transitions\":34,\"observed_transitions\":{},",
            "\"measured_workflows\":1}},",
            "\"roots\":{{{}}},",
            "\"walls_ns\":{{\"complete_wall\":{},\"row_wall_sum\":{},",
            "\"outside_rows_wall\":{},\"timer_residual\":0,",
            "\"admission\":{},\"reset\":{},\"store_open\":{},",
            "\"initial_materialization\":{},\"physical_phase\":{},",
            "\"physical_history_phase\":{},\"logical_refresh_phase\":{},",
            "\"logical_history_phase\":{},\"burst_phase\":{},",
            "\"milestone_materialization_phase\":{},\"cleanup\":{},",
            "\"artifact_write\":{}}},",
            "\"physical_to_logical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"native_edit\":{},\"durable_checkpoint\":{},",
            "\"edit_plus_checkpoint\":{},\"count_change_amplification\":{},",
            "\"physical_oracle\":{}}},",
            "\"logical_to_physical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"direct_logical_edit\":{},\"changed_root_refresh\":{},",
            "\"logical_edit_plus_refresh\":{},\"physical_oracle\":{}}},",
            "\"refresh_routes\":{{\"clone_patch\":{},\"in_place_patch\":{},",
            "\"patch_aggregate\":{},\"clone_shift\":{},\"in_place_shift\":{},",
            "\"shift_aggregate\":{},\"insert_shift\":{},\"delete_shift\":{},",
            "\"append_shift\":{},\"truncate_shift\":{},\"full_fallback_count\":{}}},",
            "\"bursts\":{{\"by_root\":{{{}}},\"aggregate\":{},",
            "\"suboperation_count\":{},\"checkpoint_count\":{},",
            "\"transaction_count\":{}}},",
            "\"history\":{{\"sessions\":{},\"aggregate\":{},",
            "\"selected_roots\":{},\"verified_open_count\":{},",
            "\"probe_count\":63,\"first_probe\":{},\"second_probe\":{},",
            "\"third_probe\":{},\"first_probe_non_payload_rows\":{},",
            "\"warm_probe_non_payload_rows\":{}}},",
            "\"materialization\":{{\"initial\":{},\"by_root\":{},",
            "\"milestone_aggregate\":{},\"live_workspace_materializations\":{},",
            "\"witness_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{}}},",
            "\"canonical_locality\":{{\"physical_checkpoints\":{},",
            "\"direct_logical_edits\":{},\"save_bursts\":{},",
            "\"total\":{},\"cdc_bytes_expected\":495616,",
            "\"cdc_bytes_observed\":{},\"payload_bytes_written\":{},",
            "\"unaffected_payload_reads\":{},\"unaffected_payload_writes\":{},",
            "\"maximum_rope_nodes_read\":{},\"maximum_rope_nodes_emitted\":{},",
            "\"content_directory_nodes_emitted\":{},\"payload_batch_maximum\":{}}},",
            "\"transactions\":{{\"expected\":34,\"observed\":{},",
            "\"committed\":{},\"rolled_back\":{},\"publication_commits\":{},",
            "\"publication_transactions_started\":{},",
            "\"publication_transactions_rolled_back\":{},",
            "\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"retained_roots_validated\":{},",
            "\"generation_increment_failures\":0}},",
            "\"authentication\":{{\"fetched_authentication_failures\":{},",
            "\"fetched_role_decode_failures\":{},\"new_object_equation_failures\":{},",
            "\"incumbent_equation_failures\":{},\"payload_batch_maximum\":{},",
            "\"phase_attribution\":{}}},",
            "\"storage\":{{\"initial_database_bytes\":{},",
            "\"terminal_database_bytes\":{},\"initial_logical_engine_bytes\":{},",
            "\"terminal_logical_engine_bytes\":{},",
            "\"canonical_object_bytes_written\":{},\"database_growth_bytes\":{},",
            "\"maximum_transition_database_growth_bytes\":{},",
            "\"physical_to_canonical_amplification\":{},",
            "\"scratch_high_water_bytes\":{},\"rollback_journal_bytes\":null,",
            "\"terminal_sidecars\":\"absent\",\"by_root_range\":{}}},",
            "\"resources\":{{\"rss_peak_bytes\":{},\"largest_buffer_bytes\":{},",
            "\"operation_q_high_water_bytes\":{},",
            "\"operation_q_maximum_terminal_bytes\":{},\"page_size\":4096,",
            "\"cache_pages\":1280,\"cache_spill_pages\":1280,",
            "\"store_connection_high_water\":{},\"store_connections_terminal\":{},",
            "\"fd_baseline\":{},\"fd_terminal\":{},",
            "\"product_child_process_peak\":{},\"child_processes_terminal\":{},",
            "\"owned_temp_residue_entries\":{},\"sidecar_residue_entries\":{},",
            "\"live_rematerializations\":{},\"network_operations\":{}}},",
            "\"timer_closure\":{{\"by_row_group\":{{{}}},",
            "\"maximum_row_residual_ns\":{},\"row_residual_sum_ns\":{},",
            "\"complete_wall_ns\":{},\"row_wall_sum_ns\":{},",
            "\"outside_rows_wall_ns\":{},\"timer_residual_ns\":0,",
            "\"hard_limit_ns\":60000000000}},",
            "\"correctness\":{{\"physical_oracles_expected\":51,",
            "\"physical_oracles_passed\":{},\"canonical_transitions_expected\":34,",
            "\"canonical_transitions_passed\":{},\"save_bursts_expected\":4,",
            "\"save_bursts_passed\":{},\"selected_history_roots_expected\":8,",
            "\"selected_history_roots_passed\":{},\"route_labels_exact\":{},",
            "\"terminal_length_exact\":{},\"fixture_unchanged\":{}}},",
            "\"optimization\":{},",
            "\"unavailable\":[",
            "{{\"field\":\"native.sync_regular_calls\",\"availability\":\"Unavailable\",\"reason\":\"product exposes only aggregate sync_calls\"}},",
            "{{\"field\":\"native.sync_directory_calls\",\"availability\":\"Unavailable\",\"reason\":\"product exposes only aggregate sync_calls\"}},",
            "{{\"field\":\"storage.rollback_journal_bytes\",\"availability\":\"Unavailable\",\"reason\":\"not continuously observed\"}},",
            "{{\"field\":\"storage.temporary_file_bytes\",\"availability\":\"Unavailable\",\"reason\":\"not continuously observed\"}}],",
            "\"failures\":[{}],\"artifacts\":{{{}}},",
            "\"disposition_reason\":\"All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.\"}}\n"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        json_escape(&fixture_root().join("master.json").display().to_string()),
        sha256_file(&campaign.run.join("master.json"))?,
        master.fixture_blake3,
        json_escape(&master.apfs_identity),
        INITIAL_BYTES,
        MAXIMUM_BYTES,
        INITIAL_BYTES,
        rows.len(),
        observed_edit_suboperations,
        transition_rows_len,
        root_json(&roots),
        complete_wall_ns,
        row_wall_sum,
        outside_rows,
        admission_wall,
        reset_wall,
        store_open_wall,
        initial_materialization_wall,
        sum_row_walls(rows, "C03")?,
        sum_row_walls(rows, "C04")?,
        sum_row_walls(rows, "C05")?,
        sum_row_walls(rows, "C06")?,
        sum_row_walls(rows, "C07")?,
        sum_row_walls(rows, "C08")?,
        cleanup_wall,
        outside_rows,
        physical_by_kind_json(rows)?,
        physical_by_size_json(rows)?,
        stats_json(&physical_native),
        stats_json(&physical_checkpoint),
        stats_json(&physical_combined),
        count_change_amplification_json(rows)?,
        stats_json(&c03_oracle),
        logical_by_kind_json(rows)?,
        logical_by_size_json(rows)?,
        stats_json(&logical_edit),
        stats_json(&logical_refresh),
        stats_json(&logical_combined),
        stats_json(&c05_oracle),
        clone_stats.as_ref().map_or_else(|| "null".to_owned(), stats_json),
        in_place_stats.as_ref().map_or_else(|| "null".to_owned(), stats_json),
        stats_json(&patch),
        clone_shift_stats
            .as_ref()
            .map_or_else(|| "null".to_owned(), stats_json),
        in_place_shift_stats
            .as_ref()
            .map_or_else(|| "null".to_owned(), stats_json),
        stats_json(&shift),
        stats_json(&insert_shift),
        stats_json(&delete_shift),
        stats_json(&append_shift),
        stats_json(&truncate_shift),
        full_fallback_count,
        by_root,
        stats_json(&burst_stats),
        burst_suboperations,
        save_bursts_passed,
        sum_key(rows, Some("C07"), "transactions_started")?,
        history_session_count,
        stats_json(&history),
        selected_history_roots_passed,
        history_session_count,
        stats_json(&first_history_probes),
        stats_json(&second_history_probes),
        stats_json(&third_history_probes),
        history_probe_sum(rows, 1, "non_payload_rows")?,
        history_probe_sum(rows, 2, "non_payload_rows")?
            + history_probe_sum(rows, 3, "non_payload_rows")?,
        stats_json(&statistics(vec![sum_phase(rows, "C02", "cold_materialization")?])?),
        materialization_by_root_json(rows)?,
        stats_json(&milestones),
        live_workspace_materializations,
        witness_materializations,
        workspace_reuses,
        live_rematerializations,
        cdc_physical,
        cdc_logical,
        cdc_bursts,
        cdc_total,
        cdc_total,
        sum_locality_key(rows, "payload_bytes_written")?,
        sum_locality_key(rows, "unaffected_payload_reads")?,
        sum_locality_key(rows, "unaffected_payload_writes")?,
        maximum_locality_key(rows, "rope_nodes_read")?,
        maximum_locality_key(rows, "rope_nodes_emitted")?,
        sum_locality_key(rows, "content_directory_nodes_emitted")?,
        maximum_key(rows, "payload_batch_maximum")?,
        transactions,
        commits,
        rollbacks,
        publications,
        sum_key(rows, None, "publication_transactions_started")?,
        sum_key(rows, None, "publication_transactions_rolled_back")?,
        sum_key(rows, None, "admission_transactions_started")?,
        sum_key(rows, None, "admission_transactions_committed")?,
        sum_key(rows, None, "admission_transactions_rolled_back")?,
        sum_key(rows, None, "admission_statements")?,
        sum_key(rows, None, "integrity_transactions_started")?,
        sum_key(rows, None, "integrity_transactions_committed")?,
        sum_key(rows, None, "integrity_transactions_rolled_back")?,
        sum_key(rows, None, "integrity_statements")?,
        sum_key(rows, None, "retained_roots_validated")?,
        authentication_validation.fetched_authentication_failures,
        authentication_validation.fetched_role_decode_failures,
        authentication_validation.new_object_equation_failures,
        authentication_validation.incumbent_equation_failures,
        authentication_validation.payload_batch_maximum,
        phase_attribution_json(&phase_attribution),
        initial_database,
        terminal_database,
        initial_logical_engine,
        terminal_logical_engine,
        canonical_written,
        database_growth,
        maximum_key(rows, "database_growth_bytes")?,
        if canonical_written == 0 { 0.0 } else { database_growth as f64 / canonical_written as f64 },
        maximum_key(rows, "scratch_high_water_bytes")?,
        storage_by_root_range_json(rows)?,
        rss_peak,
        PRODUCT_BUFFER_BOUND_BYTES,
        q_high_water,
        q_terminal,
        connection_high_water,
        connections_terminal,
        fd_baseline,
        fd_terminal,
        child_peak,
        child_terminal,
        owned_temp_terminal,
        residue_terminal,
        live_rematerializations,
        network_operations,
        by_row_group,
        max_residual,
        residual_sum,
        complete_wall_ns,
        row_wall_sum,
        outside_rows,
        physical_oracles_passed,
        canonical_transitions_passed,
        save_bursts_passed,
        selected_history_roots_passed,
        route_labels_exact,
        terminal_length_exact,
        fixture_unchanged,
        optimization_json(&optimization)?,
        failure_ledger,
        artifacts,
    );
    let summary = apply_disposition(summary, disposition);
    validate_named_wall_equation(&summary)?;
    validate_summary_json_contract(&summary)?;
    Ok(summary)
}
