use super::artifact::{json_escape, json_u128_array};
use super::model::{CampaignData, CAMPAIGN_LIMIT_NS, MIB, RESET_COUNT};
use super::operation_evidence::observed_u64_json;
use super::resource_evidence::Statistics;
use crate::stage1_fixture::{EvalResult, FILE_BYTES};
use std::collections::BTreeMap;
pub(crate) fn process_resource_summary_json(data: &CampaignData) -> String {
    let observations = data
        .process_resources
        .iter()
        .enumerate()
        .map(|(sequence, value)| {
            let crossed = if value.observed {
                (value.process_peak_rss_bytes > 67_108_864).to_string()
            } else {
                "\"Unavailable\"".to_owned()
            };
            format!(
                "{{\"sequence\":{sequence},\"operation\":\"{}\",\"observed\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"crossed_64_mib\":{crossed}}}",
                json_escape(&value.operation),
                value.observed,
                observed_u64_json(value.observed, value.current_rss_bytes),
                observed_u64_json(value.observed, value.process_peak_rss_bytes),
            )
        })
        .collect::<Vec<_>>();
    let first_crossing = data
        .process_resources
        .iter()
        .enumerate()
        .find(|(_, value)| value.observed && value.process_peak_rss_bytes > 67_108_864)
        .map_or_else(
            || "null".to_owned(),
            |(sequence, value)| {
                format!(
                    "{{\"sequence\":{sequence},\"operation\":\"{}\",\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{}}}",
                    json_escape(&value.operation),
                    value.current_rss_bytes,
                    value.process_peak_rss_bytes,
                )
            },
        );
    format!(
        "{{\"observations\":[{}],\"first_64_mib_crossing\":{first_crossing}}}",
        observations.join(",")
    )
}
pub(crate) fn summary_markdown(data: &CampaignData, wall: u128) -> EvalResult<String> {
    validate_metric_populations(data)?;
    let disposition = performance_disposition(data, wall)?;
    let mut output = format!(
        "# LayerFS Stage One Part 1\n\nDisposition: {disposition}.\n\n- Complete wall: {wall} ns\n- Store resets: {} / {RESET_COUNT}\n- Maximum user file: {FILE_BYTES} bytes\n- Store database maximum (separate authority): {} bytes\n\n| Metric | n | min ns | p50 ns | p95 ns | max ns | throughput MiB/s | target |\n|---|---:|---:|---:|---:|---:|---:|---|\n",
        data.reset_count,
        data.store_database_bytes_max
            .map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
    );
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        let throughput = data.bytes_per_observation.get(name).map_or_else(
            || "N/A".to_owned(),
            |bytes| format!("{:.3}", throughput_mib_s(*bytes, stats.p50)),
        );
        output.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {} | {throughput} | {} |\n",
            stats.sorted.len(),
            stats.minimum,
            stats.p50,
            stats.p95,
            stats.maximum,
            target_label(name, &stats, data.bytes_per_observation.get(name).copied()),
        ));
    }
    Ok(output)
}
pub(crate) fn statistics(observations: &[u128]) -> EvalResult<Statistics> {
    if observations.is_empty() {
        return Err("statistics population is empty".to_owned());
    }
    let mut sorted = observations.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let p50 = if len % 2 == 1 {
        sorted[len / 2]
    } else {
        sorted[len / 2 - 1]
            .checked_add(sorted[len / 2])
            .ok_or_else(|| "p50 overflow".to_owned())?
            / 2
    };
    let p95_rank = (95 * len).div_ceil(100).max(1);
    let minimum = sorted[0];
    let maximum = sorted[len - 1];
    let p95 = sorted[p95_rank - 1];
    let operation_wall = sorted.iter().try_fold(0_u128, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| "statistics wall overflow".to_owned())
    })?;
    Ok(Statistics {
        sorted,
        minimum,
        maximum,
        range: maximum - minimum,
        p50,
        p95,
        operation_wall,
    })
}
pub(crate) fn statistics_json(name: &str, value: &Statistics, bytes: Option<u64>) -> String {
    let raw = json_u128_array(&value.sorted);
    let throughput = bytes.map_or_else(
        || "null".to_owned(),
        |bytes| format!("{:.6}", throughput_mib_s(bytes, value.p50)),
    );
    let aggregate = bytes.map_or_else(
        || "null".to_owned(),
        |bytes| {
            let total = u128::from(bytes) * value.sorted.len() as u128;
            format!("{:.6}", throughput_mib_s_u128(total, value.operation_wall))
        },
    );
    format!(
        "{{\"raw_sorted_ns\":{raw},\"minimum_ns\":{},\"maximum_ns\":{},\"range_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"operation_population_wall_ns\":{},\"bytes_per_observation\":{},\"p50_throughput_mib_s\":{throughput},\"aggregate_throughput_mib_s\":{aggregate},\"target\":{}}}",
        value.minimum,
        value.maximum,
        value.range,
        value.p50,
        value.p95,
        value.operation_wall,
        bytes.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        target_json_for_metric(name, value, bytes),
    )
}
pub(crate) fn throughput_mib_s(bytes: u64, nanoseconds: u128) -> f64 {
    throughput_mib_s_u128(u128::from(bytes), nanoseconds)
}
pub(crate) fn throughput_mib_s_u128(bytes: u128, nanoseconds: u128) -> f64 {
    if nanoseconds == 0 {
        return f64::MAX;
    }
    bytes as f64 / MIB as f64 / (nanoseconds as f64 / 1_000_000_000.0)
}
pub(crate) fn target_json_for_metric(name: &str, stats: &Statistics, bytes: Option<u64>) -> String {
    format!("\"{}\"", json_escape(&target_label(name, stats, bytes)))
}
pub(crate) fn target_label(name: &str, stats: &Statistics, bytes: Option<u64>) -> String {
    let throughput = bytes.map(|bytes| throughput_mib_s(bytes, stats.p50));
    let (description, pass) = match name {
        "A01" => (
            ">=250 MiB/s",
            throughput.is_some_and(|value| value >= 250.0),
        ),
        "A02" => (
            "p50<=0.5ms and p95<=1.0ms",
            stats.p50 <= 500_000 && stats.p95 <= 1_000_000,
        ),
        "A03a" | "A03b" => (
            ">=150 MiB/s",
            throughput.is_some_and(|value| value >= 150.0),
        ),
        "A04/logical" => ("p50<=15ms", stats.p50 <= 15_000_000),
        "A04/native-edit-plus-checkpoint" => ("p50<=20ms", stats.p50 <= 20_000_000),
        "A09" => (
            ">=200 MiB/s",
            throughput.is_some_and(|value| value >= 200.0),
        ),
        "A10" => (
            ">=150 MiB/s",
            throughput.is_some_and(|value| value >= 150.0),
        ),
        "A11" => ("p50<=5ms", stats.p50 <= 5_000_000),
        "A12" => ("p50<=25ms", stats.p50 <= 25_000_000),
        "A13" => ("p50<=4ms", stats.p50 <= 4_000_000),
        _ => return "REPORT_ONLY".to_owned(),
    };
    format!("{} ({description})", if pass { "PASS" } else { "REVISE" })
}
pub(crate) fn target_json(data: &CampaignData, wall: u128) -> EvalResult<String> {
    let mut values = Vec::new();
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        let label = target_label(name, &stats, data.bytes_per_observation.get(name).copied());
        if label != "REPORT_ONLY" {
            values.push(format!("\"{}\":\"{}\"", json_escape(name), label));
        }
    }
    values.push(format!(
        "\"complete_campaign\":\"{} (preferred<60s hard<=120s)\"",
        if wall <= CAMPAIGN_LIMIT_NS {
            if wall < 60_000_000_000 {
                "PASS"
            } else {
                "PASS_HARD_REVISE_PREFERRED"
            }
        } else {
            "FAIL_HARD"
        }
    ));
    Ok(format!("{{{}}}", values.join(",")))
}
pub(crate) fn performance_disposition(data: &CampaignData, wall: u128) -> EvalResult<String> {
    if wall >= 60_000_000_000 {
        return Ok("REVISE".to_owned());
    }
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        if target_label(name, &stats, data.bytes_per_observation.get(name).copied())
            .starts_with("REVISE")
        {
            return Ok("REVISE".to_owned());
        }
    }
    Ok("PASS".to_owned())
}
pub(crate) fn validate_metric_populations(data: &CampaignData) -> EvalResult<()> {
    let mut expected = BTreeMap::from([
        ("A01".to_owned(), 3_usize),
        ("A02".to_owned(), 300),
        ("A03a".to_owned(), 3),
        ("A03b".to_owned(), 3),
        ("A09".to_owned(), 3),
        ("A10".to_owned(), 3),
        ("A11".to_owned(), 3),
        ("A12".to_owned(), 3),
        ("A13".to_owned(), 11),
        ("A14/edit".to_owned(), 4),
        ("A15".to_owned(), 3),
        ("A17/checkpoint".to_owned(), 100),
        ("A17/edit-plus-checkpoint".to_owned(), 100),
    ]);
    for id in ["A04", "A05", "A06", "A07", "A08"] {
        expected.insert(format!("{id}/logical"), 3);
        expected.insert(format!("{id}/native-edit-plus-checkpoint"), 3);
    }
    for (name, count) in expected {
        let actual = data.metrics.get(&name).map_or(0, Vec::len);
        if actual != count {
            return Err(format!("metric population {name}: {actual} != {count}"));
        }
    }
    Ok(())
}
