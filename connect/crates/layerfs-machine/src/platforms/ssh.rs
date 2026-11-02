//! `ssh`: a Linux host reached over ssh, connected and never created. The
//! user is root by default, so setup and the worker run as root. Host keys
//! are checked strictly: the host's key must already be in the known_hosts
//! file the caller passes; the accepted lines are copied to
//! `<work_dir>/known_hosts`, which every later call uses. The data root
//! (default `/layerfs/data`) must be on ext4, xfs or btrfs (O18); another
//! path is bind-mounted at `/layerfs/data`. The worker binds the host's
//! loopback and is reached through a detached `ssh -L` forward.
use crate::error::{Code, MachineError};
use crate::probe::first_line;
use crate::setup::forward;
use crate::setup::guest::{Guest, EXEC};
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::{Bundle, Lasting, Running, Spec, SshSpec, BUNDLE, STATE, WORKER_PORT};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The data root on a filesystem a Store may use, at `/layerfs/data`.
const DATA_ROOT: &str = r#"set -e
root=$1
mkdir -p "$root"
t=$(findmnt -n -o FSTYPE -T "$root")
case "$t" in ext4|xfs|btrfs) ;; *) echo "the data root $root is on ${t:-an unknown filesystem}, not ext4, xfs or btrfs" >&2; exit 4;; esac
if [ "$root" != /layerfs/data ]; then
  mkdir -p /layerfs/data
  mountpoint -q /layerfs/data || mount --bind "$root" /layerfs/data
fi"#;

/// Ends the worker, if its recorded pid is still exactly the bundle's
/// `layerfs worker` (pids are reused after a reboot).
const END_WORKER: &str = r#"p=$(cat /layerfs/state/worker.pid 2>/dev/null) || exit 0
if [ "$p" != $$ ] && [ "$(tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null)" = "/layerfs/bundle/layerfs worker " ]; then kill "$p"; fi; exit 0"#;

/// What connect wrote, never the data root; a bind mount of it is undone.
const FORGET: &str = r#"rm -rf "$1" "$2"
if [ "$3" != /layerfs/data ] && mountpoint -q /layerfs/data; then umount /layerfs/data; fi; exit 0"#;

fn ssh_of(spec: &Spec) -> Result<&SshSpec, MachineError> {
    spec.ssh
        .as_ref()
        .ok_or_else(|| MachineError::new(Code::InvalidInput, "an ssh machine needs its host"))
}

/// The accepted host key lines, in the machine's work folder.
fn accepted(spec: &Spec) -> PathBuf {
    spec.work_dir.join("known_hosts")
}

/// How known_hosts names the host: `host`, or `[host]:port` off port 22.
fn host_pattern(ssh: &SshSpec) -> String {
    if ssh.port == 22 {
        ssh.host.clone()
    } else {
        format!("[{}]:{}", ssh.host, ssh.port)
    }
}

