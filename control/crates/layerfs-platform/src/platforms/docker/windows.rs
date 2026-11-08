//! `docker` through WSL2 on Windows: not designed (H §7.10.1 leaves its cfg
//! and dependency plan open), so every call answers `Unsupported`.
use crate::error::MachineError;
use crate::spec::{Bundle, Running, Spec};

// The Guest/Status types the stubs name are unix-side setup types; on
// Windows every call answers Unsupported before one is built, so the
// signatures below say the same with plain types.

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

pub(crate) fn guest(spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn status(spec: &Spec) -> Result<(), MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn load_image(spec: &Spec, _archive: &std::path::Path) -> Result<String, MachineError> {
    Err(unsupported(spec))
}

pub(crate) fn host_address(spec: &Spec) -> Result<String, MachineError> {
    Err(unsupported(spec))
}

/// Provider observation on unsupported native Windows: the stub refuses.
pub(crate) fn provider_state(spec: &Spec) -> Result<crate::windows_stubs::stubs::Status, MachineError> {
    Err(unsupported(spec))
}
