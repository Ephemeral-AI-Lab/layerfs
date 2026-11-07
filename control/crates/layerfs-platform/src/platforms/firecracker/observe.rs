//! `firecracker`: one Firecracker microVM per machine, run inside a `lima`
//! machine made with `nested` (macOS has no KVM; nested KVM under `vz` needs
//! an M3 or newer and macOS 15+). Every file lives on the host's data disk
//! under `/layerfs/data/firecracker`: `bin/firecracker`, the guest kernel
//! `vmlinux`, the base root filesystem `rootfs.ext4` (an sshd that accepts
//! the key, util-linux) and that key, `id_ed25519`. They come from the
//! bundle's `firecracker/` folder when the nested host is made; a bundle
//! without them leaves creation refusing `Unsupported`, naming each missing
//! file.
//!
//! Each machine gets `vms/<vm>/` (its own root filesystem copy, config and
//! `vm.env`), a tap `lfsfc<n>` with the `/30` network `172.30.<n>.0` (host
//! `.1`, guest `.2`; `n` is lasting state, at most 250 per host), and the
//! data disk `disks/<vm>.ext4`, mounted at `/layerfs/data` in the microVM.
//! The worker is reached by `ssh -L 127.0.0.1:<port>:172.30.<n>.2:7070`
//! to the Lima host, started detached. The microVM's ssh host key is not
//! checked: its address is a point-to-point tap inside the host, reused by
//! the next VM of that number, and every root filesystem is one image's copy.
use super::scripts::INSPECT;
use crate::error::{Code, MachineError};
use crate::platforms::lima;
use crate::probe::first_line;
use crate::setup::forward;
use crate::setup::guest::Guest;
use crate::setup::vm::BOOT;
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::Spec;

pub(super) const ROOT: &str = super::host::FIRECRACKER_ROOT;

pub(super) fn host_of(spec: &Spec) -> Result<&Spec, MachineError> {
    spec.host.as_deref().ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            "a firecracker machine names its lima host",
        )
    })
}

/// The host's guest, which must run.
pub(super) fn host_guest(spec: &Spec) -> Result<Guest, MachineError> {
    lima::guest(host_of(spec)?)
}

/// The microVM's guest, through its running host.
pub(crate) fn machine_guest(spec: &Spec) -> Result<Guest, MachineError> {
    Ok(guest(spec, host_guest(spec)?))
}

pub(super) fn guest(spec: &Spec, host: Guest) -> Guest {
    Guest::Firecracker {
        host: Box::new(host),
        root: ROOT.to_owned(),
        vm: spec.vm_name.clone(),
    }
}

/// The microVM's VMM state and guest address, or `None` when it does not exist.
pub(super) fn inspect(spec: &Spec, host: &Guest) -> Result<Option<(bool, String)>, MachineError> {
    let out = host.exec(
        &spec.tools,
        &["sh", "-c", INSPECT, "sh", ROOT, &spec.vm_name],
        BOOT,
    )?;
    match out.code {
        Some(3) => Ok(None),
        Some(0) => {
            let mut lines = out.stdout.lines();
            let running = lines.next() == Some("running");
            Ok(Some((running, lines.next().unwrap_or_default().to_owned())))
        }
        _ => Err(MachineError::new(
            Code::Failed,
            format!("reading the microVM failed: {}", out.said()),
        )),
    }
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = tool::lookup(&spec.tools, "limactl")
        .and_then(|p| first_line(tool::command(&p, ["--version"]), false));
    let host = match host_guest(spec) {
        Ok(host) => host,
        Err(error) => {
            let mut status = with_worker(spec, State::Stopped, None, version);
            status.down_reason = Some(format!("its Lima host is down: {}", error.message));
            return Ok(status);
        }
    };
    let kernel = match inspect(spec, &host)? {
        None => State::Missing,
        Some((true, _)) if forward::running(spec) => State::Running,
        Some(_) => State::Stopped,
    };
    let free = (kernel == State::Running)
        .then(|| worker::free_disk(spec, &guest(spec, host)))
        .flatten();
    Ok(with_worker(spec, kernel, free, version))
}
