//! `ssh` power: see `mod.rs`.
use super::host_key::{accept_key, accepted};
use super::observe::{dest, guest, options, ssh_of, END_WORKER};
use crate::error::{Code, MachineError};
use crate::setup::forward;
use crate::setup::guest::{Guest, EXEC};
use crate::setup::{tool, worker};
use crate::spec::{Bundle, Lasting, Running, Spec, WORKER_PORT};
use std::path::Path;

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

/// The data root, the forward and the worker.
pub(super) fn bring_up(
    spec: &Spec,
    bundle: &Bundle,
    known: &Path,
) -> Result<Running, MachineError> {
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
