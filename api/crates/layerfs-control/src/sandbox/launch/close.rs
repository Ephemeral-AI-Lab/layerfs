//! Explicit stop then delete of the exact acknowledged Sandbox runtime owner.
use super::{ProviderFailure, ProviderSandbox, SandboxApi, SandboxCause, SandboxPhase};

impl SandboxApi {
    /// Stops once, then deletes once after acknowledged stop. No Workspace drain is inferred.
    /// A failed attempt retains the original runtime, control and deployment inputs; no replay.
    pub fn close(&self, mut sandbox: ProviderSandbox) -> Result<(), Box<ProviderFailure>> {
        if let Err(cause) = sandbox.runtime.stop(1) {
            return Err(failure(
                sandbox,
                SandboxPhase::Stop,
                SandboxCause::Runtime(cause),
            ));
        }
        if let Err(cause) = sandbox.runtime.delete() {
            return Err(failure(
                sandbox,
                SandboxPhase::Delete,
                SandboxCause::Runtime(cause),
            ));
        }
        Ok(())
    }
}
fn failure(
    sandbox: ProviderSandbox,
    phase: SandboxPhase,
    cause: SandboxCause,
) -> Box<ProviderFailure> {
    Box::new(ProviderFailure {
        request: sandbox.request,
        phase,
        runtime: Some(sandbox.runtime),
        control: Some(sandbox.control),
        handshake: sandbox.handshake,
        observed: Some(sandbox.startup),
        installed: sandbox.installed,
        cause,
    })
}
