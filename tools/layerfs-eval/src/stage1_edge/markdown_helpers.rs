use super::artifact::{io_error, json_escape};
use super::row_parse::{phase_wall, ParsedRow};
use crate::stage1_fixture::{self, EvalResult};
use std::fs;
use std::path::Path;
pub(crate) fn sum_phase(rows: &[ParsedRow], group: &str, phase: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, phase)?)
                .ok_or_else(|| format!("{group}/{phase} phase sum overflow"))
        })
}
pub(crate) fn sum_row_walls(rows: &[ParsedRow], group: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row.row_wall_ns)
                .ok_or_else(|| format!("{group} row wall sum overflow"))
        })
}
pub(crate) fn format_ms(ns: u128) -> String {
    let ms = ns as f64 / 1_000_000.0;
    if ms < 1.0 {
        format!("{ms:.6}")
    } else {
        format!("{ms:.3}")
    }
}
pub(crate) fn format_signed_ms(ns: i128) -> String {
    if ns < 0 {
        format!("-{}", format_ms(ns.unsigned_abs()))
    } else {
        format!("+{}", format_ms(ns as u128))
    }
}
pub(crate) fn throughput_mib_s(bytes: u64, ns: u128) -> f64 {
    if ns == 0 {
        0.0
    } else {
        bytes as f64 / 1_048_576.0 / (ns as f64 / 1_000_000_000.0)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct FailureLedgerEntry {
    pub(crate) artifact: String,
    pub(crate) field: String,
    pub(crate) reason: String,
    pub(crate) disposition_impact: String,
}
pub(crate) fn absolute_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}
pub(crate) fn preserved_failure_ledger(current_run: &Path) -> EvalResult<Vec<FailureLedgerEntry>> {
    let target = stage1_fixture::workspace_root().join("target");
    let current = current_run
        .canonicalize()
        .unwrap_or_else(|_| current_run.to_path_buf());
    let mut failures = Vec::new();
    for entry in fs::read_dir(&target).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "layerfs-stage1-fixtures" {
            let fixture_parent = entry.path();
            if fixture_parent.is_dir() {
                for child in fs::read_dir(fixture_parent).map_err(io_error)? {
                    let child = child.map_err(io_error)?;
                    if child
                        .file_name()
                        .to_string_lossy()
                        .starts_with("apple-edge-v1-preparation-failure-")
                    {
                        failures.push(FailureLedgerEntry {
                            artifact: absolute_path(&child.path()),
                            field: "fixture.preparation".to_owned(),
                            reason: "preparation stopped before immutable fixture publication"
                                .to_owned(),
                            disposition_impact: "preserved and superseded by the sealed fixture"
                                .to_owned(),
                        });
                    }
                }
            }
        } else if entry.path().is_dir() && name.starts_with("layerfs-stage1-apple-edge-") {
            let path = entry.path();
            if path.canonicalize().unwrap_or_else(|_| path.clone()) == current {
                continue;
            }
            if name.ends_with("-attempt-006") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "refresh.hard_link_alias_order;summary.md.final_disposition.complete_wall"
                        .to_owned(),
                    reason: "D7 found a non-first hard-link alias could miss AcceptedSplice and use FullFallback, plus a malformed final-wall Markdown code span"
                        .to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by a repaired source".to_owned(),
                });
                continue;
            }
            if name.ends_with("-attempt-010") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "optimization.verified_open_by_root.R34".to_owned(),
                    reason: "D7 found the R34 retained-union comparison used clean reopen C08-003 instead of the full R34-head scrub C08-001".to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by corrected row-derived attribution"
                            .to_owned(),
                });
                continue;
            }
            if name.ends_with("-attempt-011") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "tests.eof_post_visibility_conflict".to_owned(),
                    reason: "D7 found the APFS post-visibility conflict regression depended on observer scheduling during a finite copy window".to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by deterministic cfg(test) Apple fault synchronization"
                            .to_owned(),
                });
                continue;
            }
            let stderr = path.join("stderr.txt");
            if stderr.is_file() {
                let reason = fs::read_to_string(&stderr)
                    .map_err(io_error)?
                    .lines()
                    .next()
                    .unwrap_or("unknown retained failure")
                    .to_owned();
                let field = if reason.contains("locality") {
                    "canonical_locality"
                } else if reason.contains("complete_wall") {
                    "walls_ns.complete_wall"
                } else if reason.contains("milestone") {
                    "materialization.C08"
                } else {
                    "campaign.first_failed_equation"
                };
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: field.to_owned(),
                    reason,
                    disposition_impact: "preserved and superseded by a repaired source".to_owned(),
                });
            } else {
                let markdown = fs::read_to_string(path.join("summary.md")).unwrap_or_default();
                if markdown.contains("| `FAIL` |") {
                    failures.push(FailureLedgerEntry {
                        artifact: absolute_path(&path),
                        field: "summary.md.materialization.cleanup".to_owned(),
                        reason:
                            "D7 found C08 cleanup cells contradicting retained destination custody"
                                .to_owned(),
                        disposition_impact:
                            "preserved as D7 evidence and superseded by a repaired source"
                                .to_owned(),
                    });
                }
            }
        }
    }
    #[cfg(test)]
    for (attempt, field, reason) in [
        (
            "attempt-010",
            "optimization.verified_open_by_root.R34",
            "synthetic preserved failure for the self-contained summary contract",
        ),
        (
            "attempt-011",
            "tests.eof_post_visibility_conflict",
            "synthetic preserved failure for the self-contained summary contract",
        ),
    ] {
        if !failures
            .iter()
            .any(|failure| failure.artifact.ends_with(attempt))
        {
            failures.push(FailureLedgerEntry {
                artifact: absolute_path(
                    &target.join(format!("layerfs-stage1-apple-edge-synthetic-{attempt}")),
                ),
                field: field.to_owned(),
                reason: reason.to_owned(),
                disposition_impact: "synthetic unit-test receipt only".to_owned(),
            });
        }
    }
    failures.sort_by(|left, right| left.artifact.cmp(&right.artifact));
    Ok(failures)
}
pub(crate) fn failure_ledger_json(failures: &[FailureLedgerEntry]) -> String {
    failures
        .iter()
        .enumerate()
        .map(|(index, failure)| {
            format!(
                concat!(
                    "{{\"sequence\":{},\"artifact\":\"{}\",",
                    "\"field\":\"{}\",\"availability\":\"failure\",",
                    "\"reason\":\"{}\",\"disposition_impact\":\"{}\"}}"
                ),
                index + 1,
                json_escape(&failure.artifact),
                json_escape(&failure.field),
                json_escape(&failure.reason),
                json_escape(&failure.disposition_impact),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}
