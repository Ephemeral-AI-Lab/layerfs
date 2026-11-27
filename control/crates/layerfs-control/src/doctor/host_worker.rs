//! Read-only host worker health and evidence for management calls.
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, with_field};
use layerfs_machine::worker::{start_failure, HostObservation, HostPaths, HostProbe};
use std::path::Path;
use std::time::{Duration, Instant};

const ENDPOINT: &str = "machine.worker.endpoint";
const AUTH: &str = "machine.worker.auth";
const COMPAT: &str = "machine.worker.compatibility";
const PING: &str = "machine.worker.ping";
const RETAINED: &str = "machine.worker.retained";

const TIMEOUT: Duration = Duration::from_secs(2);

fn observe(home: &Path, timeout: Duration) -> HostObservation {
    match HostPaths::new(home) {
        Ok(paths) => paths.observe(Instant::now() + timeout),
        Err(error) => HostObservation::Unreachable(HostProbe::Endpoint, error),
    }
}

pub(super) fn holder(home: &Path, store: &str, live: &[u32], budget: &Budget) -> Option<u32> {
    let HostObservation::Ready(reply) = observe(home, budget.limit(TIMEOUT)) else {
        return None;
    };
    let owner = layerfs_store::lease::Owner::decode(&reply)?;
    let pid = u32::try_from(reply.get_int("pid")?).ok()?;
    (live.contains(&pid) && owner.holds(home, store).ok() == Some(true)).then_some(pid)
}

fn evidence(observation: &HostObservation) -> SdkValue {
    use HostObservation::*;
    let (health, reply) = match observation {
        Absent => ("absent", None),
        Stale => ("stale", None),
        Owned => ("owned", None),
        Ready(reply) => ("ready", Some(reply)),
        Draining(hello) => ("draining", Some(hello)),
        Incompatible(error) => ("unreachable", error.detail.get("worker")),
        Unreachable(_, _) => ("unreachable", None),
    };
    let mut fields = vec![("health", SdkValue::text(health))];
    fields.extend(
        [
            "pid",
            "pid_start",
            "pid_boot",
            "build",
            "generation",
            "admitted",
            "queued",
            "stores",
            "postgresql",
            "last_close",
        ]
        .into_iter()
        .map(|key| {
            (
                key,
                reply.and_then(|v| v.get(key)).cloned().unwrap_or_default(),
            )
        }),
    );
    if let Incompatible(error) = observation {
        fields.push(("compatible", SdkValue::Bool(false)));
        fields.push((
            "expected_build",
            SdkValue::opt_text(
                error
                    .detail
                    .get("expected")
                    .and_then(|value| value.get_text("build")),
            ),
        ));
    }
    if let Unreachable(stage, error) = observation {
        fields.push(("code", SdkValue::text(&error.code)));
        fields.push((
            "stage",
            SdkValue::text(match stage {
                HostProbe::Endpoint => "endpoint",
                HostProbe::Connect => "connect",
                HostProbe::Peer => "peer",
                HostProbe::Hello => "hello",
                HostProbe::Ping => "ping",
            }),
        ));
    }
    layerfs_home::helpers::map_of(fields)
}

