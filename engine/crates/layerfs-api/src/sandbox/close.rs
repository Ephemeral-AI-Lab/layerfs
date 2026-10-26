//! Explicit stop then delete of the exact acknowledged Sandbox runtime owner.
use super::{ManagedSandbox, SandboxApi, SandboxCause, SandboxFailure, SandboxPhase};

impl SandboxApi {
    /// Stops once, then deletes once after acknowledged stop. No Workspace drain is inferred.
    /// A failed attempt retains the original runtime, control and deployment inputs; no replay.
    pub fn close(&self, mut sandbox: ManagedSandbox) -> Result<(), Box<SandboxFailure>> {
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
    sandbox: ManagedSandbox,
    phase: SandboxPhase,
    cause: SandboxCause,
) -> Box<SandboxFailure> {
    Box::new(SandboxFailure {
        request: sandbox.request,
        phase,
        sandbox: Some(sandbox.runtime),
        control: Some(sandbox.control),
        handshake: sandbox.handshake,
        observed: Some(sandbox.startup),
        installed: sandbox.installed,
        cause,
    })
}