/// The ssh options every call shares, checking keys against `known_hosts`.
/// `BatchMode` keeps a password prompt from ever being tried.
fn options(ssh: &SshSpec, known_hosts: &Path) -> Vec<String> {
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

fn dest(ssh: &SshSpec) -> String {
    format!("{}@{}", ssh.user, ssh.host)
}

fn guest(spec: &Spec, known_hosts: &Path) -> Result<Guest, MachineError> {
    let ssh = ssh_of(spec)?;
    Ok(Guest::Ssh {
        options: options(ssh, known_hosts),
        dest: dest(ssh),
    })
}

/// The host's lines of the caller's known_hosts, from `ssh-keygen -F`.
fn known_lines(spec: &Spec, fingerprints: bool) -> Result<Vec<String>, MachineError> {
    let ssh = ssh_of(spec)?;
    let keygen = tool::find(&spec.tools, "ssh-keygen")?;
    let flags: &[&str] = if fingerprints { &["-l", "-F"] } else { &["-F"] };
    let mut command = tool::command(&keygen, flags);
    command
        .arg(host_pattern(ssh))
        .arg("-f")
        .arg(&ssh.known_hosts);
    let out = tool::run(command, None, Duration::from_secs(10), "ssh-keygen -F")?;
    Ok(out
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// Copies the host's accepted key lines to the work folder, or refuses a host
/// whose key is not known (`Denied`).
fn accept_key(spec: &Spec) -> Result<PathBuf, MachineError> {
    let ssh = ssh_of(spec)?;
    let lines = known_lines(spec, false)?;
    if lines.is_empty() {
        return Err(MachineError::new(
            Code::Denied,
            format!(
                "the host key of {} is not in {}; add it there first",
                host_pattern(ssh),
                ssh.known_hosts.display()
            ),
        ));
    }
    let file = accepted(spec);
    let fail =
        |e: std::io::Error| MachineError::new(Code::Failed, format!("{}: {e}", file.display()));
    std::fs::create_dir_all(&spec.work_dir).map_err(fail)?;
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file)
        .map_err(fail)?;
    out.write_all(format!("{}\n", lines.join("\n")).as_bytes())
        .map_err(fail)?;
    Ok(file)
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

/// Checks the host and starts its worker; nothing on the host is created but
/// `/layerfs/bundle`, `/layerfs/state` and the data root's folders.
pub(crate) fn connect(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let known = accept_key(spec)?;
    let guest = guest(spec, &known)?;
    let kernel = guest.exec(&spec.tools, &["uname", "-s"], EXEC)?;
    if !kernel.ok() {
        return Err(MachineError::new(
            Code::Unavailable,
            format!(
                "ssh to {} failed ({}): {}",
                dest(ssh_of(spec)?),
                kernel.exit(),
                kernel.said()
            ),
        ));
    }
    if kernel.stdout.trim() != "Linux" {
        return Err(MachineError::new(
            Code::Unsupported,
            format!(
                "{} answers {:?}, not Linux",
                ssh_of(spec)?.host,
                kernel.stdout.trim()
            ),
        ));
    }
    bring_up(spec, bundle, &known)
}

/// The data root, the forward and the worker.
fn bring_up(spec: &Spec, bundle: &Bundle, known: &Path) -> Result<Running, MachineError> {
    let ssh = ssh_of(spec)?;
    let (options, dest) = (options(ssh, known), dest(ssh));
    let guest = Guest::Ssh {
        options: options.clone(),
        dest: dest.clone(),
    };
    let command = guest.command(
        &spec.tools,
        &["sh", "-c", DATA_ROOT, "sh", &ssh.data_root],
        false,
    )?;
    let out = tool::run(command, None, EXEC, "checking the data root")?;
    match out.code {
        Some(0) => {}
        Some(4) => return Err(MachineError::new(Code::Unsupported, out.said())),
        _ => {
            return Err(MachineError::new(
                Code::Failed,
                format!("checking the data root failed: {}", out.said()),
            ))
        }
    }
    let port = tool::free_port()?;
    let target = format!("127.0.0.1:{WORKER_PORT}");
    let pid = forward::open(spec, &options, &dest, port, &target)?;
    match worker::bring_up(spec, &guest, bundle, port, Some(pid)) {
        Ok(endpoint) => Ok(Running {
            endpoint,
            lasting: Lasting {
                port: Some(port),
                ..spec.state
            },
        }),
        Err(error) => {
            forward::end(spec, pid, port);
            Err(error)
        }
    }
}

/// A stopped ssh machine: its forward and worker start again.
pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    forward::close(spec);
    let known = accept_key(spec)?;
    bring_up(spec, bundle, &known)
}

/// The forward and the worker end; the host keeps running.
pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    guest(spec, &accepted(spec))?.script(&spec.tools, "stopping the worker", END_WORKER, &[])?;
    Ok(())
}

/// `machine disconnect`: the worker and forward stop, and what connect wrote
/// goes; the data root never does.
pub(crate) fn disconnect(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    let guest = guest(spec, &accepted(spec))?;
    guest.script(&spec.tools, "stopping the worker", END_WORKER, &[])?;
    guest.script(
        &spec.tools,
        "removing LayerFS's folders",
        FORGET,
        &[BUNDLE, STATE, &ssh_of(spec)?.data_root],
    )?;
    Ok(())
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
