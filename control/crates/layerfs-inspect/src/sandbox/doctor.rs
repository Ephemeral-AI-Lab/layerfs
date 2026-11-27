//! The `sandbox` kind (call surface 2.0): one Sandbox's checks over its
//! saved route, without normal host dispatch, then one node per Workspace
//! its daemon lists or its folder records (`workspace_target`).
//!
//! Its records checks always run. A machine that is not ready keeps every
//! live check from being made: the Sandbox is one `not_checked` entry with
//! `blocked_by` its machine. An operation that holds it is one with
//! `reason` `busy`. Otherwise the live checks run in order, each only when
//! the one it needs did not fail; a daemon that is not seen ready blocks its
//! Workspaces (`blocked_by` the thing at the root: the Sandbox, or its
//! machine's worker) and none of their checks is tried.
use super::super::doctor::runner::Probe;
use super::super::doctor::report::{NoVerdict, Node, NotChecked};
use super::super::doctor::budget::{Budget, Checks};
use super::workspace::{self, Listing, Seen};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::home::{paths, read_record, RECORD_VERSION};
use layerfs_home::names::{self, MachineName, SandboxName};
use layerfs_home::op::Standing;
use layerfs_home::outcome::shell_word;
use layerfs_home::state::{BlockedBy, Reason, State};
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox::state::remove_fix;
use layerfs_store::record::{StoreRecord, StoreRef};
use std::path::Path;

#[path = "channel.rs"]
mod channel;
#[path = "remote.rs"]
mod remote;
#[path = "runtime.rs"]
mod runtime;

pub(crate) use remote::worker_diagnose;

/// How the doctor reaches a Sandbox's daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// No live check was made.
    None,
    /// Directly from this process: its machine's calls run here.
    Local,
    /// Through its saved forward.
    Forward,
    /// Through its machine's worker, which makes the live checks.
    Worker,
}

/// What one Sandbox's checks observed beyond their rows.
#[derive(Default)]
struct Observed {
    /// What outside it keeps its live checks from being made.
    waits: Option<BlockedBy>,
    /// An operation of a running process holds it.
    busy: bool,
    /// What its records say blocks the things on it.
    gate: Option<BlockedBy>,
    /// Its machine's worker answered no report this build reads.
    lost: bool,
    /// How its daemon's Workspace list went.
    listing: Listing,
    /// Its Workspaces.
    workspaces: Vec<Seen>,
}

/// The Sandbox's state from its records and its machine's records: the
/// Sandbox package's one derivation (`layerfs_sandbox::state::state`), so
/// the doctor and `sandbox get` agree. Its machine's record that cannot be
/// read is `sandbox.machine_reference`'s finding, not a state here.
fn recorded_state(home: &Path, record: &SandboxRecord) -> State {
    layerfs_sandbox::state::state(home, record).unwrap_or(State::Ready)
}

/// The `sandbox` kind's run: its checks into `node`, then its Workspaces'
/// nodes as `node`'s children, unless what it found blocks them.
pub(super) fn doctor(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::default();
    let mut seen = Observed::default();
    let (route, machine) = diagnose(probe, &mut checks, &mut seen);
    checks.fill(node, probe.checks, probe.budget.expired());
    let at = |check: &str| probe.checks.iter().position(|listed| listed.id == check);
    node.rows.sort_by_key(|row| at(&row.check));
    let whole = |node: &mut Node, blocked_by: Option<BlockedBy>, reason: Option<NoVerdict>| {
        node.not_checked.push(NotChecked {
            thing: probe.thing.clone(),
            check: None,
            blocked_by: blocked_by.clone(),
            reason,
        });
        node.blocked_by = blocked_by;
        node.reason = reason;
    };
    if let Some(by) = seen.waits {
        whole(node, Some(by.clone()), None);
        node.gate = Some(by);
        return;
    }
    if seen.busy {
        whole(node, None, Some(NoVerdict::Busy));
        node.gate = Some(on(name, "busy"));
        return;
    }
    if seen.lost {
        whole(node, None, Some(NoVerdict::Lost));
    }
    node.gate = (node.gate.take())
        .or(seen.gate)
        .or_else(|| gate(&checks, name, machine.as_deref(), route));
    if node.gate.is_some() {
        return;
    }
    if let (Listing::NoVerdict(reason), true) = (seen.listing, seen.workspaces.is_empty()) {
        // no Workspace can be named: the Sandbox answers for its list
        if node.reason.is_none() {
            whole(node, None, Some(reason));
        }
        return;
    }
    node.children = workspace::nodes(name, &seen.workspaces, seen.listing);
}

/// Evidence naming only the cause word of a finding.
fn cause(word: &str) -> SdkValue {
    SdkValue::map([("cause", SdkValue::text(word))])
}

