//! A Sandbox on a machine reached through its worker: `sandbox.route` and
//! `sandbox.worker_compatibility` here, then one `doctor.sandbox` request
//! whose report (the worker's own live checks from its fixed root, and its
//! Workspaces) is read into this one. A report this build does not read
//! leaves the Sandbox `not_checked` with `reason` `lost`.
use super::{local, read, Budget, Checks, Observed};
use crate::sandbox::workspace::{Listing, Seen};
use layerfs_home::names::{MachineName, SandboxName};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::state::BlockedBy;
use layerfs_machine::worker::{diagnostic_connect, DiagnosticOperation, Endpoint};
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::SandboxRecord;
use std::path::Path;
use std::time::{Duration, Instant};

/// The Sandboxes on `machine`, at least the one being checked.
fn sandboxes_on(home: &Path, machine: &str) -> usize {
    layerfs_sandbox::status::inventory::sandboxes_on(home, machine).map_or(1, |on| on.len().max(1))
}

pub(super) fn remote(
    home: &Path,
    record: &SandboxRecord,
    budget: &Budget,
    checks: &mut Checks,
    seen: &mut Observed,
) {
    const ROUTE: &str = "sandbox.route";
    let machine = record.machine();
    // `sandbox.machine_reference` names an invalid machine; no route follows
    if MachineName::parse(machine).is_err() {
        return;
    }
    let endpoint = match layerfs_home::secrets::worker_secret(home, machine) {
        Ok(Some(endpoint)) => Endpoint {
            address: endpoint.address,
            secret: endpoint.secret,
        },
        Ok(None) => {
            let by = BlockedBy {
                thing: "machine",
                handle: machine.to_owned(),
                reason: "not_answering",
            };
            let message = format!("machine {machine} has no worker recorded");
            checks.waits(ROUTE, message, by);
            return;
        }
        Err(_) => {
            let on = sandboxes_on(home, machine);
            checks.fail(
                ROUTE,
                format!("machine {machine}'s worker record is malformed or cannot be read; replacing the worker force-stops {on} Sandbox{} on it", if on == 1 { "" } else { "es" }),
            );
            checks.fix(ROUTE, layerfs_machine::state::replace_worker(machine, on));
            checks.evidence(
                ROUTE,
                SdkValue::map([
                    ("cause", SdkValue::text("worker_record_unreadable")),
                    ("machine", SdkValue::text(machine)),
                    (
                        "sandboxes_on",
                        SdkValue::Int(i64::try_from(on).unwrap_or(i64::MAX)),
                    ),
                ]),
            );
            return;
        }
    };
    let duration = budget.remaining().unwrap_or(Duration::from_secs(10));
    if duration.is_zero() {
        checks.timeout(
            ROUTE,
            "its time ran out before its machine's worker was dialled",
        );
        return;
    }
    let started = Instant::now();
    let worker = match diagnostic_connect(&endpoint, started + duration) {
        Ok(worker) => worker,
        Err(refusal) if refusal.code == "Denied" => {
            let on = sandboxes_on(home, machine);
            checks.fail(
                ROUTE,
                format!("machine {machine}'s worker refused its saved secret; replacing the worker force-stops {on} Sandbox{} on it", if on == 1 { "" } else { "es" }),
            );
            checks.fix(ROUTE, layerfs_machine::state::replace_worker(machine, on));
            checks.evidence(
                ROUTE,
                SdkValue::map([
                    ("cause", SdkValue::text("worker_refused_secret")),
                    ("machine", SdkValue::text(machine)),
                    (
                        "sandboxes_on",
                        SdkValue::Int(i64::try_from(on).unwrap_or(i64::MAX)),
                    ),
                ]),
            );
            return;
        }
        Err(_) => {
            checks.lost(
                ROUTE,
                format!("machine {machine}'s worker did not answer in time"),
            );
            return;
        }
    };
    if let Some(expected) = worker.hello.expected() {
        const COMPATIBLE: &str = "sandbox.worker_compatibility";
        let on = sandboxes_on(home, machine);
        let message = format!(
            "machine {machine}'s worker is from another build ({}; this build: {}); {on} Sandbox{} on it",
            worker.hello.describe(),
            expected.describe(),
            if on == 1 { " is" } else { "es are" },
        );
        checks.pass(ROUTE, format!("machine {machine}'s worker answers"));
        checks.fail(COMPATIBLE, message);
        // `on` is at least 1, so the stop is forced
        checks.fix(
            COMPATIBLE,
            layerfs_machine::state::replace_worker(machine, on),
        );
        checks.evidence(
            COMPATIBLE,
            SdkValue::map([
                ("machine", SdkValue::text(machine)),
                ("worker", SdkValue::text(worker.hello.describe())),
                ("expected", SdkValue::text(expected.describe())),
                (
                    "sandboxes_on",
                    SdkValue::Int(i64::try_from(on).unwrap_or(i64::MAX)),
                ),
            ]),
        );
        return;
    }
    let remaining = duration.saturating_sub(started.elapsed());
    let timeout_ms = i64::try_from(remaining.as_millis())
        .unwrap_or(i64::MAX)
        .max(1);
    let args = SdkValue::map([
        ("name", SdkValue::text(record.name())),
        ("timeout_ms", SdkValue::Int(timeout_ms)),
    ]);
    match worker.diagnose(DiagnosticOperation::Sandbox, args, &mut |_| Ok(())) {
        Ok(report) => {
            checks.pass(ROUTE, format!("machine {machine}'s worker answers"));
            checks.pass(
                "sandbox.worker_compatibility",
                format!("machine {machine}'s worker is from this build"),
            );
            merge(report, checks, seen);
        }
        Err(_) => checks.lost(
            ROUTE,
            format!("machine {machine}'s worker gave no whole report; not asked again"),
        ),
    }
}

