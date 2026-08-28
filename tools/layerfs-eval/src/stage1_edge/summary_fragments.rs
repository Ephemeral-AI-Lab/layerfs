use super::artifact::sha256_file;
use super::campaign::Campaign;
use super::engine_counters::FixtureMaster;
use super::fixture::SourceIdentity;
use super::markdown_helpers::{
    failure_ledger_json, preserved_failure_ledger, sum_phase, sum_row_walls,
};
use super::row_parse::{phase_wall, ParsedRow};
use super::statistics::{statistics, stats_json};
use crate::stage1_fixture::EvalResult;

pub(crate) struct SummaryFragments {
    pub(crate) artifacts: String,
    pub(crate) by_row_group: String,
    pub(crate) max_residual: u128,
    pub(crate) residual_sum: u128,
    pub(crate) by_root: String,
    pub(crate) admission_wall: u128,
    pub(crate) reset_wall: u128,
    pub(crate) store_open_wall: u128,
    pub(crate) initial_materialization_wall: u128,
    pub(crate) cleanup_wall: u128,
    pub(crate) failure_ledger: String,
}

pub(crate) fn summary_fragments(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    source: &SourceIdentity,
    _master: &FixtureMaster,
    campaign_time_sha256: &str,
) -> EvalResult<SummaryFragments> {
    let artifacts = format!(
        concat!(
            "\"environment_sha256\":\"{}\",\"master_sha256\":\"{}\",",
            "\"readiness_sha256\":\"{}\",\"schedule_sha256\":\"{}\",",
            "\"rows_sha256\":\"{}\",\"rows_line_count\":47,",
            "\"campaign_time_sha256\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\",",
            "\"source_tree_blake3\":\"{}\",",
            "\"source_manifest_sha256\":\"{}\""
        ),
        sha256_file(&campaign.run.join("environment.json"))?,
        sha256_file(&campaign.run.join("master.json"))?,
        sha256_file(&campaign.run.join("readiness.json"))?,
        sha256_file(&campaign.run.join("schedule.json"))?,
        sha256_file(&campaign.run.join("rows.jsonl"))?,
        campaign_time_sha256,
        source.executable_sha256,
        source.executable_blake3,
        source.tree_blake3,
        source.manifest_sha256,
    );
    let by_row_group = ["C00", "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09"]
        .into_iter()
        .map(|group| {
            let values = rows
                .iter()
                .filter(|row| row.row_group == group)
                .map(|row| row.row_residual_ns)
                .collect::<Vec<_>>();
            let maximum = values.iter().copied().max().unwrap_or(0);
            let sum = values.iter().copied().sum::<u128>();
            format!("\"{group}\":{{\"rows\":{},\"maximum_residual_ns\":{maximum},\"sum_residual_ns\":{sum}}}", values.len())
        })
        .collect::<Vec<_>>()
        .join(",");
    let max_residual = rows
        .iter()
        .map(|row| row.row_residual_ns)
        .max()
        .unwrap_or(0);
    let residual_sum = rows.iter().map(|row| row.row_residual_ns).sum::<u128>();
    let by_root = rows
        .iter()
        .filter(|row| row.row_group == "C07")
        .zip(&campaign.schedule.bursts)
        .map(|(row, burst)| -> EvalResult<String> {
            Ok(format!(
                "\"R{}\":{{\"pattern\":\"{}\",\"checkpoint\":{}}}",
                burst.root,
                burst.pattern,
                stats_json(&statistics(vec![phase_wall(
                    &row.json,
                    "durable_checkpoint"
                )?])?)
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    let admission_wall = sum_row_walls(rows, "C00")?;
    let reset_wall = sum_row_walls(rows, "C01")?;
    let store_open_wall = sum_phase(rows, "C02", "store_open")?;
    let initial_materialization_wall = sum_row_walls(rows, "C02")?
        .checked_sub(store_open_wall)
        .ok_or_else(|| "C02 named wall underflow".to_owned())?;
    let cleanup_wall = sum_row_walls(rows, "C09")?;
    let failure_ledger = preserved_failure_ledger(campaign.run)?;
    let failure_ledger = failure_ledger_json(&failure_ledger);
    Ok(SummaryFragments {
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
    })
}
