//! A Store's doctor (call surface 2.0, `checks.store`): its record, the
//! operation on it, then the shallow cut where it lives (C14), which
//! reaches it and stops after the schema check, reading no data. A sqlite
//! Store is reached through its machine (this process for `native`, the
//! worker's `store.diagnose` with `cut: schema` elsewhere), which also
//! reads its heads when it has a bucket link; a PostgreSQL Store from this
//! computer, then, when this computer reached it, once more from each
//! machine attached to it, or from `--machine` alone (owner, 2026-11-24),
//! one `store.connection` each with `from`; every reach is recorded
//! (`layerfs_store::observed`).
//! `store.operation` reads the home's record and, for a sqlite Store, the
//! compaction record beside its file through its machine (the shallow cut
//! answers it, spec v19). Under a machine that is not ready only
//! `store.record` is made: the rest wait on the machine.
use super::frame::{CheckId, Probe};
use super::report::Node;
use super::target::{Budget, Checks};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::helpers::map_of;
use layerfs_home::home::paths;
use layerfs_home::op::Standing;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, Outcome as CallOutcome};
use layerfs_home::state::{Reason, State};
use layerfs_store::diagnostic::{self, Observation, Options, Outcome};
use layerfs_store::identity::{self, Identity};
use layerfs_store::intent::clearing as clearing_call;
use layerfs_store::place::{self, Executor};
use layerfs_store::record::{StoreRecord, StoreRef};
use std::path::Path;
use std::time::{Duration, Instant};

const RECORD: &str = "store.record";
const OPERATION: &str = "store.operation";
const CONNECTION: &str = "store.connection";
const IDENTITY: &str = "store.identity";
const SCHEMA: &str = "store.schema";
const BACKUPS: &str = "store.backups";
const MOUNT_HOLDERS: &str = "store.mount_holders";
const GC_AUDIT: &str = "store.gc_audit";

/// A Store's checks, in the frozen order.
pub(super) const CHECKS: &[CheckId] = &[
    CheckId::own(RECORD),
    CheckId::own(OPERATION),
    CheckId::own(CONNECTION),
    CheckId::own(IDENTITY),
    CheckId::own(SCHEMA),
    CheckId::own(BACKUPS),
    CheckId::own(MOUNT_HOLDERS),
    CheckId::own(GC_AUDIT),
];

/// The worker's shallow cut, the one `cut` its `store.diagnose` takes.
const CUT: &str = "schema";

/// Checks Store `probe.thing` into `node`: its state from the home's
/// records (`layerfs_store::state`, with the machine family's derivation
/// handed in) says whether it waits on its machine, and what blocks the
/// Store blocks its projects too.
pub(super) fn run(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::default();
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
        // the fix its `broken` `unsupported` state gives (V20-8a)
        checks.fix(RECORD, layerfs_store::record::unread_fix(name));
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
            checks.fix(
                OPERATION,
                format!("layerfs store compact {name}", name = shell_word(name)),
            );
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
    // a file another writer holds: its id and schema were not read, so
    // both are `busy` (`not_checked`), not failed
    if let Some(why) = text("busy") {
        checks.busy(IDENTITY, why.clone());
        checks.busy(SCHEMA, why);
        return;
    }
    // a file of another Store is `store.identity`, and its schema is then
    // not checked; a file of this Store that does not open is `store.schema`
    // (conformance R4 F4)
    if let Some(why) = text("identity_error") {
        checks.fail(
            IDENTITY,
            format!("its file records another Store's id than its record: {why}"),
        );
        return;
    }
    // a file of an older schema version: outdated, with the call that
    // clears it (`state::outdated_fix`)
    let format = found.get("format");
    if let Some(why) = text("outdated") {
        checks.pass(IDENTITY, "the id in its file is the id in its record");
        checks.fail(SCHEMA, format!("its file is of an older format: {why}"));
        let reason = ("reason", SdkValue::text(Reason::Outdated.word()));
        checks.evidence(SCHEMA, with_format(vec![reason], format));
        checks.fix(
            SCHEMA,
            layerfs_store::state::outdated_fix(name, false, &why),
        );
        return;
    }
    if let Some(why) = text("schema_error") {
        checks.pass(IDENTITY, "the id in its file is the id in its record");
        checks.fail(
            SCHEMA,
            format!("its file does not open with the schema this build reads: {why}"),
        );
        checks.evidence(SCHEMA, with_format(Vec::new(), format));
        return;
    }
    checks.pass(IDENTITY, "the id in its file is the id in its record");
    checks.pass(SCHEMA, "its schema is the one this build reads");
    checks.evidence(SCHEMA, with_format(Vec::new(), format));
    // the live mount admissions and one bounded page of the retained done
    // audit: reported as evidence, never released or settled by a doctor
    match found.get("mount_holders") {
        Some(rows) => {
            super::mount_holders::report(probe.home, name, false, MOUNT_HOLDERS, Some(rows), checks)
        }
        None => checks.unknown(MOUNT_HOLDERS, "the diagnostic did not report mount holders"),
    }
    match found.get("gc_audit") {
        Some(page) if page.get("items").is_some() => {
            super::mount_holders::audit_result(Ok(page.clone()), checks)
        }
        _ => checks.unknown(GC_AUDIT, "the diagnostic did not report the done audit"),
    }
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
            checks.fix(
                BACKUPS,
                format!(
                    "layerfs backup create {name} {bucket}",
                    name = shell_word(name),
                    bucket = shell_word(&bucket)
                ),
            );
        }
    }
    if !behind {
        checks.pass(BACKUPS, "no backup of it is behind");
    }
}

