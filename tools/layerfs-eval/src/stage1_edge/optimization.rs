use super::artifact::{display_error, io_error, json_escape, json_string, json_u128, sha256_file};
use super::authentication::json_array_objects;
use super::markdown_helpers::absolute_path;
use super::row_parse::{json_object, phase_wall, row_u128, ParsedRow};
use super::statistics::{
    filtered_phase_stats, row_phase_stats, statistics, stats_json, Statistics,
};
use crate::stage1_fixture::{self, EvalResult};
use std::fs;
#[cfg(test)]
use std::path::Path;
pub(crate) const OPTIMIZATION_BASELINE: &str = "layerfs-stage1-apple-edge-20260825-attempt-007";
pub(crate) const OPTIMIZATION_BASELINE_ROWS_SHA256: &str =
    "86707e36958b4e46fa2739280e7e4a6038c1fcb7693ee71ef8d7fffdb44b590e";
pub(crate) const OPTIMIZATION_BASELINE_SUMMARY_SHA256: &str =
    "bc5594658fb5a7973c3cfe6e3d648f1a17d95f2f3c1e433680da612e1e9d5888";
#[derive(Clone, Debug)]
pub(crate) struct VerifiedOpenComparison {
    pub(crate) root: &'static str,
    pub(crate) before_ns: u128,
    pub(crate) after_ns: u128,
    pub(crate) retained_union_scrubs: u128,
    pub(crate) namespace_graphs: u128,
    pub(crate) fetched_rows: u128,
    pub(crate) object_bytes_read: u128,
    pub(crate) scratch_tables: u128,
}
#[derive(Clone, Debug)]
pub(crate) struct OptimizationComparison {
    pub(crate) baseline_path: String,
    pub(crate) baseline_complete_wall_ns: u128,
    pub(crate) current_complete_wall_ns: u128,
    pub(crate) baseline_counter_snapshot_ns: u128,
    pub(crate) current_counter_snapshot_ns: u128,
    pub(crate) baseline_history_read_ns: u128,
    pub(crate) current_history_read_ns: u128,
    pub(crate) verified_open: Vec<VerifiedOpenComparison>,
    pub(crate) baseline_append_truncate: Statistics,
    pub(crate) current_append_truncate: Statistics,
    pub(crate) baseline_materialization: Statistics,
    pub(crate) current_materialization: Statistics,
    pub(crate) baseline_clone_shift: usize,
    pub(crate) baseline_in_place_shift: usize,
    pub(crate) current_clone_shift: usize,
    pub(crate) current_in_place_shift: usize,
}
pub(crate) fn optimization_comparison(
    rows: &[ParsedRow],
    current_complete_wall_ns: u128,
) -> EvalResult<OptimizationComparison> {
    let baseline = stage1_fixture::workspace_root()
        .join("target")
        .join(OPTIMIZATION_BASELINE);
    let baseline_rows_path = baseline.join("rows.jsonl");
    let baseline_summary_path = baseline.join("summary.json");
    #[cfg(test)]
    match (baseline_rows_path.exists(), baseline_summary_path.exists()) {
        (false, false) => {
            return synthetic_optimization_comparison(rows, current_complete_wall_ns, &baseline);
        }
        (true, true) => {}
        _ => return Err("incomplete accepted attempt-007 test baseline".to_owned()),
    }
    if sha256_file(&baseline_rows_path)? != OPTIMIZATION_BASELINE_ROWS_SHA256
        || sha256_file(&baseline_summary_path)? != OPTIMIZATION_BASELINE_SUMMARY_SHA256
    {
        return Err("accepted attempt-007 optimization baseline custody".to_owned());
    }
    let baseline_rows = fs::read_to_string(&baseline_rows_path)
        .map_err(io_error)?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if baseline_rows.len() != 47 {
        return Err("accepted attempt-007 baseline rows != 47".to_owned());
    }
    let baseline_summary = fs::read_to_string(&baseline_summary_path).map_err(io_error)?;
    let baseline_complete_wall_ns =
        json_u128(json_object(&baseline_summary, "walls_ns")?, "complete_wall")?;
    let baseline_phase = |row_id: &str, phase: &str| -> EvalResult<u128> {
        baseline_rows
            .iter()
            .find(|row| json_string(row, "row_id").as_deref() == Ok(row_id))
            .ok_or_else(|| format!("baseline missing {row_id}"))
            .and_then(|row| phase_wall(row, phase))
    };
    let current_phase = |row_id: &str, phase: &str| -> EvalResult<u128> {
        rows.iter()
            .find(|row| row.row_id == row_id)
            .ok_or_else(|| format!("current rows missing {row_id}"))
            .and_then(|row| phase_wall(&row.json, phase))
    };
    let baseline_stats =
        |group: &str, phase: &str, operation: Option<&str>| -> EvalResult<Statistics> {
            statistics(
                baseline_rows
                    .iter()
                    .filter(|row| json_string(row, "row_group").as_deref() == Ok(group))
                    .filter(|row| {
                        operation.is_none_or(|expected| {
                            json_string(row, "operation").as_deref() == Ok(expected)
                        })
                    })
                    .map(|row| phase_wall(row, phase))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        };
    let baseline_counter_snapshot_ns = baseline_rows
        .iter()
        .filter(|row| {
            matches!(
                json_string(row, "row_group").as_deref(),
                Ok("C03" | "C05" | "C07")
            )
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(row, "counter_snapshot")?)
                .ok_or_else(|| "baseline counter snapshot wall overflow".to_owned())
        })?;
    let current_counter_snapshot_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "counter_snapshot")?)
                .ok_or_else(|| "current counter snapshot wall overflow".to_owned())
        })?;
    let baseline_history_read_ns = baseline_rows
        .iter()
        .filter(|row| matches!(json_string(row, "row_group").as_deref(), Ok("C04" | "C06")))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(row, "history_read")?)
                .ok_or_else(|| "baseline history wall overflow".to_owned())
        })?;
    let current_history_read_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "history_read")?)
                .ok_or_else(|| "current history wall overflow".to_owned())
        })?;
    let mut baseline_append_truncate =
        baseline_stats("C05", "changed_root_refresh", Some("append"))?.raw_ns;
    baseline_append_truncate
        .extend(baseline_stats("C05", "changed_root_refresh", Some("truncate"))?.raw_ns);
    let mut current_append_truncate =
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "append"
        })?
        .raw_ns;
    current_append_truncate.extend(
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "truncate"
        })?
        .raw_ns,
    );
    let verified_open_rows = [
        ("R5", "C04-001"),
        ("R15", "C04-003"),
        ("R30", "C06-003"),
        ("R34", "C08-001"),
    ];
    let verified_open = verified_open_rows
        .into_iter()
        .map(|(root, row_id)| {
            let row = rows
                .iter()
                .find(|row| row.row_id == row_id)
                .ok_or_else(|| format!("current rows missing {root} scrub"))?;
            let phase = json_array_objects(&row.json, "phase_counters")?
                .into_iter()
                .find(|phase| json_string(phase, "name").as_deref() == Ok("verified_open"))
                .ok_or_else(|| format!("{root} scrub missing verified_open phase counters"))?;
            let retained_union_scrubs = json_u128(phase, "retained_union_scrubs")?;
            let scratch_tables = json_u128(phase, "scratch_tables")?;
            if retained_union_scrubs != 1 || scratch_tables != 2 {
                return Err(format!(
                    "{root} optimization row must be the one-scrub/two-scratch open"
                ));
            }
            Ok(VerifiedOpenComparison {
                root,
                before_ns: baseline_phase(row_id, "verified_open")?,
                after_ns: current_phase(row_id, "verified_open")?,
                retained_union_scrubs,
                namespace_graphs: json_u128(phase, "namespace_graph_verification_passes")?,
                fetched_rows: json_u128(phase, "fetched_rows")?,
                object_bytes_read: json_u128(phase, "object_bytes_read")?,
                scratch_tables,
            })
        })
        .collect::<EvalResult<Vec<_>>>()?;
    Ok(OptimizationComparison {
        baseline_path: absolute_path(&baseline),
        baseline_complete_wall_ns,
        current_complete_wall_ns,
        baseline_counter_snapshot_ns,
        current_counter_snapshot_ns,
        baseline_history_read_ns,
        current_history_read_ns,
        verified_open,
        baseline_append_truncate: statistics(baseline_append_truncate)?,
        current_append_truncate: statistics(current_append_truncate)?,
        baseline_materialization: baseline_stats("C08", "milestone_materialization", None)?,
        current_materialization: row_phase_stats(rows, "C08", "milestone_materialization")?,
        baseline_clone_shift: baseline_rows
            .iter()
            .filter(|row| json_string(row, "row_group").as_deref() == Ok("C05"))
            .filter(|row| json_string(row, "native_route").as_deref() == Ok("CloneShift"))
            .count(),
        baseline_in_place_shift: baseline_rows
            .iter()
            .filter(|row| json_string(row, "row_group").as_deref() == Ok("C05"))
            .filter(|row| json_string(row, "native_route").as_deref() == Ok("InPlaceShift"))
            .count(),
        current_clone_shift: rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
            .count(),
        current_in_place_shift: rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
            .count(),
    })
}
#[cfg(test)]
pub(crate) fn synthetic_optimization_comparison(
    rows: &[ParsedRow],
    current_complete_wall_ns: u128,
    baseline: &Path,
) -> EvalResult<OptimizationComparison> {
    let current_counter_snapshot_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "counter_snapshot")?)
                .ok_or_else(|| "synthetic counter snapshot wall overflow".to_owned())
        })?;
    let current_history_read_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "history_read")?)
                .ok_or_else(|| "synthetic history wall overflow".to_owned())
        })?;
    let mut append_truncate = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
        row.operation == "append"
    })?
    .raw_ns;
    append_truncate.extend(
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "truncate"
        })?
        .raw_ns,
    );
    let current_append_truncate = statistics(append_truncate)?;
    let current_materialization = row_phase_stats(rows, "C08", "milestone_materialization")?;
    let verified_open = [
        ("R5", "C04-001"),
        ("R15", "C04-003"),
        ("R30", "C06-003"),
        ("R34", "C08-001"),
    ]
    .into_iter()
    .map(|(root, row_id)| {
        let row = rows
            .iter()
            .find(|row| row.row_id == row_id)
            .ok_or_else(|| format!("synthetic rows missing {row_id}"))?;
        let phase = json_array_objects(&row.json, "phase_counters")?
            .into_iter()
            .find(|phase| json_string(phase, "name").as_deref() == Ok("verified_open"))
            .ok_or_else(|| format!("synthetic rows missing {row_id} verified_open"))?;
        let retained_union_scrubs = json_u128(phase, "retained_union_scrubs")?;
        let scratch_tables = json_u128(phase, "scratch_tables")?;
        if retained_union_scrubs != 1 || scratch_tables != 2 {
            return Err(format!(
                "{root} synthetic optimization row must be the one-scrub/two-scratch open"
            ));
        }
        let after_ns = phase_wall(&row.json, "verified_open")?;
        Ok(VerifiedOpenComparison {
            root,
            before_ns: if root == "R34" {
                1_406_344_708
            } else {
                after_ns
            },
            after_ns,
            retained_union_scrubs,
            namespace_graphs: json_u128(phase, "namespace_graph_verification_passes")?,
            fetched_rows: json_u128(phase, "fetched_rows")?,
            object_bytes_read: json_u128(phase, "object_bytes_read")?,
            scratch_tables,
        })
    })
    .collect::<EvalResult<Vec<_>>>()?;
    let current_clone_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
        .count();
    let current_in_place_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
        .count();
    Ok(OptimizationComparison {
        baseline_path: absolute_path(baseline),
        baseline_complete_wall_ns: current_complete_wall_ns,
        current_complete_wall_ns,
        baseline_counter_snapshot_ns: current_counter_snapshot_ns,
        current_counter_snapshot_ns,
        baseline_history_read_ns: current_history_read_ns,
        current_history_read_ns,
        verified_open,
        baseline_append_truncate: current_append_truncate.clone(),
        current_append_truncate,
        baseline_materialization: current_materialization.clone(),
        current_materialization,
        baseline_clone_shift: current_clone_shift,
        baseline_in_place_shift: current_in_place_shift,
        current_clone_shift,
        current_in_place_shift,
    })
}
pub(crate) fn signed_gain(before: u128, after: u128) -> EvalResult<i128> {
    let before = i128::try_from(before).map_err(display_error)?;
    let after = i128::try_from(after).map_err(display_error)?;
    before
        .checked_sub(after)
        .ok_or_else(|| "optimization gain overflow".to_owned())
}
pub(crate) fn optimization_json(value: &OptimizationComparison) -> EvalResult<String> {
    let verified = value
        .verified_open
        .iter()
        .map(|receipt| -> EvalResult<String> {
            Ok(format!(
                concat!(
                    "\"{}\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{},",
                    "\"retained_union_scrubs\":{},\"namespace_graphs\":{},",
                    "\"fetched_rows\":{},\"object_bytes_read\":{},\"scratch_tables\":{}}}"
                ),
                receipt.root,
                receipt.before_ns,
                receipt.after_ns,
                signed_gain(receipt.before_ns, receipt.after_ns)?,
                receipt.retained_union_scrubs,
                receipt.namespace_graphs,
                receipt.fetched_rows,
                receipt.object_bytes_read,
                receipt.scratch_tables,
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    Ok(format!(
        concat!(
            "{{\"baseline_run\":\"{}\",\"baseline_rows_sha256\":\"{}\",",
            "\"baseline_summary_sha256\":\"{}\",",
            "\"complete_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"counter_snapshot_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"history_read_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"verified_open_by_root\":{{{}}},",
            "\"append_truncate_refresh\":{{\"before\":{},\"after\":{}}},",
            "\"milestone_materialization\":{{\"before\":{},\"after\":{}}},",
            "\"shift_routes\":{{\"before_clone\":{},\"before_in_place\":{},",
            "\"after_clone\":{},\"after_in_place\":{}}}}}"
        ),
        json_escape(&value.baseline_path),
        OPTIMIZATION_BASELINE_ROWS_SHA256,
        OPTIMIZATION_BASELINE_SUMMARY_SHA256,
        value.baseline_complete_wall_ns,
        value.current_complete_wall_ns,
        signed_gain(
            value.baseline_complete_wall_ns,
            value.current_complete_wall_ns
        )?,
        value.baseline_counter_snapshot_ns,
        value.current_counter_snapshot_ns,
        signed_gain(
            value.baseline_counter_snapshot_ns,
            value.current_counter_snapshot_ns,
        )?,
        value.baseline_history_read_ns,
        value.current_history_read_ns,
        signed_gain(
            value.baseline_history_read_ns,
            value.current_history_read_ns
        )?,
        verified,
        stats_json(&value.baseline_append_truncate),
        stats_json(&value.current_append_truncate),
        stats_json(&value.baseline_materialization),
        stats_json(&value.current_materialization),
        value.baseline_clone_shift,
        value.baseline_in_place_shift,
        value.current_clone_shift,
        value.current_in_place_shift,
    ))
}
pub(crate) fn first_group_value(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .find(|row| row.row_group == group)
        .ok_or_else(|| format!("missing row group {group}"))
        .and_then(|row| row_u128(row, key))
}
pub(crate) fn last_group_value(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .rev()
        .find(|row| row.row_group == group)
        .ok_or_else(|| format!("missing row group {group}"))
        .and_then(|row| row_u128(row, key))
}
