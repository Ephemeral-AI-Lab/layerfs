//! A Store's doctor (call surface 2.0, `checks.store`): its record, the
//! operation on it, then the shallow cut where it lives (C14), which
//! reaches it and stops after the schema check, reading no data. A sqlite
//! Store is reached through its machine (this process for `native`, the
//! worker's `store.diagnose` with `cut: schema` elsewhere), which also
//! reads its heads when it has a bucket link; a PostgreSQL Store from this
//! computer, then once more from each machine with a Sandbox of it and from
//! `--machine` (R9), one `store.connection` each with `from`.
//! `store.operation` reads the home's record and, for a sqlite Store, the
//! compaction record beside its file through its machine (the shallow cut
//! answers it, spec v19). Under a machine that is not ready only
//! `store.record` is made: the rest wait on the machine.
use super::frame::{CheckId, Probe};
use super::report::Node;
use super::target::{Budget, Checks};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::op::Standing;
use layerfs_home::outcome::{refused, Outcome as CallOutcome};
use layerfs_home::state::{Reason, State};
use layerfs_store::diagnostic::{self, Observation, Options, Outcome};
use layerfs_store::identity::{self, Identity};
use layerfs_store::place::{self, Executor};
use layerfs_store::record::{clearing_call, StoreRecord, StoreRef};
use std::path::Path;
use std::time::{Duration, Instant};

const RECORD: &str = "store.record";
const OPERATION: &str = "store.operation";
const CONNECTION: &str = "store.connection";
const IDENTITY: &str = "store.identity";
const SCHEMA: &str = "store.schema";
const BACKUPS: &str = "store.backups";

/// A Store's checks, in the frozen order.
pub(super) const CHECKS: &[CheckId] = &[
    CheckId::own(RECORD),
    CheckId::own(OPERATION),
    CheckId::own(CONNECTION),
    CheckId::own(IDENTITY),
    CheckId::own(SCHEMA),
    CheckId::own(BACKUPS),
];

/// The worker's shallow cut, the one `cut` its `store.diagnose` takes.
const CUT: &str = "schema";

/// Checks Store `probe.thing` into `node`: its state from the home's
/// records (`layerfs_store::state`, with the machine family's derivation
/// handed in) says whether it waits on its machine, and what blocks the
/// Store blocks its projects too.
pub(super) fn run(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::new("store", name);
    let mut state = probe.state.clone();
    if let Some((record, identity)) = resolve(probe.home, name, &mut checks) {
        let machines = crate::store::machine_state;
        if let Ok(recorded) = layerfs_store::state::recorded(probe.home, &record, &machines) {
            state = recorded;
        }
        match &state {
            State::Blocked(by) => {
                let message = format!("its {} {} is not ready", by.thing, by.handle);
                checks.waits(OPERATION, message.clone(), by.clone());
                checks.waits(CONNECTION, message, by.clone());
            }
            _ => match record.store_ref() {
                Ok(StoreRef::Postgres { .. }) => {
                    operation(probe.home, &record, None, &mut checks);
                    postgresql(probe, &record, &identity, &mut checks)
                }
                Ok(StoreRef::Sqlite { .. }) => sqlite(probe, &record, &identity, &mut checks),
                Err(_) => operation(probe.home, &record, None, &mut checks),
            },
        }
    }
    checks.fill(node, probe.checks, probe.budget.expired());
    if node.gate.is_none() {
        node.gate = match state.blocks("store", name) {
            Some(State::Blocked(by)) => Some(by),
            _ => None,
        };
    }
}

/// `store.record`: the record is valid, its name, engine and saved id.
fn resolve(home: &Path, name: &str, checks: &mut Checks) -> Option<(StoreRecord, Identity)> {
    let record = match StoreRecord::of(home, name) {
        Ok(record) => record,
        Err(error) => {
            checks.fail(
                RECORD,
                format!(
                    "its record cannot be read ({}): {}",
                    error.code, error.message
                ),
            );
            return None;
        }
    };
    let version = match record.field("v") {
        Some(SdkValue::Int(v)) => Some(*v),
        _ => None,
    };
    if version != Some(layerfs_home::home::RECORD_VERSION) || record.text("name") != Some(name) {
        checks.fail(RECORD, "its record's name or version is not this Store's");
        return None;
    }
    if let Err(error) = record.kind() {
        checks.fail(
            RECORD,
            format!(
                "its record names no engine LayerFS writes: {}",
                error.message
            ),
        );
        return None;
    }
    match identity::identity_in(&record) {
        Ok(identity) => {
            checks.pass(RECORD, "its record names it, its engine and its id");
            Some((record, identity))
        }
        Err(error) => {
            checks.fail(
                RECORD,
                format!("its record's saved id is not valid: {}", error.message),
            );
            None
        }
    }
}

