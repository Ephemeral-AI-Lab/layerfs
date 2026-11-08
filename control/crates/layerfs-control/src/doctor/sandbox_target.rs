//! Targeted Sandbox diagnosis over its saved route, without normal host dispatch.
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, read_record, RECORD_VERSION};
use layerfs_home::names::{self, MachineName, SandboxName};
use layerfs_home::op::Standing;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{diagnostic_connect, DiagnosticOperation, Endpoint};
use layerfs_sandbox::record::SandboxRecord;
use layerfs_store::record::{Placement, StoreRecord, StoreRef};
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "sandbox_channel.rs"]
mod channel;
#[path = "sandbox_runtime.rs"]
mod runtime;

pub(super) fn diagnose(home: &Path, name: &str, budget: &Budget, checks: &mut Checks) {
    let Some(record) = read(home, name, checks) else {
        return;
    };
    references(home, &record, checks);
    if let Some(op) = record.op() {
        match op.standing() {
            Standing::Running => checks.pass("sandbox.operation", "Sandbox operation owner is still active; diagnostic observation clears nothing"),
            Standing::Stale => checks.warning("sandbox.operation", "Sandbox retains an ended owner's certain operation"),
            Standing::Unresolved => checks.warning("sandbox.operation", "Sandbox retains an ended owner's uncertain operation; original outcome remains unresolved"),
        }
    } else {
        checks.pass(
            "sandbox.operation",
            "No sandbox lifecycle operation is recorded",
        );
    }
    if record.id().is_none() {
        checks.warning("sandbox.runtime", "Sandbox has no recorded started runtime");
        checks.blocked(
            "sandbox.daemon",
            "No started sandbox instance is recorded",
            "sandbox.runtime",
        );
        return;
    }
    if record.daemon().and_then(|d| d.forward()).is_some() {
        if runtime::forward(home, &record, checks) {
            local(home, &record, budget, checks, true);
        } else {
            checks.blocked(
                "sandbox.daemon",
                "Saved forward prerequisites prevent daemon observation",
                "sandbox.route",
            );
        }
    } else if record.machine() == "native" {
        checks.pass(
            "sandbox.route",
            "Saved route reaches the native daemon directly",
        );
        local(home, &record, budget, checks, false);
    } else {
        remote(home, &record, budget, checks);
    }
}

fn read(home: &Path, name: &str, checks: &mut Checks) -> Option<SandboxRecord> {
    if SandboxName::parse(name).is_err() {
        checks.fail("sandbox.record", "Invalid sandbox name");
        return None;
    }
    match read_record(&paths::sandbox_record(home, name)) {
        Ok(Some(value)) if value.get_int("v") == Some(RECORD_VERSION) => {
            checks.pass(
                "sandbox.record",
                "Sandbox record parses with a supported version",
            );
            Some(SandboxRecord::from_record(name, value))
        }
        Ok(None) => {
            checks.fail("sandbox.record", "Sandbox record does not exist");
            None
        }
        _ => {
            checks.fail(
                "sandbox.record",
                "Sandbox record is unreadable, malformed or has an unsupported version",
            );
            None
        }
    }
}