/// `blocked_by` the Sandbox `sandbox`, for `reason`.
fn on(sandbox: &str, reason: &'static str) -> BlockedBy {
    BlockedBy {
        thing: "sandbox",
        handle: sandbox.to_owned(),
        reason,
    }
}

/// What blocks the Workspaces of a Sandbox whose daemon was not seen
/// ready: the first live check that failed or gave no verdict, by the
/// thing at its root. None when its daemon is ready, or when nothing was
/// tried (its Workspaces then give no verdict either).
fn gate(checks: &Checks, sandbox: &str, machine: Option<&str>, route: Route) -> Option<BlockedBy> {
    if matches!(checks.outcome("sandbox.daemon_ready"), Some(("pass", _))) {
        return None;
    }
    let of = |id: &str| checks.outcome(id);
    let bad = |id: &str| matches!(of(id), Some(("fail" | "unknown", _)));
    let worker = |reason| {
        machine.map(|machine| BlockedBy {
            thing: "machine",
            handle: machine.to_owned(),
            reason,
        })
    };
    if matches!(of("sandbox.runtime"), Some(("fail", Some("error")))) {
        return Some(on(sandbox, "lost"));
    }
    if matches!(of("sandbox.route"), Some(("blocked", _))) {
        return worker("not_answering");
    }
    if bad("sandbox.route") {
        return match route {
            Route::Worker => worker("not_answering"),
            _ => Some(on(sandbox, "unreachable")),
        };
    }
    if bad("sandbox.forward_process") {
        return Some(on(sandbox, "unreachable"));
    }
    if bad("sandbox.worker_compatibility") {
        return worker("outdated");
    }
    if bad("sandbox.daemon_keys") {
        return Some(on(sandbox, "missing"));
    }
    if bad("sandbox.daemon_connect") {
        return Some(on(sandbox, "not_answering"));
    }
    if bad("sandbox.daemon_auth") {
        let reason = match of("sandbox.daemon_auth") {
            Some(("fail", _)) => "failed",
            _ => "not_answering",
        };
        return Some(on(sandbox, reason));
    }
    if bad("sandbox.daemon_instance") {
        return Some(on(sandbox, "lost"));
    }
    match of("sandbox.daemon_ready") {
        Some(("fail", Some("error"))) => Some(on(sandbox, "failed")),
        Some(("fail", _)) => Some(on(sandbox, "busy")),
        Some(("unknown", _)) => Some(on(sandbox, "not_answering")),
        _ => None,
    }
}

/// The Sandbox's checks: its records, then what its state lets run. Its
/// route and machine, when its record names them.
fn diagnose(
    probe: &Probe<'_>,
    checks: &mut Checks,
    seen: &mut Observed,
) -> (Route, Option<String>) {
    let (home, name) = (probe.home, probe.thing.name.as_str());
    let Some(record) = read(home, name, checks) else {
        return (Route::None, None);
    };
    let machine = Some(record.machine().to_owned());
    references(home, &record, checks);
    operation(&record, checks);
    let state = match probe.blocked {
        Some(by) => State::Blocked(by.clone()),
        None => recorded_state(home, &record),
    };
    match state {
        State::Blocked(by) => {
            seen.waits = Some(by);
            return (Route::None, machine);
        }
        State::Busy { .. } => {
            seen.busy = true;
            return (Route::None, machine);
        }
        State::Broken {
            reason: Reason::Missing,
            ..
        } => {
            let (message, runtime) = match record.machine_stopped() {
                true => ("its runtime was stopped", "stopped"),
                false => (
                    "it has no runtime: its create never recorded one",
                    "none_recorded",
                ),
            };
            checks.fail("sandbox.runtime", message);
            checks.fix("sandbox.runtime", remove_fix(name));
            checks.evidence(
                "sandbox.runtime",
                SdkValue::map([("runtime", SdkValue::text(runtime))]),
            );
            seen.gate = Some(on(name, "missing"));
            return (Route::None, machine);
        }
        _ => {}
    }
    let route = if record.daemon().and_then(|d| d.forward()).is_some() {
        if runtime::forward(home, &record, checks) {
            local(home, &record, probe.budget, checks, true, seen);
        }
        Route::Forward
    } else if layerfs_machine::runs_here(home, record.machine()).unwrap_or(false) {
        checks.pass(
            "sandbox.route",
            "its daemon runs on this computer and is dialled directly",
        );
        local(home, &record, probe.budget, checks, false, seen);
        Route::Local
    } else {
        remote::remote(home, &record, probe.budget, checks, seen);
        Route::Worker
    };
    (route, machine)
}

