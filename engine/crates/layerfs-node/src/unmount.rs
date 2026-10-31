//! Workspace-scoped unmount ownership slot; implemented by the native unmount lease.
use crate::{job::Outcome, registry::SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue, WorkspaceToken};

pub(crate) struct Attempt;

pub(crate) fn check(_state: &SandboxState, _path: &str) -> Result<(), SdkRefusal> {
    Ok(())
}

pub(crate) fn find(_state: &SandboxState, _path: &str) -> Result<Option<SdkValue>, SdkRefusal> {
    Ok(None)
}

pub(crate) fn check_all(_state: &SandboxState) -> Result<Vec<String>, SdkRefusal> {
    Ok(Vec::new())
}

pub(crate) fn begin(
    _state: &SandboxState,
    _token: WorkspaceToken,
    _path: &str,
) -> Result<Attempt, SdkRefusal> {
    Ok(Attempt)
}

pub(crate) fn finish(
    _attempt: &Attempt,
    _path: &str,
    _outcome: &Outcome,
) -> Result<(), SdkRefusal> {
    Ok(())
}

pub(crate) fn cancel(_attempt: &Attempt) -> Result<(), SdkRefusal> {
    Ok(())
}

pub(crate) fn cleanup_failure(outcome: Outcome, _error: SdkRefusal) -> Outcome {
    outcome
}

pub(crate) fn repeat(value: &SdkValue) -> Outcome {
    Ok(value.clone())
}
