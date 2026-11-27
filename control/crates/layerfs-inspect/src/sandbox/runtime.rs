//! Saved-route prerequisites and process-identity observations only:
//! `sandbox.runtime`, and a forward's `sandbox.route` and
//! `sandbox.forward_process`.
use super::{cause, remove_fix, Checks};
use layerfs_home::home::paths;
use layerfs_home::state::BlockedBy;
use layerfs_os::process::{self, LocalProcess};
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::SandboxRecord;
use std::path::Path;

/// `sandbox.route` and `sandbox.forward_process` of a forwarded Sandbox:
/// whether its daemon can be dialled through the saved forward.
pub(super) fn forward(home: &Path, record: &SandboxRecord, checks: &mut Checks) -> bool {
    let name = record.name();
    let Some(daemon) = record.daemon() else {
        return false;
    };
    let Some(forward) = daemon.forward() else {
        return false;
    };
    if forward.machine() != Some(record.machine()) || forward.address().is_none() {
        checks.fail(
            "sandbox.route",
            "its saved forward names no address, or another machine than its own",
        );
        checks.fix("sandbox.route", remove_fix(name));
        checks.evidence(
            "sandbox.route",
            SdkValue::map([
                ("cause", SdkValue::text("forward_incomplete")),
                (
                    "forward_machine",
                    SdkValue::opt_text(forward.machine().map(str::to_owned)),
                ),
                ("sandbox_machine", SdkValue::text(record.machine())),
                (
                    "address_present",
                    SdkValue::Bool(forward.address().is_some()),
                ),
            ]),
        );
        return false;
    }
    if !paths::worker_record(home, record.machine()).is_file() {
        let machine = record.machine();
        let by = BlockedBy {
            thing: "machine",
            handle: machine.to_owned(),
            reason: "not_answering",
        };
        let message = format!("its forward needs machine {machine}'s worker, which has no record");
        checks.waits("sandbox.route", message, by);
        return false;
    }
    if let Some(pid) = forward.pid() {
        let raw = daemon.to_record();
        let identity = raw.get("forward");
        let start = identity
            .and_then(|v| v.get_text("pid_start"))
            .and_then(|v| v.parse::<u64>().ok());
        let boot = identity
            .and_then(|v| v.get_text("pid_boot"))
            .filter(|s| s.len() == 32)
            .and_then(|v| u128::from_str_radix(v, 16).ok());
        let (Ok(pid), Some(start), Some(boot)) = (u32::try_from(pid), start, boot) else {
            checks.warning(
                "sandbox.forward_process",
                "its forward's process has no complete identity (pid, start time, boot): whether it runs is not known",
            );
            // no fix: nothing to run; the doctor only observes
            checks.evidence(
                "sandbox.forward_process",
                SdkValue::map([
                    ("pid_present", SdkValue::Bool(u32::try_from(pid).is_ok())),
                    ("start_present", SdkValue::Bool(start.is_some())),
                    ("boot_present", SdkValue::Bool(boot.is_some())),
                ]),
            );
            return false;
        };
        match observed(LocalProcess {
            pid,
            start,
            boot: Some(boot),
        }) {
            Observation::Present => checks.pass(
                "sandbox.forward_process",
                "its forward's process runs, with its saved pid, start time and boot",
            ),
            Observation::Gone => {
                checks.fail(
                    "sandbox.forward_process",
                    "its forward's process ended, or its pid was reused",
                );
                checks.fix("sandbox.forward_process", remove_fix(name));
                checks.evidence(
                    "sandbox.forward_process",
                    SdkValue::map([
                        ("pid", SdkValue::Int(i64::from(pid))),
                        ("observed", SdkValue::text("ended_or_reused")),
                    ]),
                );
                return false;
            }
            Observation::Unknown => {
                checks.unknown(
                    "sandbox.forward_process",
                    "its forward's process could not be inspected",
                );
                return false;
            }
        }
    }
    // a shared-port forward has no process of its own: its check does not apply
    checks.pass(
        "sandbox.route",
        "its saved forward can be used; no worker was asked",
    );
    true
}

/// `sandbox.runtime` of a runtime on this kernel: false when it was seen
/// gone, so its daemon is not dialled.
pub(super) fn local(home: &Path, record: &SandboxRecord, checks: &mut Checks) -> bool {
    const ID: &str = "sandbox.runtime";
    let Some(daemon) = record.daemon() else {
        checks.warning(
            ID,
            "its record names no runtime process: whether it runs is not known",
        );
        // no fix: nothing to run; the doctor only observes
        checks.evidence(ID, cause("no_daemon_record"));
        return true;
    };
    // another runtime has no process this check can match: it does not apply
    if !matches!(daemon.runtime(), Some("process" | "namespaces")) {
        return true;
    }
    let pid = daemon.pid().and_then(|p| u32::try_from(p).ok());
    let start = daemon.start().and_then(|s| s.parse::<u64>().ok());
    let boot = (daemon.boot())
        .filter(|s| s.len() == 32)
        .and_then(|s| u128::from_str_radix(s, 16).ok());
    let process = pid.zip(start).zip(boot);
    let Some(((pid, start), boot)) = process else {
        checks.warning(
            ID,
            "its runtime process has no complete identity (pid, start time, boot): whether it runs is not known",
        );
        // no fix: nothing to run; the doctor only observes
        checks.evidence(
            ID,
            SdkValue::map([
                ("pid_present", SdkValue::Bool(pid.is_some())),
                ("start_present", SdkValue::Bool(start.is_some())),
                ("boot_present", SdkValue::Bool(boot.is_some())),
            ]),
        );
        return true;
    };
    match observed(LocalProcess {
        pid,
        start,
        boot: Some(boot),
    }) {
        Observation::Present => {
            checks.pass(
                ID,
                "its runtime process runs, with its saved pid, start time and boot",
            );
            true
        }
        Observation::Gone => {
            // removing the Sandbox discards its Workspaces' uncommitted changes
            let recorded = crate::sandbox::workspace::recorded(&record.dir(home)).len();
            let mut message =
                "its runtime process ended, or its pid, start time or boot changed".to_owned();
            if recorded > 0 {
                message.push_str(&format!(
                    "; removing it loses the uncommitted changes of its {recorded} recorded Workspace(s)"
                ));
            }
            checks.fail(ID, message);
            checks.fix(ID, remove_fix(record.name()));
            checks.evidence(
                ID,
                SdkValue::map([
                    ("pid", SdkValue::Int(i64::from(pid))),
                    (
                        "runtime",
                        SdkValue::opt_text(daemon.runtime().map(str::to_owned)),
                    ),
                    ("observed", SdkValue::text("ended_or_changed")),
                    (
                        "workspaces_recorded",
                        SdkValue::Int(i64::try_from(recorded).unwrap_or(i64::MAX)),
                    ),
                ]),
            );
            false
        }
        Observation::Unknown => {
            checks.unknown(ID, "the kernel could not say whether its runtime runs");
            true
        }
    }
}

enum Observation {
    Present,
    Gone,
    Unknown,
}
fn observed(expected: LocalProcess) -> Observation {
    if expected.pid == 0 || expected.pid > i32::MAX as u32 {
        return Observation::Gone;
    }
    match process::stat(expected.pid) {
        Ok(stat) => match process::boot() {
            Ok(boot)
                if !stat.ended() && stat.start == expected.start && Some(boot) == expected.boot =>
            {
                Observation::Present
            }
            Ok(_) => Observation::Gone,
            Err(_) => Observation::Unknown,
        },
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(3) =>
        {
            Observation::Gone
        }
        Err(_) => Observation::Unknown,
    }
}
