//! A running Sandbox's daemon control capacity (#540): the
//! `backend.daemon.connection_failures` read model, queried best-effort for
//! each Sandbox whose record says it runs and whose daemon endpoint is
//! known. Nothing is started: a daemon that does not answer is a warning
//! naming its endpoint and the operation, never an error, because the
//! doctor changes nothing and a stopped daemon is a normal state.
//! Two more warnings come from the model itself: retained connection
//! failures, and a pool with no vacant control slot.
use super::report::Findings;
use super::scan::Scanned;
use layerfs_bridge::control::{CapacityModel, SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused};
use layerfs_home::secrets;
use layerfs_sandbox::daemon::operator;
use layerfs_sandbox::direct;
use layerfs_sandbox::record::SandboxRecord;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// The daemon's operator read model of control admission and retained
/// connection failures.
const OPERATION: &str = layerfs_bridge::control::CAPACITY_OPERATION;

/// How long one daemon query may take: connecting, its Hello and its one
/// exchange. Short on purpose: the doctor must not stall behind a daemon
/// that is unreachable, as an ordinary 60 s admission wait would.
const WAIT: Duration = Duration::from_secs(2);

/// Queries the daemon of every running Sandbox whose endpoint is known.
pub(super) fn check(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    for sandbox in &scanned.sandboxes {
        // a Sandbox whose create never recorded an id is gone; a creating,
        // closing or unresolved one has no steady daemon to ask
        if sandbox.id().is_none() || sandbox.state(false) != "running" {
            continue;
        }
        let name = sandbox.name();
        let Some(endpoint) = endpoint(home, sandbox) else {
            // no known endpoint: the record does not say where its daemon
            // listens, and the doctor starts nothing to find out
            continue;
        };
        match probe_bounded(home, sandbox, endpoint) {
            Ok(value) => capacity(findings, name, endpoint, &CapacityModel::parse(&value)),
            Err(refusal) => unanswered(findings, name, endpoint, &refusal),
        }
    }
}

/// The address of a running Sandbox's daemon when its record names one to
/// this host: a forwarded machine's forward, else the endpoint the start
/// recorded in `control.json`. Nothing is observed or started.
fn endpoint(home: &Path, sandbox: &SandboxRecord) -> Option<SocketAddr> {
    if let Some(forward) = sandbox.daemon().and_then(direct::forward_of) {
        return Some(forward);
    }
    let keys = secrets::daemon_keys(&paths::sandbox_dir(home, sandbox.name()))
        .ok()
        .flatten()?;
    keys.get_text("endpoint")?.parse().ok()
}

/// [`probe`] under a hard wall: a daemon that accepts a connection and then
/// says nothing must not hold the doctor past [`WAIT`].
fn probe_bounded(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<SdkValue, SdkRefusal> {
    let home = home.to_owned();
    let sandbox = sandbox.clone();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(probe(&home, &sandbox, endpoint));
    });
    match received.recv_timeout(WAIT) {
        Ok(result) => result,
        Err(_) => Err(refused(
            "Unavailable",
            format!("the daemon did not answer within {}s", WAIT.as_secs()),
        )),
    }
}

/// One daemon query: dial its recorded endpoint with its keys for at most
/// [`WAIT`], then read the capacity model once.
fn probe(
    home: &Path,
    sandbox: &SandboxRecord,
    endpoint: SocketAddr,
) -> Result<SdkValue, SdkRefusal> {
    let keys = secrets::daemon_keys(&paths::sandbox_dir(home, sandbox.name()))?
        .ok_or_else(|| refused("NoSuchSandbox", "the Sandbox has no daemon keys"))?;
    let dial = layerfs_sandbox::daemon::pool::Dial {
        endpoint,
        secret: secrets::key(&keys, "secret")?,
        peer: secrets::key(&keys, "peer")?,
        instance: secrets::key(&keys, "instance")?,
    };
    let mut control = dial.open_within(WAIT)?;
    operator::call(&mut control, OPERATION, SdkValue::Null).map_err(|error| match error {
        layerfs_sandbox::SdkError::Refused(refusal) => refusal,
        layerfs_sandbox::SdkError::Operation(failure) => {
            layerfs_sandbox::daemon::values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        layerfs_sandbox::SdkError::Mount(failure) => failed(failure),
    })
}

/// A daemon that did not answer: a warning naming its endpoint and the
/// operation. An exhausted pool is reported from the refusal's admission
/// notice, which names its resource and vacant slots; the daemon never
/// admitted the query, so it is still not an error.
fn unanswered(findings: &mut Findings, name: &str, endpoint: SocketAddr, refusal: &SdkRefusal) {
    let detail = &refusal.detail;
    if refusal.code == "Capacity" && detail.get_text("resource") == Some("daemon_control_slot") {
        findings.warning(
            ("sandbox", name),
            format!(
                "Sandbox {name}'s daemon at {endpoint} refused `{OPERATION}` with Capacity on resource daemon_control_slot (pool_vacant {}, reserved_vacant {}, retained {}): no control slot is vacant",
                detail.get_int("pool_vacant").unwrap_or(0),
                detail.get_int("reserved_vacant").unwrap_or(0),
                detail.get_int("retained").unwrap_or(0)
            ),
            format!(
                "end the calls that hold its control slots, or wait for them to end; `layerfs sandbox get {name}` shows the daemon's daemon_capacity, the `{OPERATION}` read model"
            ),
        );
        return;
    }
    findings.warning(
        ("sandbox", name),
        format!(
            "Sandbox {name}'s daemon at {endpoint} did not answer `{OPERATION}` within {}s: {}",
            WAIT.as_secs(),
            refusal.message
        ),
        format!(
            "`layerfs sandbox get {name}` shows whether it still runs; `layerfs sandbox close {name}` closes one whose daemon is gone"
        ),
    );
}

/// The warnings the daemon's capacity model gives: retained connection
/// failures, and a pool with no vacant control slot.
fn capacity(findings: &mut Findings, name: &str, endpoint: SocketAddr, model: &CapacityModel) {
    let retained = &model.retained;
    if retained.count > 0 {
        findings.warning(
            ("sandbox", name),
            format!(
                "Sandbox {name}'s daemon at {endpoint} retained {} of {} connection failure record(s) ({} dropped): calls it refused before Hello are kept there for {:.1}s",
                retained.count,
                retained.limit,
                retained.dropped,
                retained.ttl_ms as f64 / 1000.0
            ),
            capacity_fix(name),
        );
    }
    let admission = &model.admission;
    if admission.exhausted() {
        findings.warning(
            ("sandbox", name),
            format!(
                "Sandbox {name}'s daemon at {endpoint} has no vacant pool control slot (pool_vacant {}, pool_running {}, reserved_vacant {}, reserved_running {}); a call that needs one is refused Capacity on resource daemon_control_slot",
                admission.pool_vacant,
                admission.pool_running,
                admission.reserved_vacant,
                admission.reserved_running
            ),
            format!(
                "end the calls that hold its control slots, or wait for them to end; `layerfs sandbox get {name}` shows the daemon's daemon_capacity, the `{OPERATION}` read model"
            ),
        );
    }
}

/// Where the operator finds the model a capacity warning came from.
fn capacity_fix(name: &str) -> String {
    format!(
        "`layerfs sandbox get {name}` shows the daemon's daemon_capacity, the `{OPERATION}` read model"
    )
}
