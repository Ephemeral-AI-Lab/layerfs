//! `host` power: `start`.
use crate::platform::error::{Code, MachineError};
use crate::setup::spec::{Running, Spec};

/// Only reached where `host`'s route is a worker (Windows).
pub(crate) fn start(spec: &Spec) -> Result<Option<Running>, MachineError> {
    Err(MachineError::new(
        Code::Unsupported,
        format!(
            "the host machine {} on this host runs its Stores in a WSL2 worker, which is not designed yet",
            spec.name
        ),
    ))
}
