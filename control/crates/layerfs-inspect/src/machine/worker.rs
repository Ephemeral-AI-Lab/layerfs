//! Read-only host worker health and evidence for management calls.
use super::super::doctor::budget::{Budget, Checks};
use layerfs_home::home::paths;
use layerfs_machine::status::worker::{evidence, observe};
use layerfs_machine::worker::{HostObservation, HostPaths, HostProbe};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;
use std::time::Duration;

const ENDPOINT: &str = "machine.worker.endpoint";
const AUTH: &str = "machine.worker.auth";
const COMPAT: &str = "machine.worker.compatibility";
const PING: &str = "machine.worker.ping";
const RETAINED: &str = "machine.worker.retained";

const TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn holder(home: &Path, store: &str, live: &[u32], budget: &Budget) -> Option<u32> {
    let HostObservation::Ready(reply) = observe(home, budget.limit(TIMEOUT)) else {
        return None;
    };
    let owner = layerfs_store::lease::Owner::decode(&reply)?;
    let pid = u32::try_from(reply.get_int("pid")?).ok()?;
    (live.contains(&pid) && owner.holds(home, store).ok() == Some(true)).then_some(pid)
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
    match &probe {
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

pub(crate) fn directory(home: &Path, checks: &mut Checks) {
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
