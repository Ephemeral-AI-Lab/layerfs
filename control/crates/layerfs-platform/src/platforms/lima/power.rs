//! `lima` power: see `mod.rs`.
use super::observe::{disk_name, guest, observe, CLI};
use crate::error::{Code, MachineError};
use crate::setup::vm::{checked, not_found, recorded_port, BOOT};
use crate::setup::worker;
use crate::setup::State;
use crate::spec::{Bundle, Lasting, Running, Spec};

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    match observe(spec)?.0 {
        State::Missing => Err(not_found(spec)),
        State::Failed => Err(MachineError::new(
            Code::Failed,
            format!("the Lima instance {} is broken", spec.vm_name),
        )),
        State::Stopped => boot_and_bring_up(spec, bundle, port, true),
        State::Running => boot_and_bring_up(spec, bundle, port, false),
    }
}

/// `limactl stop` keeps the instance and its disk; the worker ends with it.
pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)?.0 == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], BOOT)?;
    }
    Ok(())
}

/// Boots the instance if it is stopped, attaches its disk, and starts its worker.
pub(super) fn boot_and_bring_up(
    spec: &Spec,
    bundle: &Bundle,
    port: u16,
    boot: bool,
) -> Result<Running, MachineError> {
    if boot {
        checked(spec, CLI, &["start", "--tty=false", &spec.vm_name], BOOT)?;
    }
    let guest = guest(spec)?;
    guest.script(
        &spec.tools,
        "attaching the data disk",
        ATTACH,
        &[&disk_name(&spec.vm_name)],
    )?;
    let endpoint = worker::bring_up(spec, &guest, bundle, port)?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..spec.state
        },
    })
}

/// The data disk at `/layerfs/data`, by fact: Lima's mount must be ext4.
const ATTACH: &str = r#"d=/mnt/lima-$1
test "$(findmnt -n -o FSTYPE "$d")" = ext4 || { echo "$d is not an ext4 mount" >&2; exit 1; }
mkdir -p /layerfs/data && { mountpoint -q /layerfs/data || mount --bind "$d" /layerfs/data; }"#;
