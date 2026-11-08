//! Saved-route prerequisites and process-identity observations only.
use super::Checks;
use layerfs_bridge::process::{self, LocalProcess};
use layerfs_home::home::paths;
use layerfs_sandbox::record::SandboxRecord;
use std::path::Path;

pub(super) fn forward(home: &Path, record: &SandboxRecord, checks: &mut Checks) -> bool {
    let Some(daemon) = record.daemon() else {
        return false;
    };
    let Some(forward) = daemon.forward() else {
        return false;
    };
    if forward.machine() != Some(record.machine()) || forward.address().is_none() {
        checks.fail(
            "sandbox.route",
            "Saved forward address or machine does not match sandbox placement",
        );
        return false;
    }
    if !paths::worker_record(home, record.machine()).is_file() {
        checks.unknown(
            "sandbox.route",
            "Forwarded route requires a recorded running worker; worker record is absent",
        );
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
            checks.unknown("sandbox.route", "Forward process lacks valid PID/start/boot identity; liveness is not inferred from PID alone");
            return false;
        };
        match observed(LocalProcess {
            pid,
            start,
            boot: Some(boot),
        }) {
            Observation::Present => checks.pass(
                "sandbox.forward_process",
                "Forward process matches saved PID, start time and boot",
            ),
            Observation::Gone => {
                checks.fail(
                    "sandbox.forward_process",
                    "Saved forward process ended or its PID was reused",
                );
                checks.blocked(
                    "sandbox.route",
                    "Forward process is absent",
                    "sandbox.forward_process",
                );
                return false;
            }
            Observation::Unknown => {
                checks.unknown(
                    "sandbox.forward_process",
                    "Forward process identity could not be inspected",
                );
                return false;
            }
        }
    } else {
        checks.skip(
            "sandbox.forward_process",
            "Shared-port forward records no carrier process",
        );
    }
    checks.pass(
        "sandbox.route",
        "Saved direct-forward prerequisites are present; no worker RPC or replacement forward used",
    );
    true
}

pub(super) fn local(record: &SandboxRecord, checks: &mut Checks) {
    let Some(daemon) = record.daemon() else {
        checks.unknown("sandbox.runtime", "No local runtime identity was recorded");
        return;
    };
    if !matches!(daemon.runtime(), Some("native" | "namespaces")) {
        checks.unknown(
            "sandbox.runtime",
            "Recorded runtime is unsupported by this observational process check",
        );
        return;
    }
    let process = daemon
        .pid()
        .and_then(|p| u32::try_from(p).ok())
        .zip(daemon.start().and_then(|s| s.parse().ok()))
        .zip(
            daemon
                .boot()
                .filter(|s| s.len() == 32)
                .and_then(|s| u128::from_str_radix(s, 16).ok()),
        );
    let Some(((pid, start), boot)) = process else {
        checks.unknown(
            "sandbox.runtime",
            "Runtime process lacks complete PID/start/boot identity",
        );
        return;
    };
    match observed(LocalProcess {
        pid,
        start,
        boot: Some(boot),
    }) {
        Observation::Present => checks.pass(
            "sandbox.runtime",
            "Recorded daemon/init process is live with the saved start time and boot",
        ),
        Observation::Gone => checks.fail(
            "sandbox.runtime",
            "Recorded runtime process ended or its PID/start/boot identity changed",
        ),
        Observation::Unknown => checks.unknown(
            "sandbox.runtime",
            "Kernel could not establish recorded runtime process presence",
        ),
    }
    checks.skip("sandbox.runtime_containment", "Process presence is observed; namespace isolation and application filesystem serving are not exercised");
}

enum Observation {
    Present,
    Gone,
    Unknown,
}
#[cfg(unix)]
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
}#[cfg(windows)]
/// Windows: the observation rides LocalProcess::observe (absolute
/// creation time, boot None) - the same judgement in its own units.
fn observed(expected: LocalProcess) -> Observation {
    match LocalProcess::observe(expected.pid) {
        Ok(now) if now.start == expected.start => Observation::Present,
        Ok(_) => Observation::Unknown,
        Err(_) => Observation::Gone,
    }
}