/// `store.operation`: no operation on it is half-finished or left with its
/// outcome not known, and no lease is stale; read from the home's records
/// and, for a sqlite Store, from `compaction`, the compaction record beside
/// its file as its machine read it (`report::compact_status`, null when
/// there is none). A run live there passes, even when the home's record
/// holds it unresolved after a lost reply.
fn operation(
    home: &Path,
    record: &StoreRecord,
    compaction: Option<&SdkValue>,
    checks: &mut Checks,
) {
    let name = record.name();
    let there = compaction.and_then(|run| Some((run.get_text("state")?, run.get_text("phase")?)));
    match (record.op(), there) {
        (_, Some(("busy", phase))) => checks.pass(
            OPERATION,
            format!("a compaction is running on it where it lives (phase {phase})"),
        ),
        (Some(op), _) if op.standing() == Standing::Unresolved => {
            let message = format!(
                "a {} on it left its outcome not known, and its process is gone",
                op.word()
            );
            checks.fail(OPERATION, message);
            checks.fix(OPERATION, format!("layerfs {}", clearing_call(&op, name)));
        }
        (Some(op), _) if op.standing() == Standing::Running => checks.pass(
            OPERATION,
            format!("a {} is running on it (phase {})", op.word(), op.phase()),
        ),
        (_, Some(("unresolved", phase))) => {
            let message = format!(
                "a compaction on it left its outcome not known in phase {phase}, and its process is gone"
            );
            checks.fail(OPERATION, message);
            checks.fix(OPERATION, format!("layerfs store compact {name}"));
        }
        _ => checks.pass(OPERATION, "no operation on it is unfinished"),
    }
    match layerfs_store::lease::read_holders(home, name) {
        Ok((_, stale)) if !stale.is_empty() => {
            let pids: Vec<String> = stale.iter().map(u32::to_string).collect();
            checks.notice(
                OPERATION,
                format!(
                    "{} lease(s) of processes that are gone remain (pids {}); the next call ignores them",
                    stale.len(),
                    pids.join(", ")
                ),
            );
        }
        Ok(_) => {}
        Err(error) => checks.unknown(
            OPERATION,
            format!("its leases cannot be read: {}", error.message),
        ),
    }
}

/// A sqlite Store reached where it lives: on this computer for `native`,
/// through its machine's worker elsewhere.
fn sqlite(probe: &Probe<'_>, record: &StoreRecord, identity: &Identity, checks: &mut Checks) {
    let name = record.name();
    let machine = record.machine().unwrap_or_default();
    let heads = !record.links().is_empty();
    let found = match place::on_machine_read(probe.home, &machine) {
        Ok(Executor::Local) => Ok(shallow(
            &paths::store_file(probe.home, name),
            identity,
            heads,
        )),
        Ok(Executor::Worker(_)) => {
            remote(probe.home, name, &machine, identity, heads, probe.budget)
        }
        Err(error) => Err(error),
    };
    let found = match found {
        Ok(found) => found,
        Err(error) => {
            operation(probe.home, record, None, checks);
            let message = format!("machine {machine} did not answer: {}", error.message);
            match error.code.as_str() {
                "Busy" => checks.busy(CONNECTION, message),
                _ => checks.fail(CONNECTION, message),
            }
            return;
        }
    };
    operation(probe.home, record, found.get("compaction"), checks);
    let text = |key: &str| found.get_text(key).map(str::to_owned);
    if found.get("found") != Some(&SdkValue::Bool(true)) {
        checks.fail(
            CONNECTION,
            format!("machine {machine} has no file where its record says"),
        );
        checks.evidence(
            CONNECTION,
            SdkValue::map([("reason", SdkValue::text("missing"))]),
        );
        return;
    }
    if let Some(why) = text("unsupported") {
        checks.warning(
            CONNECTION,
            format!("machine {machine} cannot open a sqlite Store: {why}"),
        );
        return;
    }
    checks.pass(
        CONNECTION,
        format!("its file is reached on machine {machine}"),
    );
    if let Some(why) = text("schema_error") {
        checks.fail(
            SCHEMA,
            format!("its file does not open with this Store's id and the schema this build reads: {why}"),
        );
        return;
    }
    checks.pass(IDENTITY, "the id in its file is the id in its record");
    checks.pass(SCHEMA, "its schema is the one this build reads");
    if heads {
        backups(record, text("heads"), text("heads_error"), checks);
    }
}