fn references(home: &Path, record: &SandboxRecord, checks: &mut Checks) {
    let project = record.project().and_then(|p| names::project(p).ok());
    match project {
        Some((store, project)) => {
            for (id, file) in [
                ("sandbox.store_reference", paths::store_record(home, store)),
                (
                    "sandbox.project_reference",
                    paths::project_record(home, store, project),
                ),
            ] {
                match read_record(&file) {
                    Ok(Some(value)) if value.get_int("v") == Some(RECORD_VERSION) => {
                        checks.pass(id, "Referenced record exists and has a supported version");
                        if id == "sandbox.store_reference" {
                            let store = StoreRecord::from_record(store, value);
                            match store.store_ref() {
                                Ok(StoreRef::Sqlite {
                                    machine: Placement::Machine(machine),
                                    ..
                                }) if machine == record.machine() => checks.pass(
                                    "sandbox.store_placement",
                                    "SQLite Store and sandbox share the same machine",
                                ),
                                Ok(StoreRef::Postgres { .. }) => checks.pass(
                                    "sandbox.store_placement",
                                    "PostgreSQL Store has no SQLite machine-placement restriction",
                                ),
                                Ok(StoreRef::Sqlite {
                                    machine: Placement::Unknown,
                                    ..
                                }) => checks.unknown(
                                    "sandbox.store_placement",
                                    "SQLite Store machine placement is unknown",
                                ),
                                Ok(_) => checks.fail(
                                    "sandbox.store_placement",
                                    "SQLite Store and sandbox are recorded on different machines",
                                ),
                                Err(_) => checks.fail(
                                    "sandbox.store_reference",
                                    "Referenced Store type is malformed or unsupported",
                                ),
                            }
                        }
                    }
                    _ => checks.fail(
                        id,
                        "Referenced record is missing, unreadable or incompatible",
                    ),
                }
            }
        }
        None => checks.fail(
            "sandbox.project_reference",
            "Sandbox project reference is missing or malformed",
        ),
    }
    let machine = record.machine();
    if MachineName::parse(machine).is_err() {
        checks.fail(
            "sandbox.machine_reference",
            "Sandbox machine reference is invalid",
        );
    } else if matches!(machine, "native" | "docker") {
        checks.pass(
            "sandbox.machine_reference",
            "Sandbox references a built-in machine",
        );
    } else {
        match read_record(&paths::machine_record(home, machine)) {
            Ok(Some(value)) if value.get_int("v") == Some(RECORD_VERSION) => checks.pass(
                "sandbox.machine_reference",
                "Machine record exists with a supported version",
            ),
            _ => checks.fail(
                "sandbox.machine_reference",
                "Machine record is missing, unreadable or incompatible",
            ),
        }
    }
}

fn local(
    home: &Path,
    record: &SandboxRecord,
    budget: &Budget,
    checks: &mut Checks,
    forwarded: bool,
) {
    if !forwarded {
        runtime::local(record, checks);
    }
    let directory = record.dir(home);
    let before = std::fs::metadata(directory.join("control.json"))
        .and_then(|m| m.modified())
        .ok();
    match layerfs_home::secrets::daemon_keys(&directory) {
        Ok(Some(keys)) => {
            let forward_endpoint = record
                .daemon()
                .and_then(|d| d.forward())
                .and_then(|f| f.address());
            let saved_endpoint = keys
                .get_text("endpoint")
                .and_then(|s| s.parse::<std::net::SocketAddr>().ok());
            if forwarded && forward_endpoint != saved_endpoint {
                checks.fail(
                    "sandbox.daemon_keys",
                    "Saved daemon endpoint does not match the recorded forward",
                );
                checks.blocked(
                    "sandbox.daemon",
                    "Forward endpoint and discovery record disagree",
                    "sandbox.daemon_keys",
                );
            } else {
                channel::observe(&keys, budget, checks);
            }
        }
        _ => {
            checks.fail(
                "sandbox.daemon_keys",
                "Saved daemon control record is missing or unreadable",
            );
            checks.blocked(
                "sandbox.daemon",
                "No usable daemon discovery record",
                "sandbox.daemon_keys",
            );
        }
    }
    let after = std::fs::metadata(directory.join("control.json"))
        .and_then(|m| m.modified())
        .ok();
    if before != after {
        checks.unknown(
            "sandbox.observation",
            "Daemon discovery record changed during observation",
        );
    }
    if forwarded {
        checks.skip("sandbox.runtime_placement", "Guest runtime containment is not separately inspected; daemon connection/identity results provide available presence evidence without a worker RPC");
    }
}

