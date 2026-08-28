use super::artifact::{json_bool, json_u128};
use super::authentication::{validate_authentication, AuthenticationValidation};
use super::campaign::Campaign;
use super::context::Disposition;
use super::engine_counters::FixtureMaster;
use super::fixture::SourceIdentity;
use super::optimization::{optimization_comparison, OptimizationComparison};
use super::report_disposition::{derive_disposition, maximum_key, sum_key};
use super::report_groups::{phase_attributions, PhaseAttribution};
use super::row_parse::{json_all_u128, row_u128, validate_ref_chain, ParsedRow};
use super::statistics::roots_from_rows;
use super::summary_markdown_contract::validate_summary_markdown_contract;
use super::validate_availability::{validate_availability_rows, validate_refresh_rows};
use super::validate_history::validate_history_rows;
use super::validate_locality::{validate_locality_rows, validate_phase_counter_rows};
use crate::stage1_fixture::EvalResult;
pub(crate) struct MarkdownReport<'a, 'campaign> {
    pub(crate) campaign: &'a Campaign<'campaign>,
    pub(crate) rows: &'a [ParsedRow],
    pub(crate) source: &'a SourceIdentity,
    pub(crate) master: &'a FixtureMaster,
    pub(crate) complete_wall_ns: u128,
    pub(crate) disposition: Disposition,
    pub(crate) authentication: AuthenticationValidation,
    pub(crate) selected_history_roots_passed: usize,
    pub(crate) phase_attribution: Vec<PhaseAttribution>,
    pub(crate) optimization: OptimizationComparison,
    pub(crate) roots: Vec<String>,
    pub(crate) rss_peak: u128,
    pub(crate) q_high_water: u128,
    pub(crate) q_terminal: u128,
    pub(crate) connection_high_water: u128,
    pub(crate) connection_terminal: u128,
    pub(crate) fd_baseline: u128,
    pub(crate) fd_terminal: u128,
    pub(crate) child_peak: u128,
    pub(crate) child_terminal: u128,
    pub(crate) owned_temp_terminal: u128,
    pub(crate) residue_terminal: u128,
    pub(crate) rematerializations: u128,
    pub(crate) network_operations: u128,
    pub(crate) physical_oracles: usize,
    pub(crate) canonical_transitions: usize,
    pub(crate) patch_refreshes: usize,
    pub(crate) fallback_refreshes: usize,
    pub(crate) shift_refreshes: usize,
    pub(crate) output: String,
}
impl<'a, 'campaign> MarkdownReport<'a, 'campaign> {
    pub(crate) fn new(
        campaign: &'a Campaign<'campaign>,
        rows: &'a [ParsedRow],
        source: &'a SourceIdentity,
        master: &'a FixtureMaster,
        complete_wall_ns: u128,
    ) -> EvalResult<Self> {
        let disposition = derive_disposition(rows);
        validate_ref_chain(rows, campaign.schedule)?;
        let authentication = validate_authentication(rows)?;
        validate_locality_rows(rows)?;
        validate_phase_counter_rows(rows)?;
        validate_refresh_rows(rows)?;
        validate_availability_rows(rows)?;
        let selected_history_roots_passed = validate_history_rows(rows)?;
        let phase_attribution = phase_attributions(rows)?;
        let optimization = optimization_comparison(rows, complete_wall_ns)?;
        let roots = roots_from_rows(rows)?;
        let c09 = rows
            .iter()
            .find(|row| row.row_group == "C09")
            .ok_or_else(|| "missing C09 terminal row".to_owned())?;
        let rss_peak = maximum_key(rows, "rss_peak_bytes")?;
        let q_high_water = maximum_key(rows, "operation_q_high_water_bytes")?;
        let q_terminal = maximum_key(rows, "operation_q_terminal_bytes")?;
        let connection_high_water = maximum_key(rows, "active_store_connections")?;
        let connection_terminal = row_u128(c09, "active_store_connections")?;
        let fd_baseline = json_u128(&c09.json, "pre_cleanup_fd_count")?;
        let fd_terminal = row_u128(c09, "fd_current")?;
        let child_peak = maximum_key(rows, "child_processes")?;
        let child_terminal = row_u128(c09, "child_processes")?;
        let owned_temp_terminal = row_u128(c09, "owned_temp_entries")?;
        let residue_terminal = row_u128(c09, "residue_entries")?;
        let rematerializations = sum_key(rows, None, "rematerializations")?;
        let network_operations = maximum_key(rows, "network_operations")?;
        let physical_oracles = rows
            .iter()
            .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
            .filter(|row| json_bool(&row.json, "physical_bytes_exact") == Ok(true))
            .count()
            + rows
                .iter()
                .filter(|row| row.row_group == "C07")
                .map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns"))
                .collect::<EvalResult<Vec<_>>>()?
                .iter()
                .map(Vec::len)
                .sum::<usize>();
        let canonical_transitions = rows
            .iter()
            .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
            .filter(|row| json_bool(&row.json, "canonical_bytes_exact") == Ok(true))
            .count();
        let patch_refreshes = rows
            .iter()
            .filter(|row| {
                row.row_group == "C05"
                    && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
            })
            .count();
        let fallback_refreshes = rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "FullFallback")
            .count();
        let shift_refreshes = rows
            .iter()
            .filter(|row| {
                row.row_group == "C05"
                    && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
            })
            .count();
        Ok(Self {
            campaign,
            rows,
            source,
            master,
            complete_wall_ns,
            disposition,
            authentication,
            selected_history_roots_passed,
            phase_attribution,
            optimization,
            roots,
            rss_peak,
            q_high_water,
            q_terminal,
            connection_high_water,
            connection_terminal,
            fd_baseline,
            fd_terminal,
            child_peak,
            child_terminal,
            owned_temp_terminal,
            residue_terminal,
            rematerializations,
            network_operations,
            physical_oracles,
            canonical_transitions,
            patch_refreshes,
            fallback_refreshes,
            shift_refreshes,
            output: String::new(),
        })
    }
    pub(crate) fn finish(mut self) -> EvalResult<String> {
        if self.disposition != Disposition::Pass {
            self.output = self.output.replacen(
                "Disposition: `PASS`",
                &format!("Disposition: `{}`", self.disposition.as_str()),
                1,
            );
            self.output = self.output.replacen(
                "Result: `PASS`",
                &format!("Result: `{}`", self.disposition.as_str()),
                1,
            );
            self.output = self.output.replacen(
            "Reason: All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.",
            "Reason: All hard gates passed; a retained report-only observation requires source review before PASS.",
            1,
        );
        }
        validate_summary_markdown_contract(&self.output)?;
        Ok(self.output)
    }
}