fn read(home: &Path, name: &str, checks: &mut Checks) -> Option<SandboxRecord> {
    const ID: &str = "sandbox.record";
    if SandboxName::parse(name).is_err() {
        checks.fail(ID, "its name is not a Sandbox name");
        checks.evidence(ID, cause("invalid_name"));
        return None;
    }
    match read_record(&paths::sandbox_record(home, name)) {
        Ok(Some(value)) if value.get_int("v") == Some(RECORD_VERSION) => {
            checks.pass(ID, "its record parses and has a supported version");
            Some(SandboxRecord::from_record(name, value))
        }
        Ok(None) => {
            checks.fail(ID, "it has no record");
            checks.evidence(ID, cause("absent"));
            None
        }
        Ok(Some(value)) => {
            checks.fail(
                ID,
                "its record cannot be read, is malformed or has an unsupported version",
            );
            checks.evidence(
                ID,
                SdkValue::map([
                    ("cause", SdkValue::text("unsupported_version")),
                    (
                        "v",
                        value.get_int("v").map_or(SdkValue::Null, SdkValue::Int),
                    ),
                    ("expected_v", SdkValue::Int(RECORD_VERSION)),
                ]),
            );
            None
        }
        Err(_) => {
            checks.fail(
                ID,
                "its record cannot be read, is malformed or has an unsupported version",
            );
            checks.evidence(ID, cause("unreadable"));
            None
        }
    }
}

/// Whether the record at `file` exists with a supported version.
fn readable(file: &Path) -> Option<SdkValue> {
    match read_record(file) {
        Ok(Some(value)) if value.get_int("v") == Some(RECORD_VERSION) => Some(value),
        _ => None,
    }
}

/// `sandbox.machine_reference`, `.store_reference`, `.project_reference`
/// and `.store_placement`: the records it names. Each failure is fixed by
/// removing the Sandbox.
fn references(home: &Path, record: &SandboxRecord, checks: &mut Checks) {
    let sandbox = record.name();
    let fail = |checks: &mut Checks, id: &str, message: String, evidence: SdkValue| {
        checks.fail(id, message);
        checks.fix(id, remove_fix(sandbox));
        checks.evidence(id, evidence);
    };
    let named = |cause: &str, key: &str, value: &str| {
        SdkValue::map([
            ("cause", SdkValue::text(cause)),
            (key, SdkValue::text(value)),
        ])
    };
    let machine = record.machine();
    const MACHINE: &str = "sandbox.machine_reference";
    if MachineName::parse(machine).is_err() {
        fail(
            checks,
            MACHINE,
            format!("its machine `{machine}` is not a machine name"),
            named("invalid_name", "machine", machine),
        );
    } else if layerfs_home::names::BUILT_IN.contains(&machine) {
        checks.pass(MACHINE, "its machine is built in");
    } else if readable(&paths::machine_record(home, machine)).is_some() {
        checks.pass(MACHINE, format!("its machine {machine} has a record"));
    } else {
        fail(
            checks,
            MACHINE,
            format!("its machine {machine} has no readable record"),
            named("no_readable_record", "machine", machine),
        );
    }
    let Some((store, project)) = record.project().and_then(|p| names::project(p).ok()) else {
        let message = "its record names no project, or a malformed one".to_owned();
        let evidence = cause("malformed_or_absent");
        fail(checks, "sandbox.project_reference", message, evidence);
        return;
    };
    const STORE: &str = "sandbox.store_reference";
    let Some(value) = readable(&paths::store_record(home, store)) else {
        fail(
            checks,
            STORE,
            format!("its Store {store} has no readable record"),
            named("no_readable_record", "store", store),
        );
        return;
    };
    let placed = StoreRecord::from_record(store, value);
    match placed.store_ref() {
        Ok(_) => checks.pass(STORE, format!("its Store {store} has a readable record")),
        Err(_) => fail(
            checks,
            STORE,
            format!("its Store {store}'s record names no kind this build reads"),
            named("unknown_kind", "store", store),
        ),
    }
    const PROJECT: &str = "sandbox.project_reference";
    match readable(&paths::project_record(home, store, project)) {
        Some(_) => checks.pass(
            PROJECT,
            format!("its project {store}/{project} has a record"),
        ),
        None => fail(
            checks,
            PROJECT,
            format!("its project {store}/{project} has no record"),
            named("no_record", "project", &format!("{store}/{project}")),
        ),
    }
    const PLACEMENT: &str = "sandbox.store_placement";
    match placed.store_ref() {
        Ok(StoreRef::Sqlite { machine: on, .. }) if on == machine => {
            checks.pass(PLACEMENT, "its sqlite Store is on its machine")
        }
        Ok(StoreRef::Sqlite { machine: on, .. }) => {
            checks.fail(
                PLACEMENT,
                format!(
                    "its sqlite Store {store} is on machine {on}, not on its machine {machine}"
                ),
            );
            // findings.md: move the Store to the Sandbox's machine
            checks.fix(
                PLACEMENT,
                format!(
                    "layerfs store move {} {}",
                    shell_word(store),
                    shell_word(machine)
                ),
            );
            checks.evidence(
                PLACEMENT,
                SdkValue::map([
                    ("store", SdkValue::text(store)),
                    ("store_machine", SdkValue::text(on.as_str())),
                    ("sandbox_machine", SdkValue::text(machine)),
                ]),
            );
        }
        Ok(StoreRef::Postgres { .. }) => {
            checks.pass(PLACEMENT, "its PostgreSQL Store is on no machine")
        }
        Err(_) => {}
    }
}