/// `store.backups`: each bucket link's heads against the Store's now; a
/// backup that is behind is a notice on the Store, one per bucket.
fn backups(record: &StoreRecord, now: Option<String>, error: Option<String>, checks: &mut Checks) {
    let Some(now) = now else {
        let why = error.unwrap_or_else(|| "they were not read".to_owned());
        checks.unknown(BACKUPS, format!("its heads cannot be read: {why}"));
        return;
    };
    let name = record.name();
    let mut behind = false;
    for (bucket, link) in record.links() {
        if link.heads().is_some_and(|then| then != now) {
            behind = true;
            checks.notice(BACKUPS, format!("its backup in {bucket} is behind"));
            checks.fix(BACKUPS, format!("layerfs backup create {name} {bucket}"));
        }
    }
    if !behind {
        checks.pass(BACKUPS, "no backup of it is behind");
    }
}

/// The shallow cut of the sqlite file at `path`, as this process or a
/// worker makes it: `{ found, compaction, unsupported, schema_error, heads,
/// heads_error }`, `compaction` the run its `<store>.compact-op` records
/// (`report::compact_status`), its heads read only when `heads` and the
/// schema check passed.
fn shallow(path: &Path, identity: &Identity, heads: bool) -> SdkValue {
    let mut fields = vec![("found", SdkValue::Bool(path.is_file()))];
    if !path.is_file() {
        return map(fields);
    }
    if let Some(run) = layerfs_store::compact::recorded_run(path) {
        fields.push(("compaction", run));
    }
    match diagnostic::sqlite_schema(path, identity) {
        Err(error) if error.code == "Unsupported" => {
            fields.push(("unsupported", SdkValue::Text(error.message)))
        }
        Err(error) => fields.push(("schema_error", SdkValue::Text(error.message))),
        Ok(()) if heads => match layerfs_bucket::backup::heads(path) {
            Ok(now) => fields.push((
                "heads",
                SdkValue::text(layerfs_bucket::bucket::pieces::hex(&now)),
            )),
            Err(why) => fields.push(("heads_error", SdkValue::Text(why))),
        },
        Ok(()) => {}
    }
    map(fields)
}

fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

/// The shallow cut through machine `machine`'s worker: one diagnostic
/// connection under the Store's budget, nothing resent.
fn remote(
    home: &Path,
    name: &str,
    machine: &str,
    identity: &Identity,
    heads: bool,
    budget: &Budget,
) -> Result<SdkValue, SdkRefusal> {
    use layerfs_machine::worker::{diagnostic_connect, Endpoint};
    let saved = layerfs_home::secrets::worker_secret(home, machine)?
        .ok_or_else(|| refused("Unavailable", "it records no worker endpoint"))?;
    let endpoint = Endpoint {
        address: saved.address,
        secret: saved.secret,
    };
    let deadline = Instant::now() + budget.limit(Duration::from_secs(5));
    let connection = diagnostic_connect(&endpoint, deadline)?;
    let timeout = budget.remaining().map_or(SdkValue::Null, |remaining| {
        SdkValue::Int(
            i64::try_from(remaining.as_millis())
                .unwrap_or(i64::MAX)
                .max(1),
        )
    });
    let args = SdkValue::map([
        ("store", SdkValue::text(name)),
        ("binding", SdkValue::Bytes(identity.binding.clone())),
        ("cursor", SdkValue::Bytes(identity.cursor_key.to_vec())),
        ("cut", SdkValue::text(CUT)),
        ("heads", SdkValue::Bool(heads)),
        ("timeout_ms", timeout),
    ]);
    connection.diagnose_store(args, budget.deadline(), &mut |_| Ok(()))
}

/// The worker's `store.diagnose` of Store `store` under its fixed root:
/// the shallow cut, `cut: schema`, which a request must name (an older
/// host's full scan is refused); caller paths and leases are not used.
pub(crate) fn in_worker(
    root: &Path,
    args: SdkValue,
    _events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
) -> CallOutcome {
    let name = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store required"))?;
    layerfs_home::names::StoreName::parse(name)?;
    if args.get_text("cut") != Some(CUT) {
        return Err(refused(
            "InvalidInput",
            "store.diagnose takes cut: schema (the shallow cut); the whole-Store scan left the doctors",
        ));
    }
    let bytes = |key: &str| match args.get(key) {
        Some(SdkValue::Bytes(bytes)) => Ok(bytes.clone()),
        _ => Err(refused("InvalidInput", format!("{key} bytes required"))),
    };
    let identity = Identity {
        kind: identity::Kind::Sqlite,
        binding: bytes("binding")?,
        cursor_key: bytes("cursor")?
            .try_into()
            .map_err(|_| refused("InvalidInput", "cursor width"))?,
    };
    let heads = args.get("heads") == Some(&SdkValue::Bool(true));
    Ok(shallow(&paths::store_file(root, name), &identity, heads))
}

