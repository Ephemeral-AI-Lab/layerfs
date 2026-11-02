//! `docker` through WSL2 on Windows: not designed (H §7.10.1 leaves its cfg
//! and dependency plan open), so every call answers `Unsupported`.
use crate::error::MachineError;
use crate::setup::guest::Guest;
use crate::setup::Status;
use crate::spec::{Bundle, Running, Spec};

fn unsupported(spec: &Spec) -> MachineError {
    MachineError::missing(
        format!(
            "the docker machine {} through WSL2 is not designed yet",
            spec.name
        ),
        vec!["host:wsl2-worker".into()],
    )
}

pub(crate) fn start(spec: &Spec, _bundle: &Bundle) -> Result<Running, MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    Err(unsupported(spec))
}