/// `sandbox.operation`: no operation of an ended owner on its record.
fn operation(record: &SandboxRecord, checks: &mut Checks) {
    const ID: &str = "sandbox.operation";
    let Some(op) = record.op() else {
        checks.pass(ID, "no operation is left on its record");
        return;
    };
    let what = format!("{} (phase {})", op.name(), op.phase());
    let seen = |standing: &str| {
        SdkValue::map([
            ("op", SdkValue::text(op.name())),
            ("phase", SdkValue::text(op.phase())),
            ("standing", SdkValue::text(standing)),
        ])
    };
    match op.standing() {
        Standing::Running => checks.pass(ID, format!("its {what} has a running owner")),
        Standing::Stale => {
            // no fix: it holds nothing, and the next call on the Sandbox clears it
            checks.notice(
                ID,
                format!("its record keeps the {what} of a process that ended; it holds nothing"),
            );
            checks.evidence(ID, seen("stale"));
        }
        Standing::Unresolved => {
            checks.fail(
                ID,
                format!("the outcome of its {what} is not known: its process ended"),
            );
            checks.fix(ID, remove_fix(record.name()));
            checks.evidence(ID, seen("unresolved"));
        }
    }
}

/// The live checks of a daemon on this kernel, or behind a forward: its
/// runtime (not behind a forward), its saved keys, its daemon and its
/// Workspace list, and whether its saved keys changed meanwhile. A
/// worker's report is this too, from the worker's own root.
fn local(
    home: &Path,
    record: &SandboxRecord,
    budget: &Budget,
    checks: &mut Checks,
    forwarded: bool,
    seen: &mut Observed,
) {
    let name = record.name();
    if !forwarded && !runtime::local(home, record, checks) {
        return;
    }
    if !forwarded {
        super::super::machine::network::sandbox(record, checks);
    }
    let directory = record.dir(home);
    let modified = || {
        std::fs::metadata(directory.join("control.json"))
            .and_then(|m| m.modified())
            .ok()
    };
    let before = modified();
    const KEYS: &str = "sandbox.daemon_keys";
    let keys = match layerfs_home::secrets::daemon_keys(&directory) {
        Ok(Some(keys)) => keys,
        _ => {
            checks.fail(
                KEYS,
                "its daemon's saved keys are missing or cannot be read",
            );
            checks.fix(KEYS, remove_fix(name));
            checks.evidence(KEYS, cause("missing_or_unreadable"));
            return;
        }
    };
    let forward_endpoint = (record.daemon())
        .and_then(|d| d.forward())
        .and_then(|f| f.address());
    let saved_endpoint = keys
        .get_text("endpoint")
        .and_then(|s| s.parse::<std::net::SocketAddr>().ok());
    if forwarded && forward_endpoint != saved_endpoint {
        checks.fail(KEYS, "its daemon's saved endpoint is not its forward's");
        checks.fix(KEYS, remove_fix(name));
        checks.evidence(
            KEYS,
            SdkValue::map([
                ("cause", SdkValue::text("endpoint_not_forwards")),
                (
                    "forward_address",
                    SdkValue::opt_text(forward_endpoint.map(|a| a.to_string())),
                ),
                (
                    "saved_address",
                    SdkValue::opt_text(saved_endpoint.map(|a| a.to_string())),
                ),
            ]),
        );
        return;
    }
    let (listing, rows) = channel::observe(name, &keys, budget, checks);
    const OBSERVATION: &str = "sandbox.observation";
    match before == modified() {
        true => checks.pass(
            OBSERVATION,
            "its daemon's saved keys did not change while read",
        ),
        false => checks.unknown(OBSERVATION, "its daemon's saved keys changed while read"),
    }
    seen.listing = listing;
    let recorded = workspace::recorded(&directory);
    seen.workspaces = match listing {
        Listing::NotAsked => Vec::new(),
        _ => workspace::seen(name, &recorded, rows),
    };
}
