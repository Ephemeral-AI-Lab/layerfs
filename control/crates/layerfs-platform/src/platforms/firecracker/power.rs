//! `firecracker` power: see `mod.rs`.
use super::observe::{guest, host_guest, inspect, ROOT};
use super::scripts::{BOOT_VM, STOP};
use crate::error::{Code, MachineError};
use crate::platforms::lima;
use crate::setup::forward;
use crate::setup::guest::Guest;
use crate::setup::vm::{not_found, BOOT};
use crate::setup::{tool, worker};
use crate::spec::{Bundle, Lasting, Running, Spec, WORKER_PORT};

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let host = host_guest(spec)?;
    if inspect(spec, &host)?.is_none() {
        return Err(not_found(spec));
    }
    boot_and_bring_up(spec, bundle, &host, spec.state)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    let host = host_guest(spec)?;
    host.script(
        &spec.tools,
        "stopping the microVM",
        STOP,
        &[ROOT, &spec.vm_name],
    )?;
    Ok(())
}

/// Boots the microVM unless it runs, opens a new forward, starts the worker.
pub(super) fn boot_and_bring_up(
    spec: &Spec,
    bundle: &Bundle,
    host: &Guest,
    lasting: Lasting,
) -> Result<Running, MachineError> {
    let out = host.exec(
        &spec.tools,
        &["sh", "-c", BOOT_VM, "sh", ROOT, &spec.vm_name],
        BOOT,
    )?;
    out.answer("booting the microVM")?;
    let Some((_, ip)) = inspect(spec, host)? else {
        return Err(not_found(spec));
    };
    let port = lasting.port.map_or_else(tool::free_port, Ok)?;
    forward::close(spec);
    let Guest::Lima { dir, vm } = host else {
        return Err(MachineError::new(
            Code::InvalidInput,
            "a firecracker host is a lima machine",
        ));
    };
    let (options, dest) = lima::tunnel(dir, vm);
    let target = format!("{ip}:{WORKER_PORT}");
    let pid = forward::open(spec, &options, &dest, port, &target)?;
    let endpoint = worker::bring_up(spec, &guest(spec, host.clone()), bundle, port, Some(pid))
        .inspect_err(|_| forward::end(spec, pid, port))?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..lasting
        },
    })
}