pub(super) fn diagnose(home: &Path, budget: &Budget, checks: &mut Checks) {
    use HostObservation::*;
    let probe = observe(home, budget.limit(TIMEOUT));
    let evidence = || evidence(&probe);
    let endpoint = |checks: &mut Checks| checks.pass(ENDPOINT, "its control socket is private");
    let auth = |checks: &mut Checks| {
        checks.pass(
            AUTH,
            "the endpoint belongs to this user and identifies this home",
        )
    };
    let compatible =
        |checks: &mut Checks| checks.pass(COMPAT, "the worker build and protocol match");
    // a background start that failed (#833) left its reason: the calls
    // went direct meanwhile, so this is where it shows, while no worker
    // listens (a worker that publishes its endpoints removes it)
    let failed = matches!(&probe, Absent | Stale)
        .then(|| start_failure(home))
        .flatten();
    match &probe {
        Absent | Stale if failed.is_some() => {
            let record = failed.unwrap_or_default();
            let message = record.get_text("message").unwrap_or("unknown");
            checks.warning(
                ENDPOINT,
                format!(
                    "the last background start of the host worker failed: {message}; calls run directly until a worker starts (see log/worker.log)"
                ),
            );
            checks.fix(
                ENDPOINT,
                format!(
                    "layerfs machine start {}",
                    layerfs_home::names::HOST_MACHINE
                ),
            );
            checks.evidence(ENDPOINT, with_field(evidence(), "start_failure", record));
        }
        Absent => checks.pass_evidence(ENDPOINT, "no worker endpoint is published", evidence()),
        Stale => {
            checks.notice(
                ENDPOINT,
                "a leftover worker socket has no listener or lacks its private mode, and no worker or start holds its locks; the next start replaces it",
            );
            checks.evidence(ENDPOINT, evidence());
        }
        Owned => {
            checks.warning(
                ENDPOINT,
                "the worker socket has no listener or lacks its private mode while a host worker holds its lifetime lock (starting, closing or wedged) or a start holds the startup lock; nothing replaces it until they are released",
            );
            checks.evidence(ENDPOINT, evidence());
        }
        Incompatible(error) => {
            endpoint(checks);
            auth(checks);
            checks.notice(COMPAT, &error.message);
            checks.evidence(COMPAT, evidence());
        }
        Unreachable(stage, error) => {
            let check = match stage {
                HostProbe::Endpoint | HostProbe::Connect => ENDPOINT,
                HostProbe::Peer | HostProbe::Hello => {
                    endpoint(checks);
                    AUTH
                }
                HostProbe::Ping => {
                    endpoint(checks);
                    auth(checks);
                    compatible(checks);
                    PING
                }
            };
            if budget.expired() {
                checks.timeout(check, &error.message);
            } else {
                checks.fail(check, &error.message);
                checks.evidence(check, evidence());
            }
        }
        Ready(_) | Draining(_) => {
            endpoint(checks);
            auth(checks);
            compatible(checks);
            let message = if matches!(&probe, Ready(_)) {
                "the worker answered its read-only ping"
            } else {
                "the worker is draining"
            };
            checks.pass_evidence(PING, message, evidence());
        }
    }
}

/// `machine.worker.retained`: write outcomes the host worker kept because
/// its caller may not know them (§8.5). Read from the home, so it answers
/// whether or not a worker runs, and never starts or replaces one.
pub(super) fn retained(home: &Path, checks: &mut Checks) {
    const SHOWN: usize = 5;
    match layerfs_store::host_write::retained(home, SHOWN) {
        Ok((0, _)) => checks.pass(RETAINED, "no write outcome is kept for a caller"),
        Ok((count, records)) => {
            let dir = layerfs_store::host_write::retained_dir(home);
            checks.notice(
                RETAINED,
                format!(
                    "{count} write outcome(s) a caller may not know are kept in {}; check each record's Branch (`layerfs commit list`), then remove the records",
                    dir.display()
                ),
            );
            let dir = layerfs_home::outcome::shell_word(&dir.to_string_lossy());
            checks.fix(RETAINED, format!("rm {dir}/*.json"));
            checks.evidence(
                RETAINED,
                SdkValue::map([
                    ("count", SdkValue::Int(count as i64)),
                    ("records", SdkValue::List(records)),
                ]),
            );
        }
        Err(error) => checks.fail(RETAINED, &error.message),
    }
}

pub(super) fn directory(home: &Path, checks: &mut Checks) {
    use std::os::unix::fs::MetadataExt;
    let directory = paths::machine_dir(home, layerfs_home::names::HOST_MACHINE);
    if std::fs::symlink_metadata(&directory)
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    {
        return;
    }
    // a link or a mode others may use is the home scan's finding, with its
    // fix: the same folder is not named twice
    let scanned = [home.to_owned(), home.join("machine"), directory.clone()]
        .iter()
        .any(|folder| {
            std::fs::symlink_metadata(folder).is_ok_and(|m| m.is_symlink() || m.mode() & 0o076 != 0)
        });
    if scanned {
        return;
    }
    let verified = HostPaths::new(home).and_then(|paths| paths.check_worker_directories());
    if let Err(error) = verified {
        checks.fail("home.records", error.message);
        checks.about("home.records", "machine", layerfs_home::names::HOST_MACHINE);
        checks.evidence(
            "home.records",
            SdkValue::map([
                ("path", SdkValue::text(directory.display().to_string())),
                ("code", SdkValue::text(&error.code)),
            ]),
        );
    }
}

pub(crate) fn live(home: &Path, answer: SdkValue) -> SdkValue {
    let probe = observe(home, TIMEOUT);
    let live = answer.get("live").cloned().unwrap_or_default();
    let state = if answer.get_text("state") == Some("stopped") {
        "stopped"
    } else {
        "enabled"
    };
    let live = with_field(live, "machine_state", SdkValue::text(state));
    with_field(answer, "live", with_field(live, "worker", evidence(&probe)))
}
