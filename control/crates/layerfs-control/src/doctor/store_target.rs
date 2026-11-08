//! Store diagnostics resolve saved identity and authoritative placement once.
use super::target::{Budget, Checks};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{
    home::paths,
    outcome::{refused, Outcome as CallOutcome},
};
use layerfs_store::{
    diagnostic::{self, Observation, Options, Outcome},
    identity,
    place::{self, Executor},
    record::{StoreRecord, StoreRef},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn diagnose_integrity(
    home: &Path,
    name: &str,
    quick: bool,
    budget: &Budget,
    checks: &mut Checks,
) {
    let Some((record, identity)) = resolve(home, name, checks) else {
        return;
    };
    let options = Options {
        quick,
        deadline: budget.deadline(),
        cancelled: None,
    };
    let result = match record.store_ref() {
        Ok(StoreRef::Postgres { .. }) => {
            diagnostic::postgresql(&record, &identity, options, false, &mut |o| {
                observe(checks, o)
            })
        }
        Ok(reference @ StoreRef::Sqlite { .. }) => match place::plan(home, reference) {
            Ok(Executor::Local) => diagnostic::sqlite(
                &paths::store_file(home, name),
                &identity,
                options,
                &mut |o| observe(checks, o),
            ),
            Ok(Executor::Worker(_)) => {
                return remote(
                    home,
                    name,
                    reference.machine().unwrap_or(""),
                    &identity,
                    quick,
                    budget,
                    checks,
                )
            }
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        checks.unknown(
            "store.integrity.access",
            format!(
                "integrity verification did not complete ({}): {}",
                error.code, error.message
            ),
        );
        checks.blocked(
            "store.integrity.coverage",
            "remaining data was not verified",
            "store.integrity.access",
        );
    }
}

pub(super) fn diagnose_connection(home: &Path, name: &str, budget: &Budget, checks: &mut Checks) {
    let Some((record, identity)) = resolve(home, name, checks) else {
        return;
    };
    match record.store_ref(){
        Ok(StoreRef::Postgres{..})=>if let Err(error)=diagnostic::postgresql(&record,&identity,Options{quick:true,deadline:budget.deadline(),cancelled:None},true,&mut|o|observe(checks,o)){checks.unknown("store.connection",format!("PostgreSQL connection diagnosis unavailable ({})",error.code));},
        Ok(StoreRef::Sqlite{..})=>checks.fail("store.connection.kind","connection diagnostics require a PostgreSQL Store; use doctor store integrity for SQLite"),
        Err(_)=>checks.fail("store.record","Store kind is malformed"),
    }
}

fn resolve(
    home: &Path,
    name: &str,
    checks: &mut Checks,
) -> Option<(StoreRecord, identity::Identity)> {
    let record = match StoreRecord::of(home, name) {
        Ok(record) => record,
        Err(error) => {
            checks.fail(
                "store.record",
                format!("Store record unavailable or invalid ({})", error.code),
            );
            return None;
        }
    };
    if record.field("v").and_then(|v| {
        if let SdkValue::Int(v) = v {
            Some(*v)
        } else {
            None
        }
    }) != Some(layerfs_home::home::RECORD_VERSION)
        || record.text("name") != Some(name)
    {
        checks.fail(
            "store.record",
            "saved Store record name or version is invalid",
        );
        return None;
    }
    let identity = match identity::identity_in(&record) {
        Ok(identity) => identity,
        Err(_) => {
            checks.fail("store.identity", "saved binding/cursor identity is invalid");
            return None;
        }
    };
    checks.pass(
        "store.record",
        "Store record, kind and saved authority parsed",
    );
    Some((record, identity))
}

fn observe(checks: &mut Checks, observation: Observation) {
    if observation.id.ends_with(".progress") {
        return;
    }
    if observation.outcome == Outcome::Fail && observation.id.starts_with("store.advisory.") {
        checks.warning(observation.id, observation.message);
        return;
    }
    match observation.outcome {
        Outcome::Pass => checks.pass(observation.id, observation.message),
        Outcome::Fail => checks.fail(observation.id, observation.message),
        Outcome::Unknown => checks.unknown(observation.id, observation.message),
        Outcome::Skipped => checks.skip(observation.id, observation.message),
    }
}

fn remote(
    home: &Path,
    name: &str,
    machine: &str,
    identity: &identity::Identity,
    quick: bool,
    budget: &Budget,
    checks: &mut Checks,
) {
    use layerfs_machine::worker::{diagnostic_connect, Endpoint};
    let endpoint = match layerfs_home::secrets::worker_secret(home, machine) {
        Ok(Some(saved)) => Endpoint {
            address: saved.address,
            secret: saved.secret,
        },
        _ => {
            checks.unknown(
                "store.worker",
                "authoritative machine has no readable worker endpoint",
            );
            checks.blocked(
                "store.integrity.coverage",
                "database integrity could not be verified",
                "store.worker",
            );
            return;
        }
    };
    let deadline = Instant::now() + budget.limit(Duration::from_secs(5));
    let connection = match diagnostic_connect(&endpoint, deadline) {
        Ok(connection) => connection,
        Err(error) => {
            checks.unknown(
                "store.worker",
                format!("authoritative worker unavailable ({})", error.code),
            );
            return;
        }
    };
    let timeout = budget.remaining().map_or(SdkValue::Null, |remaining| {
        SdkValue::Int(i64::try_from(remaining.as_millis()).unwrap_or(i64::MAX))
    });
    let args = SdkValue::map([
        ("store", SdkValue::text(name)),
        ("binding", SdkValue::Bytes(identity.binding.clone())),
        ("cursor", SdkValue::Bytes(identity.cursor_key.to_vec())),
        ("quick", SdkValue::Bool(quick)),
        ("timeout_ms", timeout),
    ]);
    let result = connection.diagnose_store(args, budget.deadline(), &mut |event| {
        let id = event
            .get_text("id")
            .ok_or_else(|| refused("Unknown", "worker diagnostic event lacks id"))?;
        let message = event
            .get_text("message")
            .ok_or_else(|| refused("Unknown", "worker diagnostic event lacks message"))?;
        if id.ends_with(".progress") {
            return Ok(());
        }
        match event.get_text("outcome") {
            Some("pass") => checks.pass(id, message),
            Some("fail") if id.starts_with("store.advisory.") => checks.warning(id, message),
            Some("fail") => checks.fail(id, message),
            Some("skipped") => checks.skip(id, message),
            Some("unknown") => checks.unknown(id, message),
            _ => return Err(refused("Unknown", "worker diagnostic outcome invalid")),
        };
        Ok(())
    });
    match result {
        Ok(value) if value.get("completed") == Some(&SdkValue::Bool(true)) => checks.pass(
            "store.worker",
            "authoritative worker completed the requested audit",
        ),
        Ok(_) => checks.unknown("store.worker", "worker audit coverage was incomplete"),
        Err(error) => checks.unknown(
            "store.worker",
            format!("worker audit result unavailable ({})", error.code),
        ),
    }
}

/// Fixed-root observational worker handler; caller paths and source leases are excluded.
pub(crate) fn in_worker(
    root: &Path,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
) -> CallOutcome {
    let name = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store required"))?;
    layerfs_home::names::StoreName::parse(name)?;
    let bytes = |key: &str| match args.get(key) {
        Some(SdkValue::Bytes(bytes)) => Ok(bytes.clone()),
        _ => Err(refused("InvalidInput", format!("{key} bytes required"))),
    };
    let binding = bytes("binding")?;
    let cursor_key = bytes("cursor")?
        .try_into()
        .map_err(|_| refused("InvalidInput", "cursor width"))?;
    let quick = args.get("quick") == Some(&SdkValue::Bool(true));
    let timeout = match args.get("timeout_ms") {
        Some(SdkValue::Int(ms)) if *ms > 0 => Some(Duration::from_millis(*ms as u64)),
        Some(SdkValue::Null) | None => None,
        _ => {
            return Err(refused(
                "InvalidInput",
                "positive diagnostic timeout required",
            ))
        }
    };
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let options = Options {
        quick,
        deadline: timeout.and_then(|duration| Instant::now().checked_add(duration)),
        cancelled: Some(&cancelled),
    };
    let identity = identity::Identity {
        kind: identity::Kind::Sqlite,
        binding,
        cursor_key,
    };
    let mut stream_error = None;
    let result = diagnostic::sqlite(
        &paths::store_file(root, name),
        &identity,
        options,
        &mut |o| {
            if stream_error.is_some() {
                return;
            }
            let outcome = match o.outcome {
                Outcome::Pass => "pass",
                Outcome::Fail => "fail",
                Outcome::Unknown => "unknown",
                Outcome::Skipped => "skipped",
            };
            if let Err(error) = events(SdkValue::map([
                ("id", SdkValue::text(o.id)),
                ("outcome", SdkValue::text(outcome)),
                (
                    "message",
                    SdkValue::text(o.message.chars().take(4096).collect::<String>()),
                ),
            ])) {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                stream_error = Some(error);
            }
        },
    );
    if let Some(error) = stream_error {
        return Err(error);
    }
    result?;
    Ok(SdkValue::map([("completed", SdkValue::Bool(true))]))
}