/// The worker's report read into `checks` and `seen`: its rows, and its
/// Workspaces with how its daemon's list went.
fn merge(report: SdkValue, checks: &mut Checks, seen: &mut Observed) {
    let rows = match report.get("checks") {
        Some(SdkValue::List(rows)) if !report.get_bool("truncated") => rows,
        _ => {
            seen.lost = true;
            return;
        }
    };
    for row in rows {
        let Some(id) =
            (row.get_text("id")).filter(|id| id.starts_with("sandbox.") && id.len() < 80)
        else {
            seen.lost = true;
            return;
        };
        let message = row
            .get_text("message")
            .unwrap_or("the worker's observation");
        match (row.get_text("outcome"), row.get_text("severity")) {
            (Some("pass"), _) => checks.pass(id, message),
            (Some("fail"), Some("warning")) => checks.warning(id, message),
            (Some("fail"), Some("notice")) => checks.notice(id, message),
            (Some("fail"), _) => checks.fail(id, message),
            // a check the worker's own checks kept from running is not shown
            (Some("blocked"), _) => continue,
            _ => checks.unknown(id, message),
        }
        if let Some(fix) = row.get_text("fix").filter(|fix| !fix.is_empty()) {
            checks.fix(id, fix);
        }
        if let Some(evidence) = row.get("evidence").filter(|e| **e != SdkValue::Null) {
            checks.evidence(id, evidence.clone());
        }
    }
    seen.listing = Listing::of(report.get_text("listing"));
    let listed: Option<Vec<Seen>> = match report.get("workspaces") {
        Some(SdkValue::List(items)) => items.iter().map(Seen::of).collect(),
        _ => None,
    };
    match listed {
        Some(workspaces) => seen.workspaces = workspaces,
        None => seen.lost = true,
    }
}

/// Worker-only fixed-root observation; no Store routes or caller-controlled
/// paths: the live checks of Sandbox `name` from this root, its
/// Workspaces (`workspaces`, named by this root's records) and how its
/// daemon's list went (`listing`).
pub fn worker_diagnose(root: &Path, args: SdkValue) -> Outcome {
    let name = args
        .get_text("name")
        .ok_or_else(|| refused("InvalidInput", "sandbox diagnostic name"))?;
    SandboxName::parse(name)?;
    let millis = args
        .get_int("timeout_ms")
        .filter(|n| *n > 0)
        .ok_or_else(|| refused("InvalidInput", "sandbox diagnostic timeout"))?;
    let budget = Budget::new(Some(Duration::from_millis(millis as u64)));
    let mut checks = Checks::default();
    let mut seen = Observed::default();
    if let Some(record) = read(root, name, &mut checks) {
        local(root, &record, &budget, &mut checks, false, &mut seen);
    }
    let report = layerfs_home::home::with_field(
        checks.to_wire(),
        "workspaces",
        SdkValue::List(seen.workspaces.iter().map(Seen::to_sdk).collect()),
    );
    Ok(layerfs_home::home::with_field(
        report,
        "listing",
        SdkValue::text(seen.listing.word()),
    ))
}
