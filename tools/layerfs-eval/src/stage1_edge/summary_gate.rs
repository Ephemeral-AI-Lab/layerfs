use super::artifact::{json_bool, json_u128};
use super::authentication::{validate_authentication, AuthenticationValidation};
use super::campaign::Campaign;
use super::limits::{INITIAL_BYTES, REPLACEMENT_BACKING_BYTES};
use super::optimization::{first_group_value, last_group_value};
use super::report_disposition::{maximum_key, sum_key};
use super::row_parse::{json_all_u128, json_object, row_u128, validate_ref_chain, ParsedRow};
use super::validate_availability::{validate_availability_rows, validate_refresh_rows};
use super::validate_history::validate_history_rows;
use super::validate_locality::{validate_locality_rows, validate_phase_counter_rows};
use crate::stage1_fixture::EvalResult;
pub(crate) struct SummaryGateFacts {
    pub(crate) authentication: AuthenticationValidation,
    pub(crate) selected_history_roots_passed: usize,
    pub(crate) row_wall_sum: u128,
    pub(crate) outside_rows: u128,
    pub(crate) cdc_physical: u128,
    pub(crate) cdc_logical: u128,
    pub(crate) cdc_bursts: u128,
    pub(crate) cdc_total: u128,
    pub(crate) transactions: u128,
    pub(crate) commits: u128,
    pub(crate) rollbacks: u128,
    pub(crate) publications: u128,
    pub(crate) initial_database: u128,
    pub(crate) terminal_database: u128,
    pub(crate) database_growth: u128,
    pub(crate) canonical_written: u128,
    pub(crate) initial_logical_engine: u128,
    pub(crate) terminal_logical_engine: u128,
    pub(crate) rss_peak: u128,
    pub(crate) q_high_water: u128,
    pub(crate) q_terminal: u128,
    pub(crate) connection_high_water: u128,
    pub(crate) connections_terminal: u128,
    pub(crate) fd_baseline: u128,
    pub(crate) fd_terminal: u128,
    pub(crate) child_peak: u128,
    pub(crate) child_terminal: u128,
    pub(crate) owned_temp_terminal: u128,
    pub(crate) residue_terminal: u128,
    pub(crate) network_operations: u128,
    pub(crate) live_rematerializations: u128,
    pub(crate) physical_oracles_passed: usize,
    pub(crate) canonical_transitions_passed: usize,
    pub(crate) save_bursts_passed: usize,
    pub(crate) observed_edit_suboperations: usize,
    pub(crate) transition_rows: usize,
    pub(crate) burst_suboperations: usize,
    pub(crate) history_session_count: usize,
    pub(crate) witness_materializations: usize,
    pub(crate) live_workspace_materializations: u128,
    pub(crate) workspace_reuses: u128,
    pub(crate) route_labels_exact: bool,
    pub(crate) terminal_length_exact: bool,
    pub(crate) fixture_unchanged: bool,
}
pub(crate) fn validate_summary_gates(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    complete_wall_ns: u128,
) -> EvalResult<SummaryGateFacts> {
    validate_ref_chain(rows, campaign.schedule)?;
    let authentication = validate_authentication(rows)?;
    validate_locality_rows(rows)?;
    validate_phase_counter_rows(rows)?;
    validate_refresh_rows(rows)?;
    validate_availability_rows(rows)?;
    let selected_history_roots_passed = validate_history_rows(rows)?;
    let burst_oracles = rows
        .iter()
        .filter(|row| row.row_group == "C07")
        .map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns"))
        .collect::<EvalResult<Vec<_>>>()?;
    if burst_oracles.iter().map(Vec::len).sum::<usize>() != 21 {
        return Err("burst physical-oracle population != 21".to_owned());
    }
    let row_wall_sum = rows.iter().try_fold(0_u128, |total, row| {
        total
            .checked_add(row.row_wall_ns)
            .ok_or_else(|| "row wall sum overflow".to_owned())
    })?;
    if row_wall_sum != campaign.row_wall_sum_ns {
        return Err("summary row wall sum derives exactly from rows.jsonl".to_owned());
    }
    let outside_rows = complete_wall_ns
        .checked_sub(row_wall_sum)
        .ok_or_else(|| "complete wall below row wall sum".to_owned())?;
    let cdc_physical = sum_key(rows, Some("C03"), "cdc_bytes_scanned")?;
    let cdc_logical = sum_key(rows, Some("C05"), "cdc_bytes_scanned")?;
    let cdc_bursts = sum_key(rows, Some("C07"), "cdc_bytes_scanned")?;
    let cdc_total = cdc_physical + cdc_logical + cdc_bursts;
    if cdc_total != REPLACEMENT_BACKING_BYTES as u128 {
        return Err("canonical CDC total = 495,616".to_owned());
    }
    let transactions = sum_key(rows, None, "transactions_started")?;
    let commits = sum_key(rows, None, "transactions_committed")?;
    let rollbacks = sum_key(rows, None, "transactions_rolled_back")?;
    let publications = sum_key(rows, None, "publication_commits")?;
    if (transactions, commits, rollbacks, publications) != (34, 34, 0, 34) {
        return Err("summary transaction/COMMIT closure".to_owned());
    }
    let initial_database = row_u128(
        rows.iter()
            .find(|row| row.row_group == "C02")
            .ok_or_else(|| "missing C02 storage observation".to_owned())?,
        "database_bytes",
    )?;
    let terminal_database = row_u128(
        rows.iter()
            .rev()
            .find(|row| row.row_group == "C07")
            .ok_or_else(|| "missing terminal C07 storage observation".to_owned())?,
        "database_bytes",
    )?;
    let database_growth = terminal_database.saturating_sub(initial_database);
    let canonical_written = sum_key(rows, None, "canonical_object_bytes_written")?;
    let initial_logical_engine = first_group_value(rows, "C02", "logical_engine_bytes")?;
    let terminal_logical_engine = last_group_value(rows, "C07", "logical_engine_bytes")?;
    if terminal_database < initial_database || terminal_logical_engine < initial_logical_engine {
        return Err("summary storage monotonicity".to_owned());
    }
    let rss_peak = maximum_key(rows, "rss_peak_bytes")?;
    let q_high_water = maximum_key(rows, "operation_q_high_water_bytes")?;
    let q_terminal = maximum_key(rows, "operation_q_terminal_bytes")?;
    let connection_high_water = maximum_key(rows, "active_store_connections")?;
    let c09 = rows
        .iter()
        .find(|row| row.row_group == "C09")
        .ok_or_else(|| "missing C09 terminal row".to_owned())?;
    let connections_terminal = row_u128(c09, "active_store_connections")?;
    let fd_terminal = row_u128(c09, "fd_current")?;
    let child_peak = maximum_key(rows, "child_processes")?;
    let child_terminal = row_u128(c09, "child_processes")?;
    let owned_temp_terminal = row_u128(c09, "owned_temp_entries")?;
    let residue_terminal = row_u128(c09, "residue_entries")?;
    let network_operations = maximum_key(rows, "network_operations")?;
    let live_rematerializations = sum_key(rows, None, "rematerializations")?;
    let pre_cleanup_residue = json_u128(&c09.json, "pre_cleanup_residue_entries")?;
    let pre_cleanup_connections = json_u128(&c09.json, "pre_cleanup_active_store_connections")?;
    let fd_baseline = json_u128(&c09.json, "pre_cleanup_fd_count")?;
    if rss_peak > 33_554_432
        || q_high_water > 8_388_608
        || q_terminal != 0
        || connection_high_water > 2
        || connections_terminal != 0
        || fd_terminal != fd_baseline
        || child_peak != 0
        || child_terminal != 0
        || owned_temp_terminal != 0
        || residue_terminal != 0
        || pre_cleanup_residue != 0
        || pre_cleanup_connections != 0
        || network_operations != 0
        || live_rematerializations != 0
    {
        return Err("rows-derived summary resource closure".to_owned());
    }
    let physical_oracles_passed = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .filter(|row| json_bool(&row.json, "physical_bytes_exact") == Ok(true))
        .count()
        + burst_oracles.iter().map(Vec::len).sum::<usize>();
    let transition_rows = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    let burst_suboperations = burst_oracles.iter().map(Vec::len).sum::<usize>();
    let observed_edit_suboperations = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .count()
        + burst_suboperations;
    let history_session_count = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .count();
    let witness_materializations = rows.iter().filter(|row| row.row_group == "C08").count();
    let live_workspace_materializations = sum_key(rows, None, "workspace_materializations")?;
    let workspace_reuses = sum_key(rows, None, "workspace_reuses")?;
    let canonical_transitions_passed = transition_rows
        .iter()
        .filter(|row| json_bool(&row.json, "canonical_bytes_exact") == Ok(true))
        .count();
    let route_labels_exact = transition_rows
        .iter()
        .all(|row| json_bool(&row.json, "route_exact") == Ok(true));
    let save_bursts_passed = rows
        .iter()
        .filter(|row| row.row_group == "C07" && row.status == "PASS")
        .count();
    let r34 = rows
        .iter()
        .find(|row| row.row_id == "C08-003")
        .ok_or_else(|| "missing R34 terminal witness".to_owned())?;
    let r34_oracle = json_object(&r34.json, "oracle")?;
    let terminal_length_exact = json_u128(r34_oracle, "logical_length")?
        == u128::from(INITIAL_BYTES)
        && json_bool(r34_oracle, "physical_bytes_exact")?
        && json_bool(r34_oracle, "canonical_bytes_exact")?;
    let fixture_unchanged = json_bool(&c09.json, "fixture_unchanged")?;
    if physical_oracles_passed != 51
        || canonical_transitions_passed != 34
        || !route_labels_exact
        || save_bursts_passed != 4
        || selected_history_roots_passed != 8
        || !terminal_length_exact
        || !fixture_unchanged
        || rows.len() != 47
        || observed_edit_suboperations != 51
        || transition_rows.len() != 34
        || burst_suboperations != 21
        || history_session_count != 6
        || witness_materializations != 3
        || live_workspace_materializations != 1
        || workspace_reuses != 34
    {
        return Err("rows-derived summary correctness closure".to_owned());
    }
    Ok(SummaryGateFacts {
        authentication,
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
        transition_rows: transition_rows.len(),
        burst_suboperations,
        history_session_count,
        witness_materializations,
        live_workspace_materializations,
        workspace_reuses,
        route_labels_exact,
        terminal_length_exact,
        fixture_unchanged,
    })
}
