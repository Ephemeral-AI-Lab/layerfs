//! One thing's own checks through the doctor frame, as the real kinds make
//! them, with nothing on it and nothing above it: what today's per-kind
//! tests read. [`own`] answers the report's long form; [`rows`] lays its
//! tree's rows out as today's per-check rows, `{ id, outcome, severity,
//! message, fix, evidence, reason, blocked_by }`: `outcome` is `pass`, `fail`
//! (with the finding's `severity`, `message`, `fix` and `evidence`) or
//! `unknown` (with its `reason` or `blocked_by`). A check that does not
//! apply has no row.
#![allow(dead_code)]
use layerfs_home::events::Events;
use layerfs_home::state::State;
use layerfs_inspect::{kinds, run_with, Kind, Located, Options, Registry, Thing};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use std::time::Duration;

fn alone(_: &Path, _: &Thing) -> Result<Located, SdkRefusal> {
    Ok(Located {
        state: State::Ready,
        blocked: None,
        children: Vec::new(),
    })
}

/// The long report of the `kind` thing `name`'s own checks.
pub fn own(home: &Path, kind: Kind, name: &str, options: &Options) -> SdkValue {
    own_within(home, kind, name, options, None)
}

/// [`own`] with the kind's budget replaced by `budget`.
pub fn own_within(
    home: &Path,
    kind: Kind,
    name: &str,
    options: &Options,
    budget: Option<Duration>,
) -> SdkValue {
    let mut doctors = kinds().to_vec();
    if let Some(budget) = budget {
        for doctor in doctors.iter_mut().filter(|doctor| doctor.kind == kind) {
            doctor.budget = budget;
        }
    }
    let registry = Registry {
        kinds: Box::leak(doctors.into_boxed_slice()),
        locate: alone,
    };
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    let report = run_with(
        &registry,
        home,
        Thing::new(kind, name),
        options,
        &mut events,
    );
    report.unwrap().to_sdk()
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// The report's rows as today's per-check rows.
pub fn rows(report: &SdkValue) -> Vec<SdkValue> {
    let tree = report.get("tree").cloned().unwrap_or_default();
    let mut out = Vec::new();
    for row in list(&tree, "checks") {
        let id = row.get_text("check").unwrap_or("");
        let outcome = row.get_text("outcome").unwrap_or("");
        let finding = (list(report, "findings").iter())
            .rev()
            .find(|finding| finding.get_text("check") == Some(id));
        let open = (list(report, "not_checked").iter())
            .rev()
            .find(|entry| entry.get_text("check") == Some(id));
        let field = |from: Option<&SdkValue>, key: &str| {
            from.and_then(|value| value.get(key))
                .cloned()
                .unwrap_or_default()
        };
        let legacy = match outcome {
            "pass" => "pass",
            "not_checked" => "unknown",
            _ => "fail",
        };
        out.push(SdkValue::map([
            ("id", SdkValue::text(id)),
            ("outcome", SdkValue::text(legacy)),
            ("severity", field(finding, "severity")),
            ("message", field(finding, "message")),
            ("fix", field(finding, "fix")),
            ("evidence", field(finding, "evidence")),
            ("reason", field(open, "reason")),
            ("blocked_by", field(open, "blocked_by")),
        ]));
    }
    out
}

/// The row of check `id`, if the check applied.
pub fn row(report: &SdkValue, id: &str) -> Option<SdkValue> {
    rows(report)
        .into_iter()
        .find(|row| row.get_text("id") == Some(id))
}

/// The outcome of check `id` as today's rows say it: `pass`, `fail` or
/// `unknown`; none when it did not apply.
pub fn outcome(report: &SdkValue, id: &str) -> Option<String> {
    row(report, id).and_then(|row| row.get_text("outcome").map(str::to_owned))
}

/// Whether every check was made: nothing is `not_checked`.
pub fn complete(report: &SdkValue) -> bool {
    list(report, "not_checked").is_empty()
}
