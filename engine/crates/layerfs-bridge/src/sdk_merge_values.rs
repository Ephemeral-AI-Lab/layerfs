//! Byte-bounded merge diagnostics; omitted details never change merge semantics.
use crate::control_sdk::SdkValue;
use layerfs_content::merge::{Conflict, LwwDecision, Side, UnitReport};

// Separate windows leave room for source identities, outcome and observation.
const WINDOW: usize = 16 * 1024;

fn spend(remaining: &mut usize, cost: usize) -> bool {
    match remaining.checked_sub(cost) {
        Some(next) => {
            *remaining = next;
            true
        }
        None => false,
    }
}

fn conflict_window(values: &[Conflict]) -> Vec<usize> {
    let mut remaining = WINDOW;
    let mut hunks = Vec::new();
    for conflict in values {
        if !spend(&mut remaining, conflict.path.len().saturating_add(256)) {
            break;
        }
        let mut count = 0;
        for hunk in &conflict.hunks {
            let cost = hunk
                .ours
                .iter()
                .chain(&hunk.theirs)
                .fold(96usize, |sum, line| {
                    sum.saturating_add(line.len().saturating_add(8))
                });
            if !spend(&mut remaining, cost) {
                break;
            }
            count += 1;
        }
        hunks.push(count);
    }
    hunks
}

/// Whether the byte window omitted any conflict paths. Individual records also
/// report `hunksOmitted` when their full marker text did not fit.
pub fn conflicts_truncated(values: &[Conflict]) -> bool {
    conflict_window(values).len() != values.len()
}

/// Conflict paths and bounded whole hunks. `blocks` always reports the original
/// block count, and `hunksOmitted` explicitly identifies incomplete hunk text.
pub fn conflicts(values: &[Conflict]) -> SdkValue {
    let lines = |lines: &[String]| SdkValue::List(lines.iter().map(SdkValue::text).collect());
    SdkValue::List(
        values
            .iter()
            .zip(conflict_window(values))
            .map(|(conflict, count)| {
                SdkValue::map([
                    ("path", SdkValue::text(&conflict.path)),
                    ("kind", SdkValue::text(conflict.kind.as_str())),
                    ("blocks", SdkValue::Int(conflict.hunks.len() as i64)),
                    (
                        "hunksOmitted",
                        SdkValue::Bool(count != conflict.hunks.len()),
                    ),
                    (
                        "hunks",
                        SdkValue::List(
                            conflict.hunks[..count]
                                .iter()
                                .map(|hunk| {
                                    SdkValue::map([
                                        ("line", SdkValue::Int(hunk.line as i64)),
                                        ("ours", lines(&hunk.ours)),
                                        ("theirs", lines(&hunk.theirs)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect(),
    )
}

fn lww_count(values: &[LwwDecision]) -> usize {
    let mut remaining = WINDOW;
    values
        .iter()
        .take_while(|decision| {
            spend(
                &mut remaining,
                decision
                    .path
                    .len()
                    .saturating_add(decision.rule.len())
                    .saturating_add(96),
            )
        })
        .count()
}

/// Whether the byte window omitted any generated-path decisions.
pub fn lww_truncated(values: &[LwwDecision]) -> bool {
    lww_count(values) != values.len()
}

/// `.lww` decisions in one byte window. SDK results pair this list with
/// `lwwTruncated`; this bounds diagnostics, never the merge's total work.
pub fn lww(values: &[LwwDecision]) -> SdkValue {
    SdkValue::List(
        values[..lww_count(values)]
            .iter()
            .map(|decision| {
                SdkValue::map([
                    ("path", SdkValue::text(&decision.path)),
                    (
                        "kept",
                        SdkValue::text(if decision.kept_ours { "ours" } else { "theirs" }),
                    ),
                    ("deleted", SdkValue::Bool(decision.deleted)),
                    ("rule", SdkValue::text(&decision.rule)),
                ])
            })
            .collect(),
    )
}

fn units_count(values: &[UnitReport]) -> usize {
    let mut remaining = WINDOW;
    values
        .iter()
        .take_while(|unit| spend(&mut remaining, unit.name.len().saturating_add(192)))
        .count()
}

/// Whether the byte window omitted any decided merge unit (#352 F-057).
pub fn units_truncated(values: &[UnitReport]) -> bool {
    units_count(values) != values.len()
}

/// Merge units both sides changed and how each was decided, in one byte
/// window. SDK results pair this list with `unitsTruncated`.
pub fn units(values: &[UnitReport]) -> SdkValue {
    let count = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    SdkValue::List(
        values[..units_count(values)]
            .iter()
            .map(|unit| {
                SdkValue::map([
                    ("unit", SdkValue::text(&unit.name)),
                    (
                        "kept",
                        SdkValue::text(match unit.kept {
                            Side::Ours => "ours",
                            Side::Theirs => "theirs",
                        }),
                    ),
                    ("keptTime", SdkValue::text(utc(unit.kept_time))),
                    ("changedOurs", count(unit.changed_ours)),
                    ("changedTheirs", count(unit.changed_theirs)),
                    ("dropped", count(unit.dropped)),
                    ("crossLinks", count(unit.cross_links)),
                ])
            })
            .collect(),
    )
}

/// RFC 3339 UTC text of a modification time, with nanoseconds.
fn utc((seconds, nanoseconds): (i64, u32)) -> String {
    let (days, second) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{nanoseconds:09}Z",
        second / 3600,
        second / 60 % 60,
        second % 60
    )
}
