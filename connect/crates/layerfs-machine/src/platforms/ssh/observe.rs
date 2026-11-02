//! `ssh`: a Linux host reached over ssh, connected and never created. The
//! user is root by default, so setup and the worker run as root. Host keys
//! are checked strictly: the host's key must already be in the known_hosts
//! file the caller passes; the accepted lines are copied to
//! `<work_dir>/known_hosts`, which every later call uses. The data root
//! (default `/layerfs/data`) must be on ext4, xfs or btrfs (O18); another
//! path is bind-mounted at `/layerfs/data`. The worker binds the host's
//! loopback and is reached through a detached `ssh -L` forward.
use super::host_key::{accepted, known_lines};
use crate::error::{Code, MachineError};
use crate::probe::first_line;
use crate::setup::forward;
use crate::setup::guest::{Guest, EXEC};
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::{Spec, SshSpec};
use std::path::Path;

/// Ends the worker, if its recorded pid is still exactly the bundle's
/// `layerfs worker` (pids are reused after a reboot).
pub(super) const END_WORKER: &str = r#"p=$(cat /layerfs/state/worker.pid 2>/dev/null) || exit 0
if [ "$p" != $$ ] && [ "$(tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null)" = "/layerfs/bundle/layerfs worker " ]; then kill "$p"; fi; exit 0"#;

pub(super) fn ssh_of(spec: &Spec) -> Result<&SshSpec, MachineError> {
    spec.ssh
        .as_ref()
        .ok_or_else(|| MachineError::new(Code::InvalidInput, "an ssh machine needs its host"))
}

/// The ssh options every call shares, checking keys against `known_hosts`.
/// `BatchMode` keeps a password prompt from ever being tried.
pub(super) fn options(ssh: &SshSpec, known_hosts: &Path) -> Vec<String> {
    let mut options: Vec<String> = [
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "StrictHostKeyChecking=yes",
    ]
    .map(String::from)
    .to_vec();
    options.extend([
        "-o".into(),
        format!("UserKnownHostsFile={}", known_hosts.display()),
    ]);
    options.extend(["-p".into(), ssh.port.to_string()]);
    if let Some(identity) = &ssh.identity {
        options.extend(["-i".into(), identity.display().to_string()]);
    }
    options
}

pub(super) fn dest(ssh: &SshSpec) -> String {
    format!("{}@{}", ssh.user, ssh.host)
}

pub(super) fn guest(spec: &Spec, known_hosts: &Path) -> Result<Guest, MachineError> {
    let ssh = ssh_of(spec)?;
    Ok(Guest::Ssh {
        options: options(ssh, known_hosts),
        dest: dest(ssh),
    })
}

/// The guest of a connected host, through its accepted key.
pub(crate) fn machine_guest(spec: &Spec) -> Result<Guest, MachineError> {
    guest(spec, &accepted(spec))
}

/// The host's identity: its key's fingerprint, else `/etc/machine-id`.
pub(crate) fn identity(spec: &Spec) -> Result<String, MachineError> {
    let fingerprint = known_lines(spec, true)?
        .iter()
        .flat_map(|line| line.split_whitespace())
        .find(|token| token.starts_with("SHA256:"))
        .map(str::to_owned);
    if let Some(fingerprint) = fingerprint {
        return Ok(fingerprint);
    }
    let ssh = ssh_of(spec)?;
    let id = guest(spec, &ssh.known_hosts)?
        .exec(&spec.tools, &["cat", "/etc/machine-id"], EXEC)?
        .answer("reading /etc/machine-id")?;
    if id.is_empty() {
        return Err(MachineError::new(
            Code::Failed,
            format!("{} has no identity", ssh.host),
        ));
    }
    Ok(format!("machine-id:{id}"))
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version =
        tool::lookup(&spec.tools, "ssh").and_then(|p| first_line(tool::command(&p, ["-V"]), true));
    if !forward::running(spec) {
        let mut status = with_worker(spec, State::Stopped, None, version);
        status.down_reason = Some("its worker forward is not running".into());
        return Ok(status);
    }
    let free = worker::free_disk(spec, &guest(spec, &accepted(spec))?);
    Ok(with_worker(spec, State::Running, free, version))
}