fn remote(home: &Path, record: &SandboxRecord, budget: &Budget, checks: &mut Checks) {
    if MachineName::parse(record.machine()).is_err() {
        checks.blocked(
            "sandbox.route",
            "Machine reference is invalid",
            "sandbox.machine_reference",
        );
        return;
    }
    let endpoint = match layerfs_home::secrets::worker_secret(home, record.machine()) {
        Ok(Some(endpoint)) => Endpoint {
            address: endpoint.address,
            secret: endpoint.secret,
        },
        Ok(None) => {
            checks.unknown(
                "sandbox.route",
                "Machine has no recorded worker; it may be stopped",
            );
            checks.blocked(
                "sandbox.daemon",
                "Worker route is unavailable",
                "sandbox.route",
            );
            return;
        }
        _ => {
            checks.fail(
                "sandbox.route",
                "Worker discovery record is malformed or unreadable",
            );
            return;
        }
    };
    let duration = budget.remaining().unwrap_or(Duration::from_secs(10));
    if duration.is_zero() {
        checks.unknown("sandbox.route", "Deadline expired before reaching worker");
        return;
    }
    let started = Instant::now();
    let worker = match diagnostic_connect(&endpoint, started + duration) {
        Ok(worker) => worker,
        Err(refusal) => {
            if refusal.code == "Denied" {
                checks.fail("sandbox.route", "Worker rejected saved authentication");
            } else {
                checks.unknown(
                    "sandbox.route",
                    "Worker route could not be authenticated within the diagnostic budget",
                );
            }
            checks.blocked(
                "sandbox.daemon",
                "Worker route is unavailable",
                "sandbox.route",
            );
            return;
        }
    };
    if worker.hello.expected().is_some() {
        checks.fail("sandbox.worker_compatibility", "Worker protocol or sealed bundle differs from this caller; diagnostic request was not sent");
        checks.blocked(
            "sandbox.daemon",
            "Worker is incompatible",
            "sandbox.worker_compatibility",
        );
        return;
    }
    let remaining = duration.saturating_sub(started.elapsed());
    match worker.diagnose(
        DiagnosticOperation::Sandbox,
        SdkValue::map([
            ("name", SdkValue::text(record.name())),
            (
                "timeout_ms",
                SdkValue::Int(
                    i64::try_from(remaining.as_millis())
                        .unwrap_or(i64::MAX)
                        .max(1),
                ),
            ),
        ]),
        &mut |_| Ok(()),
    ) {
        Ok(report) => {
            checks.pass(
                "sandbox.route",
                "Authenticated compatible worker served the observational sandbox route",
            );
            merge(report, checks);
        }
        Err(refusal) => {
            let code = match refusal.code.as_str() {
                "Unknown" | "Unavailable" | "Denied" | "Timeout" | "Unsupported"
                | "InvalidInput" | "TooLarge" => refusal.code.as_str(),
                _ => "Unrecognized",
            };
            checks.unknown(
                "sandbox.route",
                format!("Worker diagnostic failed or has an unknown outcome ({code})"),
            );
            checks.blocked(
                "sandbox.daemon",
                "Worker returned no complete diagnostic report",
                "sandbox.route",
            );
        }
    }
}

fn merge(report: SdkValue, checks: &mut Checks) {
    let Some(SdkValue::List(rows)) = report
        .get("checks")
        .filter(|_| !report.get_bool("truncated"))
    else {
        checks.unknown(
            "sandbox.worker_report",
            "Worker diagnostic report was malformed or truncated",
        );
        return;
    };
    for row in rows {
        let Some(id) = row
            .get_text("id")
            .filter(|id| id.starts_with("sandbox.") && id.len() < 80)
        else {
            checks.unknown(
                "sandbox.worker_report",
                "Worker returned an invalid diagnostic check identity",
            );
            return;
        };
        let message = row.get_text("message").unwrap_or("Worker observation");
        match row.get_text("outcome") {
            Some("pass") => checks.pass(id, message),
            Some("fail") if row.get_text("severity") == Some("warning") => {
                checks.warning(id, message)
            }
            Some("fail") => checks.fail(id, message),
            Some("skipped") => checks.skip(id, message),
            Some("blocked") => {
                let dependency = row
                    .get("blocked_by")
                    .and_then(|v| match v {
                        SdkValue::List(items) => items.first(),
                        _ => None,
                    })
                    .and_then(|v| match v {
                        SdkValue::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .unwrap_or("sandbox.route");
                checks.blocked(id, message, dependency);
            }
            _ => checks.unknown(id, message),
        }
        if let Some(evidence) = row.get("evidence") {
            checks.evidence(id, evidence.clone());
        }
    }
    if !report.get_bool("complete") {
        checks.incomplete();
    }
}

/// Worker-only fixed-root observation; no Store routes or caller-controlled paths.
pub(crate) fn worker_diagnose(root: &Path, args: SdkValue) -> Outcome {
    let name = args
        .get_text("name")
        .ok_or_else(|| refused("InvalidInput", "sandbox diagnostic name"))?;
    SandboxName::parse(name)?;
    let millis = args
        .get_int("timeout_ms")
        .filter(|n| *n > 0)
        .ok_or_else(|| refused("InvalidInput", "sandbox diagnostic timeout"))?;
    let budget = Budget::new(Some(Duration::from_millis(millis as u64)));
    let mut checks = Checks::new("sandbox", name);
    if let Some(record) = read(root, name, &mut checks) {
        local(root, &record, &budget, &mut checks, false);
    }
    Ok(checks.to_sdk())
}
