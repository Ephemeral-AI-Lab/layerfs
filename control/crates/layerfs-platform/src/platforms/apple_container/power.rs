//! `apple_container` power: see `mod.rs`.
use super::observe::{gateway, guest, observe, CLI};
use crate::error::MachineError;
use crate::setup::vm::{checked, not_found, recorded_port, CREATE};
use crate::setup::worker;
use crate::setup::State;
use crate::spec::{Bundle, Lasting, Running, Spec};

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    match observe(spec)? {
        State::Missing => return Err(not_found(spec)),
        State::Running => {}
        State::Stopped | State::Failed => {
            checked(spec, CLI, &["start", &spec.vm_name], CREATE)?;
        }
    }
    bring_up(spec, bundle, port)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], CREATE)?;
    }
    Ok(())
}

/// Brings the worker up and records the container network's gateway, read
/// once here (#66): every start replaces it, and a Sandbox's PostgreSQL
/// reach uses it without running `container inspect`. A gateway this start
/// could not read is recorded as none; the reach then looks it up itself,
/// and refuses as before when the machine reports none.
pub(super) fn bring_up(spec: &Spec, bundle: &Bundle, port: u16) -> Result<Running, MachineError> {
    let endpoint = worker::bring_up(spec, &guest(spec), bundle, port)?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            gateway: gateway(spec).ok().and_then(|g| g.parse().ok()),
        },
    })
}
