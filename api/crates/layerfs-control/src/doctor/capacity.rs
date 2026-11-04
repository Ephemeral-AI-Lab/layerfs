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
use crate::common::home::paths;
use crate::common::outcome::{failed, refused};
use crate::common::secrets;
use crate::daemon::operator;
use crate::sandbox::direct;
use crate::sandbox::record::SandboxRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// The daemon's operator read model of control admission and retained
/// connection failures.
const OPERATION: &str = "backend.daemon.connection_failures";

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
            Ok(value) => capacity(findings, name, endpoint, &value),
            Err(refusal) => unanswered(findings, name, endpoint, &refusal),
        }
    }
}

/// The address of a running Sandbox's daemon when its record names one to
/// this host: a forwarded machine's forward, else the endpoint the start
/// recorded in `control.json`. Nothing is observed or started.
fn endpoint(home: &Path, sandbox: &SandboxRecord) -> Option<SocketAddr> {
    if let Some(forward) = (sandbox.to_record().get("daemon")).and_then(direct::forward_of) {
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
    let dial = crate::daemon::pool::Dial {
        endpoint,
        // a forwarded endpoint is already on this host's loopback
        host: None,
        secret: secrets::key(&keys, "secret")?,
        peer: secrets::key(&keys, "peer")?,
        instance: secrets::key(&keys, "instance")?,
    };
    let mut control = dial.open_within(WAIT)?;
    operator::call(&mut control, OPERATION, SdkValue::Null).map_err(|error| match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failure) => {
            crate::daemon::values::refusal(&failure).unwrap_or_else(|| failed(failure))
        }
        crate::SdkError::Mount(failure) => failed(failure),
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
fn capacity(findings: &mut Findings, name: &str, endpoint: SocketAddr, value: &SdkValue) {
    let number = |value: &SdkValue, field: &str| value.get_int(field).unwrap_or(0);
    if let Some(retained) = value.get("retained") {
        let count = number(retained, "count");
        if count > 0 {
            findings.warning(
                ("sandbox", name),
                format!(
                    "Sandbox {name}'s daemon at {endpoint} retained {count} of {} connection failure record(s) ({} dropped): calls it refused before Hello are kept there for {:.1}s",
                    number(retained, "limit"),
                    number(retained, "dropped"),
                    number(retained, "ttl_ms") as f64 / 1000.0
                ),
                format!(
                    "`layerfs sandbox get {name}` shows the daemon's daemon_capacity, the `{OPERATION}` read model"
                ),
            );
        }
    }
    if let Some(admission) = value.get("admission") {
        let vacant = number(admission, "pool_vacant");
        let running = number(admission, "pool_running");
        if vacant == 0 && running > 0 {
            findings.warning(
                ("sandbox", name),
                format!(
                    "Sandbox {name}'s daemon at {endpoint} has no vacant pool control slot (pool_vacant {vacant}, pool_running {running}, reserved_vacant {}, reserved_running {}); a call that needs one is refused Capacity on resource daemon_control_slot",
                    number(admission, "reserved_vacant"),
                    number(admission, "reserved_running")
                ),
                format!(
                    "end the calls that hold its control slots, or wait for them to end; `layerfs sandbox get {name}` shows the daemon's daemon_capacity, the `{OPERATION}` read model"
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(
        pool_vacant: i64,
        pool_running: i64,
        count: i64,
        limit: i64,
        dropped: i64,
    ) -> SdkValue {
        SdkValue::map([
            (
                "admission",
                SdkValue::map([
                    ("pool_vacant", SdkValue::Int(pool_vacant)),
                    ("pool_running", SdkValue::Int(pool_running)),
                    ("reserved_vacant", SdkValue::Int(1)),
                    ("reserved_running", SdkValue::Int(0)),
                ]),
            ),
            (
                "retained",
                SdkValue::map([
                    ("limit", SdkValue::Int(limit)),
                    ("ttl_ms", SdkValue::Int(3_600_000)),
                    ("count", SdkValue::Int(count)),
                    ("dropped", SdkValue::Int(dropped)),
                    ("recorded", SdkValue::Int(count)),
                    ("failures", SdkValue::List(Vec::new())),
                ]),
            ),
        ])
    }

    fn findings(value: &SdkValue) -> SdkValue {
        let mut found = Findings::default();
        let endpoint: SocketAddr = "127.0.0.1:1".parse().unwrap();
        capacity(&mut found, "sb", endpoint, value);
        found.to_sdk()
    }

    #[test]
    fn retained_failures_and_an_exhausted_pool_are_warnings_with_their_counts() {
        let report = findings(&model(0, 8, 3, 64, 2));
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
        let SdkValue::List(items) = report.get("findings").unwrap() else {
            panic!("no findings");
        };
        assert_eq!(items.len(), 2, "{report:?}");
        let text: Vec<String> = items
            .iter()
            .map(|item| {
                format!(
                    "{} {}",
                    item.get_text("message").unwrap(),
                    item.get_text("fix").unwrap()
                )
            })
            .collect();
        assert!(text[0].contains("retained 3 of 64"), "{text:?}");
        assert!(text[0].contains("2 dropped"), "{text:?}");
        assert!(text[0].contains(OPERATION), "{text:?}");
        assert!(text[1].contains("pool_vacant 0"), "{text:?}");
        assert!(text[1].contains("pool_running 8"), "{text:?}");
        assert!(text[1].contains("Capacity"), "{text:?}");
        assert!(text[1].contains("daemon_control_slot"), "{text:?}");
        assert!(text[1].contains(OPERATION), "{text:?}");
        for item in items {
            assert_eq!(item.get_text("severity"), Some("warning"));
        }
    }

    #[test]
    fn a_healthy_daemon_gives_no_finding() {
        let report = findings(&model(7, 1, 0, 64, 0));
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
        assert!(matches!(report.get("findings"), Some(SdkValue::List(items)) if items.is_empty()));
    }

    #[test]
    fn an_exhausted_pool_refusal_is_a_capacity_warning_not_an_error() {
        let refusal = SdkRefusal {
            code: "Capacity".to_owned(),
            message: "the Sandbox's daemon has no vacant control slot".to_owned(),
            detail: SdkValue::map([
                ("resource", SdkValue::text("daemon_control_slot")),
                ("retained", SdkValue::Int(3)),
                ("pool_vacant", SdkValue::Int(0)),
                ("reserved_vacant", SdkValue::Int(1)),
                ("retryable", SdkValue::Bool(true)),
            ]),
        };
        let mut found = Findings::default();
        let endpoint: SocketAddr = "127.0.0.1:1".parse().unwrap();
        unanswered(&mut found, "sb", endpoint, &refusal);
        let report = found.to_sdk();
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
        let SdkValue::List(items) = report.get("findings").unwrap() else {
            panic!("no findings");
        };
        assert_eq!(items.len(), 1, "{report:?}");
        let message = items[0].get_text("message").unwrap();
        assert!(message.contains("Capacity"), "{message}");
        assert!(message.contains("daemon_control_slot"), "{message}");
        assert!(message.contains("pool_vacant 0"), "{message}");
        assert!(message.contains("reserved_vacant 1"), "{message}");
        assert!(message.contains("retained 3"), "{message}");
        assert_eq!(items[0].get_text("severity"), Some("warning"));
    }
}
