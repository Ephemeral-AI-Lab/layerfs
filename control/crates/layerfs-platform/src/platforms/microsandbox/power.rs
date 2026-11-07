//! `microsandbox` power: see `mod.rs`.
use super::observe::{guest, observe, CLI};
use crate::error::MachineError;
use crate::setup::vm::{checked, not_found, recorded_port, CREATE};
use crate::setup::worker;
use crate::setup::State;
use crate::spec::{Bundle, Lasting, Running, Spec};

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    if observe(spec)? == State::Missing {
        return Err(not_found(spec));
    }
    boot_and_bring_up(spec, bundle, port)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], CREATE)?;
    }
    Ok(())
}

pub(super) fn boot_and_bring_up(
    spec: &Spec,
    bundle: &Bundle,
    port: u16,
) -> Result<Running, MachineError> {
    if observe(spec)? != State::Running {
        checked(spec, CLI, &["start", &spec.vm_name], CREATE)?;
    }
    let endpoint = worker::bring_up(spec, &guest(spec), bundle, port, None)?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..spec.state
        },
    })
}