/// A PostgreSQL Store from this computer, folded into one
/// `store.connection` (the failing step in its message and evidence), then
/// `store.identity` and `store.schema`, then from each machine.
fn postgresql(probe: &Probe<'_>, record: &StoreRecord, identity: &Identity, checks: &mut Checks) {
    let options = Options {
        quick: true,
        deadline: probe.budget.deadline(),
        cancelled: None,
    };
    let mut seen = Vec::new();
    let made = diagnostic::postgresql(record, identity, options, true, &mut |o| seen.push(o));
    match made {
        Err(error) if seen.is_empty() => {
            checks.fail(
                CONNECTION,
                format!("this computer cannot connect: {}", error.message),
            );
            checks.evidence(
                CONNECTION,
                SdkValue::map([("step", SdkValue::text("login"))]),
            );
        }
        _ => fold(record, &seen, checks),
    }
    reach(probe, record, checks);
}

/// The step id `o` gives, without its `store.connection.` prefix.
fn step(o: &Observation) -> &str {
    o.id.strip_prefix("store.connection.").unwrap_or(o.id)
}

/// Today's connection steps as the three frozen checks.
fn fold(record: &StoreRecord, seen: &[Observation], checks: &mut Checks) {
    let host = record
        .text("url")
        .and_then(|url| layerfs_store::postgresql::connect::target(url).ok())
        .map(|(host, port, _)| format!("{host}:{port}"))
        .unwrap_or_default();
    let bookkeeping = [
        "schema",
        "identity",
        "read",
        "tls",
        "cancellation.release",
        "close",
    ];
    let steps = || seen.iter().filter(|o| !bookkeeping.contains(&step(o)));
    let failed = steps().find(|o| o.outcome == Outcome::Fail);
    let unknown = steps().find(|o| o.outcome == Outcome::Unknown);
    let evidence = |o: &Observation| SdkValue::map([("step", SdkValue::text(step(o)))]);
    match (failed, unknown) {
        (Some(o), _) => {
            checks.fail(
                CONNECTION,
                format!(
                    "this computer is refused by the server at {host}: {}",
                    o.message
                ),
            );
            checks.evidence(CONNECTION, evidence(o));
        }
        (None, Some(o)) if step(o) == "cancelled" => checks.unknown(CONNECTION, o.message.clone()),
        (None, Some(o)) => {
            checks.fail(
                CONNECTION,
                format!(
                    "this computer cannot reach the server at {host}: {}",
                    o.message
                ),
            );
            checks.evidence(CONNECTION, evidence(o));
        }
        (None, None) if steps().any(|o| step(o) == "authentication") => checks.pass(
            CONNECTION,
            format!("this computer reaches the server at {host}"),
        ),
        (None, None) => {
            checks.unknown(CONNECTION, "the connection gave no answer");
            return;
        }
    }
    // in the frozen order: identity (or the read that could not make it),
    // then schema, whatever order the steps ran in
    for (id, of) in [
        (IDENTITY, &["identity", "read"][..]),
        (SCHEMA, &["schema"][..]),
    ] {
        for o in seen.iter().filter(|o| of.contains(&step(o))) {
            let message = o.message.clone();
            match o.outcome {
                Outcome::Pass => checks.pass(id, message),
                Outcome::Fail => checks.fail(id, message),
                Outcome::Unknown => checks.unknown(id, message),
                Outcome::Skipped => {}
            }
        }
    }
}

/// `store.connection` of PostgreSQL Store `record` from each machine that
/// has a Sandbox of it, and from `--machine`, as those machines dial it. A
/// machine that is not ready is not dialled.
fn reach(probe: &Probe<'_>, record: &StoreRecord, checks: &mut Checks) {
    let name = record.name();
    let mut machines: Vec<String> = crate::inventory::sandboxes(probe.home)
        .unwrap_or_default()
        .into_iter()
        .filter(|sandbox| {
            let project = sandbox.project().unwrap_or("");
            project.split_once('/').map(|(store, _)| store) == Some(name)
        })
        .map(|sandbox| sandbox.machine().to_owned())
        .collect();
    machines.extend(probe.options.machine.clone());
    // this computer dialled the Store's own address above; `native` is
    // probed again only for an address of its own
    machines.retain(|machine| machine != "native" || record.own_connection(machine).is_some());
    machines.sort();
    machines.dedup();
    for machine in &machines {
        let state = crate::store::machine_state(probe.home, machine)
            .unwrap_or_else(|_| State::broken(Reason::Missing, None));
        match state.blocks("machine", machine) {
            Some(State::Blocked(by)) => {
                checks.waits(CONNECTION, format!("machine {machine} is not ready"), by)
            }
            _ => {
                let mut sink = |_| {};
                let mut events = Events::new(&mut sink);
                match crate::store::check_from(probe.home, &mut events, record, machine).1 {
                    Ok(_) => {
                        checks.pass(CONNECTION, format!("machine {machine} reaches the server"))
                    }
                    Err(refusal) => checks.fail(CONNECTION, refusal.message),
                }
            }
        }
        checks.from(CONNECTION, machine);
    }
}
