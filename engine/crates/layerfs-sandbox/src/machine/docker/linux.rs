//! `docker` on Linux: Store calls run in the caller's process and Sandboxes
//! use the host's Docker, so there is no worker to start or stop.
use super::cli::engine_version;
use crate::machine::error::{Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::setup::{State, Status};
use crate::machine::spec::{Bundle, Running, Spec};

/// Not reached: the route is in-process, so `machine::start` starts nothing.
pub(crate) fn start(spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
    Err(MachineError::new(
        Code::InvalidInput,
        format!(
            "the docker machine {} runs Store calls in-process on Linux; it has no worker",
            spec.name
        ),
    ))
}

/// Not reached: the machine's files are the host's own.
pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    Err(MachineError::new(
        Code::InvalidInput,
        format!(
            "the docker machine {} runs in-process on Linux; it has no guest",
            spec.name
        ),
    ))
}

pub(crate) fn stop(_spec: &Spec) -> Result<(), MachineError> {
    Ok(())
}

/// Running while the Engine answers; free disk is the caller's to read from
/// Docker's volumes folder, which this process can see directly.
pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = engine_version(spec);
    let down = version.is_none().then(|| {
        format!(
            "the Docker Engine at {} does not answer",
            spec.tools.docker_socket.display()
        )
    });
    Ok(Status {
        state: if down.is_none() {
            State::Running
        } else {
            State::Stopped
        },
        free_disk: None,
        tool_version: version,
        down_reason: down,
    })
}
