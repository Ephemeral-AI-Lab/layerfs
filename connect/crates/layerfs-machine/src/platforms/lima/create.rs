//! `lima` create: see `mod.rs`.
use super::observe::{disk_exists, disk_name, host_check, observe, CLI};
use super::power::boot_and_bring_up;
use super::template::template;
use crate::error::{kept, Code, MachineError};
use crate::setup::tool;
use crate::setup::vm::{call, checked, exists, mib, new_port, BOOT, CALL};
use crate::setup::State;
use crate::spec::{Bundle, Running, Spec};

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
        &tool::find(&spec.tools, CLI)?,
        ["create", "--name", &spec.vm_name, "--tty=false", "-"],
    );
    let made = tool::run(
        command,
        Some(template(spec, port).as_bytes()),
        CALL,
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
