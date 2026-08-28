use super::artifact::{json_bool, json_string, json_u128};
use super::authentication::json_array_objects;
use super::markdown_helpers::format_ms;
use super::report_disposition::sum_key;
use super::row_parse::{json_all_u128, json_object, phase_wall, row_u128, ParsedRow};
use super::statistics::{statistics, Statistics};
use crate::stage1_fixture::EvalResult;
pub(crate) const SUMMARY_HEADINGS: [&str; 17] = [
    "# LayerFS Stage 1.1 — Single-file APFS Edge Result",
    "## 1. Disposition and custody",
    "## 2. Overall gate scoreboard",
    "## 3. Physical APFS edit to LayerFS checkpoint",
    "## 4. Physical count-changing amplification",
    "## 5. Logical LayerFS edit to physical APFS refresh",
    "## 6. Refresh-route summary",
    "## 7. Canonical locality",
    "## 8. Multi-edit save bursts",
    "## 9. Fresh Verified history sessions",
    "## 10. Materialization and reconstruction",
    "## 11. Transaction and authentication closure",
    "## 12. Storage growth and amplification",
    "## 13. Resource closure",
    "## 14. Timer closure",
    "## 15. Preserved failures and unavailable observations",
    "## 16. Final disposition",
];
pub(crate) const SUMMARY_TABLE_HEADERS: [&str; 23] = [
    "| Field | Value |",
    "| Artifact | SHA-256 | Additional identity |",
    "| Gate | Required | Observed | Status |",
    "| Operation | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms | Oracle | Status |",
    "| Size band | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms |",
    "| Seq | Operation | Offset | Suffix B | Replacement B | Native read B | Native write B | Equation | Route | Status |",
    "| Kind | n | Suffix shifted B | Native read B | Native write B | Amplification |",
    "| Operation | n | Logical p50 ms | Logical p95 ms | Route class | Refresh p50 ms | Refresh p95 ms | End-to-end p50 ms | End-to-end p95 ms | Oracle |",
    "| Route | Required count | Observed | p50 ms | p95 ms | Physical B | Rematerializations | Status |",
    "| Population | Transitions | CDC expected B | CDC observed B | Unaffected reads B | Unaffected writes B | Max nodes read | Max nodes emitted | Status |",
    "| Root | Pattern | Sub-edits | Native ms | Oracle ms | Checkpoint ms | Row ms | Transactions | COMMITs | Final B | Status |",
    "| Session | Head | Roots checked | Open/scrub ms | Objects authenticated | Bytes authenticated | Probe B | Writer tx | Native writes | Status |",
    "| Probe ordinal | n | p50 ms | p95 ms | Non-payload rows | Payload rows | Cache classification |",
    "| Root | Purpose | Logical B | Wall ms | MiB/s | Native write B | Exact bytes | Metadata | Cleanup |",
    "| Equation | Required | Observed/failures | Status |",
    "| Counter phase | Rows | Statements | Fetched/auth/role | Object read B | Object write B | Tx/COMMIT | Scrubs | Engine/VFS scratch tables | Q structural-reservation high B | Connections |",
    "| Metric | Initial | Terminal/peak | Delta | Status |",
    "| Root range | Transitions | Canonical B written | DB growth B | Amplification |",
    "| Resource | Hard gate | Observed | Status |",
    "| Row group | Rows | Maximum residual ns | Sum residual ns | Status |",
    "| Sequence | Artifact/row | Field | Availability/failure | Reason | Disposition impact |",
    "| Optimization metric | Attempt-007 before ms | Current after ms | Absolute gain ms | Owner |",
    "| Category | Result | Decisive evidence |",
];
pub(crate) fn validate_summary_headings(markdown: &str) -> EvalResult<()> {
    let actual = markdown
        .lines()
        .filter(|line| line.starts_with('#'))
        .collect::<Vec<_>>();
    if actual != SUMMARY_HEADINGS {
        return Err(format!("summary heading order mismatch: {actual:?}"));
    }
    Ok(())
}
pub(crate) fn validate_summary_markdown_contract(markdown: &str) -> EvalResult<()> {
    validate_summary_headings(markdown)?;
    for header in SUMMARY_TABLE_HEADERS {
        if !markdown.contains(header) {
            return Err(format!(
                "summary Markdown missing required table header {header}"
            ));
        }
    }
    if markdown.contains("Disposition: `PASS`") && markdown.contains("| `FAIL` |") {
        return Err("PASS summary Markdown contains a hard-gate FAIL cell".to_owned());
    }
    Ok(())
}
pub(crate) fn validate_summary_pair(json: &str, markdown: &str) -> EvalResult<()> {
    let status = json_string(json, "status")?;
    if !markdown.contains(&format!("Disposition: `{status}`"))
        || !markdown.contains(&format!("Result: `{status}`"))
    {
        return Err("summary JSON/Markdown disposition mismatch".to_owned());
    }
    let materialization = json_object(json, "materialization")?;
    let by_root = json_object(materialization, "by_root")?;
    for (root, purpose) in [
        ("R15", "Physical-chain milestone"),
        ("R30", "Logical-refresh milestone"),
        ("R34", "Burst-chain milestone"),
    ] {
        let receipt = json_object(by_root, root)?;
        let cleanup = json_bool(receipt, "cleanup_exact")?;
        let line = markdown
            .lines()
            .find(|line| line.starts_with(&format!("| {root} | {purpose}")))
            .ok_or_else(|| format!("summary Markdown missing {root} materialization row"))?;
        if line.ends_with("| `PASS` |") != cleanup {
            return Err(format!("{root} JSON/Markdown cleanup mismatch"));
        }
    }
    let failures = json_array_objects(json, "failures")?;
    for failure in &failures {
        let artifact = json_string(failure, "artifact")?;
        if !markdown.contains(&artifact) {
            return Err(format!("failure ledger missing from Markdown: {artifact}"));
        }
    }
    if !markdown.contains(&format!("Preserved failed attempts: `{}`", failures.len())) {
        return Err("summary JSON/Markdown failure-ledger count mismatch".to_owned());
    }
    let optimization = json_object(json, "optimization")?;
    let complete = json_object(optimization, "complete_wall")?;
    let before = json_u128(complete, "before_ns")?;
    let after = json_u128(complete, "after_ns")?;
    if !markdown.contains(&format!(
        "| Complete campaign wall | `{}` | `{}` |",
        format_ms(before),
        format_ms(after)
    )) {
        return Err("optimization JSON/Markdown complete-wall mismatch".to_owned());
    }
    let verified = json_object(optimization, "verified_open_by_root")?;
    for root in ["R5", "R15", "R30", "R34"] {
        let receipt = json_object(verified, root)?;
        if !markdown.contains(&format!(
            "| Verified open {root} | `{}` | `{}` |",
            format_ms(json_u128(receipt, "before_ns")?),
            format_ms(json_u128(receipt, "after_ns")?),
        )) {
            return Err(format!("optimization JSON/Markdown {root} mismatch"));
        }
        let counters = format!(
            "current scrub/graphs/fetched/object B/scratch=`{}/{}/{}/{}/{}`",
            json_u128(receipt, "retained_union_scrubs")?,
            json_u128(receipt, "namespace_graphs")?,
            json_u128(receipt, "fetched_rows")?,
            json_u128(receipt, "object_bytes_read")?,
            json_u128(receipt, "scratch_tables")?,
        );
        if !markdown.contains(&counters) {
            return Err(format!(
                "optimization JSON/Markdown {root} scrub-counter mismatch"
            ));
        }
    }
    Ok(())
}
pub(crate) fn title(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}
pub(crate) fn optional_route_stats(
    rows: &[ParsedRow],
    route: &str,
) -> EvalResult<Option<Statistics>> {
    let selected = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == route)
        .map(|row| phase_wall(&row.json, "changed_root_refresh"))
        .collect::<EvalResult<Vec<_>>>()?;
    if selected.is_empty() {
        Ok(None)
    } else {
        statistics(selected).map(Some)
    }
}
pub(crate) fn sum_route_bytes(
    rows: &[ParsedRow],
    route: &str,
    operation: Option<&str>,
) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| {
            row.row_group == "C05"
                && (row.native_route == route
                    || route == "Shift"
                        && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift"))
                && operation.is_none_or(|operation| row.operation == operation)
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "route physical bytes overflow".to_owned())
        })
}
pub(crate) fn sum_patch_bytes(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| {
            row.row_group == "C05"
                && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "patch bytes overflow".to_owned())
        })
}
pub(crate) fn maximum_group_key(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .map(|row| row_u128(row, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| format!("no {group} values for {key}"))
}
pub(crate) fn sum_subfield(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            json_all_u128(&row.json, key)?
                .into_iter()
                .try_fold(total, |total, value| {
                    total
                        .checked_add(value)
                        .ok_or_else(|| format!("{key} subfield sum overflow"))
                })
        })
}
pub(crate) fn storage_initial(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .find(|row| row.row_group == "C02")
        .map(|row| row_u128(row, "database_bytes"))
        .transpose()?
        .ok_or_else(|| "initial database bytes unavailable".to_owned())
}
pub(crate) fn storage_terminal(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .rev()
        .find(|row| row.row_group == "C07")
        .map(|row| row_u128(row, "database_bytes"))
        .transpose()?
        .ok_or_else(|| "terminal database bytes unavailable".to_owned())
}
pub(crate) fn storage_amplification(rows: &[ParsedRow]) -> EvalResult<f64> {
    let canonical = sum_key(rows, None, "canonical_object_bytes_written")?;
    Ok(if canonical == 0 {
        0.0
    } else {
        storage_terminal(rows)?.saturating_sub(storage_initial(rows)?) as f64 / canonical as f64
    })
}
pub(crate) fn range_sum(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    sum_key(rows, Some(group), key)
}
pub(crate) fn range_amplification(rows: &[ParsedRow], group: &str) -> EvalResult<f64> {
    let canonical = range_sum(rows, group, "canonical_object_bytes_written")?;
    Ok(if canonical == 0 {
        0.0
    } else {
        range_sum(rows, group, "database_growth_bytes")? as f64 / canonical as f64
    })
}
