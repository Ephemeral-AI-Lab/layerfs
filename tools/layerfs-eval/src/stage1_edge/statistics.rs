use super::artifact::json_string;
use super::row_parse::{phase_wall, ParsedRow};
use crate::stage1_fixture::EvalResult;
#[derive(Clone, Debug)]
pub(crate) struct Statistics {
    pub(crate) raw_ns: Vec<u128>,
    pub(crate) sorted_ns: Vec<u128>,
    pub(crate) minimum_ns: u128,
    pub(crate) p50_ns: u128,
    pub(crate) p95_ns: u128,
    pub(crate) maximum_ns: u128,
    pub(crate) range_ns: u128,
    pub(crate) sum_ns: u128,
}
pub(crate) fn statistics(raw: Vec<u128>) -> EvalResult<Statistics> {
    if raw.is_empty() {
        return Err("statistics population is empty".to_owned());
    }
    let mut sorted = raw.clone();
    sorted.sort_unstable();
    let n = sorted.len();
    let p50_index = (n * 50).div_ceil(100).saturating_sub(1);
    let p95_index = (n * 95).div_ceil(100).saturating_sub(1);
    let minimum_ns = sorted[0];
    let maximum_ns = sorted[n - 1];
    Ok(Statistics {
        raw_ns: raw,
        sorted_ns: sorted.clone(),
        minimum_ns,
        p50_ns: sorted[p50_index],
        p95_ns: sorted[p95_index],
        maximum_ns,
        range_ns: maximum_ns - minimum_ns,
        sum_ns: sorted.iter().try_fold(0_u128, |total, value| {
            total
                .checked_add(*value)
                .ok_or_else(|| "statistics sum overflow".to_owned())
        })?,
    })
}
pub(crate) fn stats_json(stats: &Statistics) -> String {
    format!(
        concat!(
            "{{\"n\":{},\"raw_ns\":{},\"sorted_ns\":{},",
            "\"minimum_ns\":{},\"p50_ns\":{},\"p95_ns\":{},",
            "\"maximum_ns\":{},\"range_ns\":{},\"sum_ns\":{}}}"
        ),
        stats.raw_ns.len(),
        u128_array_json(&stats.raw_ns),
        u128_array_json(&stats.sorted_ns),
        stats.minimum_ns,
        stats.p50_ns,
        stats.p95_ns,
        stats.maximum_ns,
        stats.range_ns,
        stats.sum_ns,
    )
}
pub(crate) fn u128_array_json(values: &[u128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(crate) fn row_phase_stats(
    rows: &[ParsedRow],
    group: &str,
    phase: &str,
) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group)
            .map(|row| phase_wall(&row.json, phase))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}
pub(crate) fn filtered_phase_stats(
    rows: &[ParsedRow],
    group: &str,
    phase: &str,
    predicate: impl Fn(&ParsedRow) -> bool,
) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group && predicate(row))
            .map(|row| phase_wall(&row.json, phase))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}
pub(crate) fn combined_phase_stats(
    rows: &[ParsedRow],
    group: &str,
    first: &str,
    second: &str,
    predicate: impl Fn(&ParsedRow) -> bool,
) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group && predicate(row))
            .map(|row| {
                phase_wall(&row.json, first)?
                    .checked_add(phase_wall(&row.json, second)?)
                    .ok_or_else(|| "combined phase wall overflow".to_owned())
            })
            .collect::<EvalResult<Vec<_>>>()?,
    )
}
pub(crate) fn roots_from_rows(rows: &[ParsedRow]) -> EvalResult<Vec<String>> {
    let mut roots = Vec::new();
    let initial = rows
        .iter()
        .find(|row| row.row_group == "C02")
        .ok_or_else(|| "missing C02 root".to_owned())?;
    roots.push(json_string(
        initial
            .json
            .split_once("\"post_ref\":")
            .ok_or_else(|| "C02 missing post_ref".to_owned())?
            .1,
        "root",
    )?);
    for row in rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
    {
        roots.push(json_string(
            row.json
                .split_once("\"post_ref\":")
                .ok_or_else(|| format!("{} missing post_ref", row.row_id))?
                .1,
            "root",
        )?);
    }
    if roots.len() != 35 {
        return Err(format!("retained root count {} != 35", roots.len()));
    }
    Ok(roots)
}