/// The shallow cut of the sqlite file at `path`, as this process or a
/// worker makes it: `{ found, compaction, format, unsupported, busy,
/// identity_error, outdated, schema_error, heads, heads_error }` (`busy`:
/// another writer holds the file; `outdated`: it is of an older schema
/// version), `compaction` the run its `<store>.compact-op` records
/// (`report::compact_status`), `format` its format from its header
/// ([`format_evidence`], or `{ error }`), its heads read only when `heads`
/// and the schema check passed.
fn shallow(path: &Path, identity: &Identity, heads: bool) -> SdkValue {
    let mut fields = vec![("found", SdkValue::Bool(path.is_file()))];
    if !path.is_file() {
        return map_of(fields);
    }
    if let Some(run) = layerfs_store::compact::recorded_run(path) {
        fields.push(("compaction", run));
    }
    // the format from its header alone, whatever this build makes of it
    fields.push((
        "format",
        match layerfs_persistence::upgrade::sqlite_format_evidence(path) {
            Ok(evidence) => format_evidence(&evidence),
            Err(error) => SdkValue::map([("error", SdkValue::text(upgrade_error(&error)))]),
        },
    ));
    match diagnostic::sqlite_schema(path, identity) {
        Err(error) if error.code == "Unsupported" => {
            fields.push(("unsupported", SdkValue::Text(error.message)))
        }
        // another writer holds the file: neither its id nor its schema
        // could be read, which says nothing wrong of either
        Err(error) if error.code == "Busy" => fields.push(("busy", SdkValue::Text(error.message))),
        Err(error) => match diagnostic::sqlite_identity(path, identity) {
            Ok(false) => fields.push(("identity_error", SdkValue::Text(error.message))),
            _ if error.code == layerfs_store::state::OLD_STORE => {
                fields.push(("outdated", SdkValue::Text(error.message)))
            }
            _ => fields.push(("schema_error", SdkValue::Text(error.message))),
        },
        Ok(()) if heads => match layerfs_bucket::backup::heads(path) {
            Ok(now) => fields.push((
                "heads",
                SdkValue::text(layerfs_bucket::bucket::pieces::hex(&now)),
            )),
            Err(why) => fields.push(("heads_error", SdkValue::Text(why))),
        },
        Ok(()) => {}
    }
    // the read-only mount holder view and the retained done audit: the
    // doctor reports them beside the schema, and changes no row
    fields.push((
        "mount_holders",
        match diagnostic::mount_holders(path, identity) {
            Ok(rows) => rows,
            Err(error) => SdkValue::map([("error", SdkValue::text(error.message))]),
        },
    ));
    fields.push((
        "gc_audit",
        match diagnostic::done_audit(path, identity, 0, 16) {
            Ok(page) => page,
            Err(error) => SdkValue::map([("error", SdkValue::text(error.message))]),
        },
    ));
    map_of(fields)
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
/// the shallow cut, `cut: schema`, or one project's data, `cut: project`
/// (`project doctor --full`), which a request must name (an older host's
/// full scan is refused); caller paths and leases are not used.
pub(crate) fn in_worker(
    root: &Path,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
) -> CallOutcome {
    let name = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store required"))?;
    layerfs_home::names::StoreName::parse(name)?;
    let cut = args.get_text("cut");
    if cut != Some(CUT) && cut != Some(super::project_target::CUT) {
        return Err(refused(
            "InvalidInput",
            "store.diagnose takes cut: schema (the shallow cut) or cut: project (one project's data); the whole-Store scan left the doctors",
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
    if cut == Some(super::project_target::CUT) {
        return super::project_target::in_worker(root, &args, &identity, events);
    }
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
    let reached = match made {
        Err(error) if seen.is_empty() => {
            checks.fail(
                CONNECTION,
                format!("this computer cannot connect: {}", error.message),
            );
            checks.evidence(
                CONNECTION,
                SdkValue::map([("step", SdkValue::text("login"))]),
            );
            // a refusal before any dial carries its own fix: a password
            // this home does not hold names `store connect` again
            fix_of(&error, checks);
            let failed = layerfs_store::observed::Reach::failed(&error.message);
            layerfs_store::observed::record(probe.home, record.name(), None, &failed);
            false
        }
        // a server that said nothing within the handshake limit is not
        // dialled a second time: its silence is the reach recorded
        _ => match fold(record, &seen, probe.budget.expired(), checks) {
            Some(silent) => {
                let failed = layerfs_store::observed::Reach::failed(&silent);
                layerfs_store::observed::record(probe.home, record.name(), None, &failed);
                false
            }
            None => {
                postgresql_format(record, checks);
                here(probe.home, record)
            }
        },
    };
    // the machines dial the same URL with the same login: when this
    // computer was not let in, each would only repeat its failure
    if reached {
        reach(probe, record, checks);
    }
}

/// Records this computer's reach of PostgreSQL Store `record` (spec v23
/// V23-9): the check an attached machine is checked by, so the
/// observation carries its latency and the server's version. Whether it
/// was reached.
fn here(home: &Path, record: &StoreRecord) -> bool {
    use layerfs_store::postgresql::{check, connect};
    let checked = connect::selection(record).and_then(|pg| check::check(record, &pg));
    let seen = layerfs_store::observed::Reach::of(&checked);
    layerfs_store::observed::record(home, record.name(), None, &seen);
    checked.is_ok()
}

/// The step id `o` gives, without its `store.connection.` prefix.
fn step(o: &Observation) -> &str {
    o.id.strip_prefix("store.connection.").unwrap_or(o.id)
}

/// Today's connection steps as the three frozen checks. A connection the
/// handshake limit cut while the Store's budget had time left
/// (`expired` false) is a server that did not answer: `store.connection`
/// fails, and its message is returned.
fn fold(
    record: &StoreRecord,
    seen: &[Observation],
    expired: bool,
    checks: &mut Checks,
) -> Option<String> {
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
        (None, Some(o)) if step(o) == "cancelled" && expired => {
            checks.unknown(CONNECTION, o.message.clone())
        }
        (None, Some(o)) if step(o) == "cancelled" => {
            let limit = diagnostic::POSTGRESQL_HANDSHAKE.as_secs();
            let message = format!(
                "this computer cannot reach the server at {host}: it did not answer within {limit} s (connect and login)"
            );
            checks.fail(CONNECTION, message.clone());
            let step = SdkValue::map([("step", SdkValue::text("handshake"))]);
            checks.evidence(CONNECTION, step);
            return Some(message);
        }
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
            checks.lost(CONNECTION, "the connection gave no answer");
            return None;
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
    None
}

/// `store.connection` of PostgreSQL Store `record` from each machine
/// attached to it, or from `--machine` alone (the call refuses one that is
/// not attached), as those machines dial it; each result is what was seen
/// of its reach (spec v23 V23-9). A machine that is not ready is not
/// dialled.
fn reach(probe: &Probe<'_>, record: &StoreRecord, checks: &mut Checks) {
    let machines = match &probe.options.machine {
        Some(machine) => vec![machine.clone()],
        None => record.attached(),
    };
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
                let checked = crate::store::check_from(probe.home, &mut events, record, machine).1;
                let seen = layerfs_store::observed::Reach::of(&checked);
                layerfs_store::observed::record(probe.home, record.name(), Some(machine), &seen);
                match checked {
                    Ok(_) => {
                        checks.pass(CONNECTION, format!("machine {machine} reaches the server"))
                    }
                    Err(refusal) => {
                        checks.fail(CONNECTION, refusal.message.clone());
                        fix_of(&refusal, checks);
                    }
                }
            }
        }
        checks.from(CONNECTION, machine);
    }
}

/// The last `store.connection` observation with `refusal`'s `fix`, when
/// it carries one.
fn fix_of(refusal: &SdkRefusal, checks: &mut Checks) {
    if let Some(fix) = refusal.detail.get_text("fix") {
        checks.fix(CONNECTION, fix);
    }
}

/// `store.schema`'s evidence (#804 §2): `{ version, latest, status, raw,
/// variant }`, the format with the raw encoding (`user_version`, or
/// `history_meta.schema_version`) and the recorded-source variant.
fn format_evidence(evidence: &layerfs_persistence::upgrade::FormatEvidence) -> SdkValue {
    let format = &evidence.format;
    SdkValue::map([
        (
            "version",
            format.version.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("latest", SdkValue::Int(format.latest)),
        ("status", SdkValue::text(format.status.word())),
        ("raw", SdkValue::Int(evidence.raw)),
        ("variant", SdkValue::opt_text(evidence.variant.clone())),
    ])
}

/// The engine's words for a format it could not read.
fn upgrade_error(error: &layerfs_persistence::upgrade::UpgradeError) -> String {
    use layerfs_persistence::upgrade::UpgradeError;
    match error {
        UpgradeError::InUse(why)
        | UpgradeError::Busy(why)
        | UpgradeError::Unsupported(why)
        | UpgradeError::Failed(why)
        | UpgradeError::Unknown(why) => why.clone(),
        UpgradeError::Cancelled => layerfs_store::upgrade::CANCELLED.to_owned(),
    }
}

/// `fields` with the shallow cut's `format` (`{ version, latest, status,
/// raw, variant }` or `{ error }`) as `format` beside them.
fn with_format(mut fields: Vec<(&str, SdkValue)>, format: Option<&SdkValue>) -> SdkValue {
    if let Some(format) = format {
        fields.push(("format", format.clone()));
    }
    map_of(fields)
}

/// PostgreSQL Store `record`'s format as `store.schema`'s evidence: one
/// connect and one `SELECT` (`postgresql_format_evidence`).
fn postgresql_format(record: &StoreRecord, checks: &mut Checks) {
    let url = layerfs_store::record::url(record);
    let schema = layerfs_store::upgrade::postgresql_schema(record);
    let format = match (url, schema) {
        (Ok(url), Ok(schema)) => {
            match layerfs_persistence::upgrade::postgresql_format_evidence(&url, &schema) {
                Ok(evidence) => format_evidence(&evidence),
                Err(error) => SdkValue::map([("error", SdkValue::text(upgrade_error(&error)))]),
            }
        }
        (Err(error), _) | (_, Err(error)) => {
            SdkValue::map([("error", SdkValue::text(error.message))])
        }
    };
    checks.evidence(SCHEMA, with_format(Vec::new(), Some(&format)));
}
