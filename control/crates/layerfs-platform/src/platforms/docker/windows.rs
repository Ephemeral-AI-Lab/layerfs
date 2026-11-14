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

pub(crate) fn stop(spec: &Spec, _force: bool) -> Result<(), MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn load_image(spec: &Spec, _archive: &std::path::Path) -> Result<String, MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn host_address(spec: &Spec) -> Result<String, MachineError> {
    Err(unsupported(spec))
}

/// Provider observation on unsupported native Windows.
pub(crate) fn provider_state(spec: &Spec) -> Result<crate::setup::State, MachineError> {
    status(spec).map(|value| value.state)
}

/// Image reuse is unavailable on unsupported hosts.
pub(crate) fn cached_image(
    spec: &Spec,
    _archive: &std::path::Path,
    _content: &str,
    _previous: Option<&str>,
) -> Result<String, MachineError> {
    Err(unsupported(spec))
}
