use super::artifact::{json_bool, json_string, json_u128};
use super::authentication::json_array_objects;
use super::row_parse::{json_object, phase_wall, row_u128, ParsedRow};
use super::statistics::{
    combined_phase_stats, filtered_phase_stats, statistics, stats_json, Statistics,
};
use super::summary_markdown_contract::range_sum;
use crate::stage1_fixture::EvalResult;
pub(crate) fn physical_by_kind_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.operation == kind)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.operation == kind
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.operation == kind
            })?;
        values.push(format!(
            "\"{kind}\":{{\"native_edit\":{},\"durable_checkpoint\":{},\"edit_plus_checkpoint\":{}}}",
            stats_json(&native),
            stats_json(&checkpoint),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn physical_by_size_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for band in ["near-8-kib", "near-16-kib", "near-32-kib"] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.size_band == band)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.size_band == band
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.size_band == band
            })?;
        values.push(format!(
            "\"{band}\":{{\"native_edit\":{},\"durable_checkpoint\":{},\"edit_plus_checkpoint\":{}}}",
            stats_json(&native),
            stats_json(&checkpoint),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn logical_by_kind_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
            row.operation == kind
        })?;
        let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == kind
        })?;
        let combined = combined_phase_stats(
            rows,
            "C05",
            "direct_logical_edit",
            "changed_root_refresh",
            |row| row.operation == kind,
        )?;
        values.push(format!(
            "\"{kind}\":{{\"direct_logical_edit\":{},\"changed_root_refresh\":{},\"logical_edit_plus_refresh\":{}}}",
            stats_json(&logical),
            stats_json(&refresh),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn logical_by_size_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for band in ["near-8-kib", "near-16-kib", "near-32-kib"] {
        let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
            row.size_band == band
        })?;
        let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.size_band == band
        })?;
        let combined = combined_phase_stats(
            rows,
            "C05",
            "direct_logical_edit",
            "changed_root_refresh",
            |row| row.size_band == band,
        )?;
        values.push(format!(
            "\"{band}\":{{\"direct_logical_edit\":{},\"changed_root_refresh\":{},\"logical_edit_plus_refresh\":{}}}",
            stats_json(&logical),
            stats_json(&refresh),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn route_stats(
    rows: &[ParsedRow],
    route: &str,
    operation: Option<&str>,
) -> EvalResult<Statistics> {
    filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
        (route == "Patch" && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
            || route == "Shift"
                && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
            || row.native_route == route)
            && operation.is_none_or(|operation| row.operation == operation)
    })
}
pub(crate) fn root_json(roots: &[String]) -> String {
    [0_usize, 5, 10, 15, 20, 25, 30, 31, 32, 33, 34]
        .into_iter()
        .map(|index| format!("\"R{index}\":\"{}\"", roots[index]))
        .collect::<Vec<_>>()
        .join(",")
}
pub(crate) fn count_change_amplification_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["insert", "delete", "append", "truncate"] {
        let selected = rows
            .iter()
            .filter(|row| row.row_group == "C03" && row.operation == kind)
            .collect::<Vec<_>>();
        let suffix = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "suffix_bytes_shifted")?)
                .ok_or_else(|| "suffix amplification sum overflow".to_owned())
        })?;
        let read = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_read")?)
                .ok_or_else(|| "native read amplification sum overflow".to_owned())
        })?;
        let written = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "native write amplification sum overflow".to_owned())
        })?;
        let logical_change = selected
            .iter()
            .map(|row| u128::from(row.before_bytes.abs_diff(row.after_bytes)))
            .sum::<u128>();
        let amplification = if logical_change == 0 {
            0.0
        } else {
            (read + written) as f64 / logical_change as f64
        };
        values.push(format!(
            "\"{kind}\":{{\"n\":{},\"suffix_bytes_shifted\":{suffix},\"native_bytes_read\":{read},\"native_bytes_written\":{written},\"logical_change_bytes\":{logical_change},\"amplification\":{amplification:.9}}}",
            selected.len()
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn materialization_by_root_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for (index, root) in [15_u8, 30, 34].into_iter().enumerate() {
        let row = rows
            .iter()
            .filter(|row| row.row_group == "C08")
            .nth(index)
            .ok_or_else(|| format!("missing C08 materialization R{root}"))?;
        let wall = phase_wall(&row.json, "milestone_materialization")?;
        let oracle = json_object(&row.json, "oracle")?;
        let custody = json_object(&row.json, "custody")?;
        let exact_bytes = json_bool(oracle, "physical_bytes_exact")?
            && json_bool(oracle, "canonical_bytes_exact")?;
        let metadata_exact = json_bool(oracle, "metadata_exact")?;
        let extra_user_files = json_u128(custody, "extra_user_files")?;
        let cleanup_exact = json_u128(custody, "cleanup_residue_entries")? == 0;
        let metadata_receipt = json_object(custody, "fresh_metadata")?;
        values.push(format!(
            "\"R{root}\":{{\"logical_bytes\":{},\"wall\":{},\"native_bytes_written\":{},\"exact_bytes\":{exact_bytes},\"metadata_exact\":{metadata_exact},\"metadata\":{metadata_receipt},\"extra_user_files\":{extra_user_files},\"cleanup_exact\":{cleanup_exact}}}",
            row.after_bytes,
            stats_json(&statistics(vec![wall])?),
            row_u128(row, "bytes_written")?,
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn storage_by_root_range_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for (label, group, transitions) in [
        ("R0_to_R15", "C03", 15),
        ("R15_to_R30", "C05", 15),
        ("R30_to_R34", "C07", 4),
    ] {
        let canonical = range_sum(rows, group, "canonical_object_bytes_written")?;
        let database = range_sum(rows, group, "database_growth_bytes")?;
        let amplification = if canonical == 0 {
            0.0
        } else {
            database as f64 / canonical as f64
        };
        values.push(format!(
            "\"{label}\":{{\"transitions\":{transitions},\"canonical_bytes_written\":{canonical},\"database_growth_bytes\":{database},\"amplification\":{amplification:.9}}}"
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}
#[derive(Clone, Debug, Default)]
pub(crate) struct PhaseAttribution {
    pub(crate) name: &'static str,
    pub(crate) rows: usize,
    pub(crate) statements: u128,
    pub(crate) fetched_rows: u128,
    pub(crate) authentication_passes: u128,
    pub(crate) role_decode_passes: u128,
    pub(crate) object_bytes_read: u128,
    pub(crate) object_bytes_written: u128,
    pub(crate) transactions: u128,
    pub(crate) commits: u128,
    pub(crate) publication_commits: u128,
    pub(crate) retained_union_scrubs: u128,
    pub(crate) scratch_tables: u128,
    pub(crate) operation_scratch_tables: u128,
    pub(crate) q_high_water_bytes: u128,
    pub(crate) active_connections: u128,
}
pub(crate) fn phase_attributions(rows: &[ParsedRow]) -> EvalResult<Vec<PhaseAttribution>> {
    let mut output = Vec::new();
    for name in [
        "store_open",
        "materialization",
        "checkpoint",
        "logical_edit",
        "apfs_refresh",
        "canonical_witness",
        "verified_open",
        "history_read",
        "storage_observation",
    ] {
        let phases = rows
            .iter()
            .map(|row| json_array_objects(&row.json, "phase_counters"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|phase| json_string(phase, "name").as_deref() == Ok(name))
            .collect::<Vec<_>>();
        if phases.is_empty() {
            return Err(format!("phase attribution {name} is empty"));
        }
        let sum = |key: &str| -> EvalResult<u128> {
            phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, key)?)
                    .ok_or_else(|| format!("phase attribution {name}.{key} overflow"))
            })
        };
        let maximum = |key: &str| -> EvalResult<u128> {
            phases
                .iter()
                .map(|phase| json_u128(phase, key))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .max()
                .ok_or_else(|| format!("phase attribution {name}.{key} maximum"))
        };
        output.push(PhaseAttribution {
            name,
            rows: phases.len(),
            statements: sum("statements")?,
            fetched_rows: sum("fetched_rows")?,
            authentication_passes: sum("fetched_row_authentication_passes")?,
            role_decode_passes: sum("fetched_row_role_decode_passes")?,
            object_bytes_read: sum("object_bytes_read")?,
            object_bytes_written: sum("object_bytes_written")?,
            transactions: sum("transactions_started")?,
            commits: sum("transactions_committed")?,
            publication_commits: sum("publication_commits")?,
            retained_union_scrubs: sum("retained_union_scrubs")?,
            scratch_tables: sum("scratch_tables")?,
            operation_scratch_tables: sum("operation_scratch_tables")?,
            q_high_water_bytes: maximum("q_high_water_bytes")?,
            active_connections: maximum("active_connections")?,
        });
    }
    Ok(output)
}
pub(crate) fn phase_attribution_json(values: &[PhaseAttribution]) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|value| format!(
                concat!(
                    "\"{}\":{{\"rows\":{},\"statements\":{},\"fetched_rows\":{},",
                    "\"authentication_passes\":{},\"role_decode_passes\":{},",
                    "\"object_bytes_read\":{},\"object_bytes_written\":{},",
                    "\"transactions\":{},\"commits\":{},\"publication_commits\":{},",
                    "\"retained_union_scrubs\":{},\"scratch_tables\":{},",
                    "\"operation_scratch_tables\":{},",
                    "\"q_high_water_bytes\":{},\"active_connections\":{}}}"
                ),
                value.name,
                value.rows,
                value.statements,
                value.fetched_rows,
                value.authentication_passes,
                value.role_decode_passes,
                value.object_bytes_read,
                value.object_bytes_written,
                value.transactions,
                value.commits,
                value.publication_commits,
                value.retained_union_scrubs,
                value.scratch_tables,
                value.operation_scratch_tables,
                value.q_high_water_bytes,
                value.active_connections,
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(crate) fn history_probe_stats(rows: &[ParsedRow], ordinal: u8) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
            .map(|row| json_array_objects(&row.json, "history_probes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|probe| json_u128(probe, "ordinal") == Ok(u128::from(ordinal)))
            .map(|probe| json_u128(probe, "wall_ns"))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}
pub(crate) fn history_probe_sum(rows: &[ParsedRow], ordinal: u8, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .map(|row| json_array_objects(&row.json, "history_probes"))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .filter(|probe| json_u128(probe, "ordinal") == Ok(u128::from(ordinal)))
        .try_fold(0_u128, |total, probe| {
            total
                .checked_add(json_u128(probe, key)?)
                .ok_or_else(|| format!("history probe {key} sum overflow"))
        })
}
