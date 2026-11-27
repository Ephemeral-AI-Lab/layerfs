//! `lima` create: see `mod.rs`.
use super::status::{disk_exists, disk_name, host_check, observe, CLI};
use super::template::template;
use crate::platform::error::{kept, Code, MachineError};
use crate::setup::tool;
use crate::setup::vm::{call, checked, exists, mib, new_port, BOOT, CALL, CREATE};
use crate::setup::State;
use crate::setup::spec::{Bundle, Running, Spec};

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    host_check(spec)?;
    if observe(spec)?.0 != State::Missing {
        return Err(exists(spec, "the Lima instance"));
    }
    let disk = disk_name(&spec.vm_name);
    if disk_exists(spec, &disk)? {
        return Err(MachineError::new(
            Code::Exists,
            format!(
                "the Lima disk {disk} for {} already exists; it is left alone",
                spec.vm_name
            ),
        ));
    }
    let port = new_port(spec)?;
    let size = format!("{}iB", mib(spec.disk));
    checked(
        spec,
        CLI,
        &["disk", "create", &disk, "--size", &size, "--format", "raw"],
        CALL,
    )?;
    let command = tool::command(
        &spec.tools,
        &tool::find(&spec.tools, CLI)?,
        ["create", "--name", &spec.vm_name, "--tty=false", "-"],
    );
    let made = tool::run(
        command,
        Some(template(spec, port).as_bytes()),
        CREATE,
        "limactl create",
    )?;
    if !made.ok() {
        if observe(spec)?.0 != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("limactl create failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: the disk this create made goes (one cleanup)
        let _ = call(spec, CLI, &["disk", "delete", &disk], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("limactl create failed: {}", made.said()),
        ));
    }
    kept(boot_and_bring_up(spec, bundle, port, true))
}

/// The instance, then its data disk.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)?.0 != State::Missing {
        checked(spec, CLI, &["delete", "--force", &spec.vm_name], BOOT)?;
    }
    let disk = disk_name(&spec.vm_name);
    if disk_exists(spec, &disk)? {
        checked(spec, CLI, &["disk", "delete", &disk], CALL)?;
    }
    Ok(())
}

// `lima` power: see `mod.rs`.
use super::status::guest;
use crate::setup::vm::{not_found, recorded_port};
use crate::setup::worker;
use crate::setup::spec::Lasting;

/// Lima reports an instance `Broken` when it cannot read the instance's own
/// files, and `limactl start` refuses one itself ("errors inspecting
/// instance"), so a start cannot repair it and is refused before any call.
/// apple-container has no such provider refusal and tries its `Failed` once
/// (`apple_container/power.rs`).
pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    match observe(spec)?.0 {
        State::Missing => Err(not_found(spec)),
        State::Failed => Err(MachineError::new(
            Code::Failed,
            format!(
                "the Lima instance {} is broken: Lima cannot read its files, so it cannot be started; `limactl list {}` says why",
                spec.vm_name, spec.vm_name
            ),
        )),
        State::Stopped => boot_and_bring_up(spec, bundle, port, true),
        State::Running => boot_and_bring_up(spec, bundle, port, false),
    }
}

/// `limactl stop` keeps the instance and its disk; the worker ends with it.
pub(crate) fn stop(spec: &Spec, force: bool) -> Result<(), MachineError> {
    let state = observe(spec)?.0;
    if state == State::Running || (force && state == State::Failed) {
        let mut words = vec!["stop"];
        if force {
            words.push("--force");
        }
        words.push(&spec.vm_name);
        checked(spec, CLI, &words, BOOT)?;
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
