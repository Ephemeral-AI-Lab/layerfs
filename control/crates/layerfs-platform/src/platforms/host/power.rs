//! `native` power: `start`.
use crate::error::{Code, MachineError};
use crate::spec::{Running, Spec};

/// Only reached where `native`'s route is a worker (Windows).
pub(crate) fn start(spec: &Spec) -> Result<Option<Running>, MachineError> {
    Err(MachineError::new(
        Code::Unsupported,
        format!(
            "the native machine {} on this host runs its Stores in a WSL2 worker, which is not designed yet",
            spec.name
        ),
    ))
}
