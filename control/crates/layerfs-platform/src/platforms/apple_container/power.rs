//! `apple_container` power: see `mod.rs`.
use super::observe::{gateway, guest, observe, CLI};
use crate::error::{Code, MachineError};
use crate::setup::vm::{checked, not_found, recorded_port, CREATE};
use crate::setup::worker;
use crate::setup::State;
use crate::spec::{Bundle, Lasting, Running, Spec};

/// Starts a stopped container, and tries a failed one once: `container
/// start` runs its main process again over the same volume, and the
/// documented fix of a `failed` machine is `machine start`; the CLI's own
/// answer decides. Lima refuses its `Broken` before any call, because
/// `limactl start` refuses such an instance itself (`lima/power.rs`).
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

/// `container stop` keeps the container and its volume. A forced stop gives
/// it no grace (`--time 0`, as docker's `-t 0`) and also stops a failed
/// container or one between states (`Unavailable`), which an ordinary stop
/// leaves alone.
pub(crate) fn stop(spec: &Spec, force: bool) -> Result<(), MachineError> {
    let stoppable = match observe(spec) {
        Ok(state) => state == State::Running || (force && state == State::Failed),
        Err(passing) if force && passing.code == Code::Unavailable => true,
        Err(error) => return Err(error),
    };
    if stoppable {
        let mut words = vec!["stop"];
        if force {
            words.extend(["--time", "0"]);
        }
        words.push(&spec.vm_name);
        checked(spec, CLI, &words, CREATE)?;
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
