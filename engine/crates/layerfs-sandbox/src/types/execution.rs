//! Process observation is independent of pipe EOF and filesystem drain.
use super::{ContainerId, ExecId, RuntimeError};
/// Explicit nonroot identity for ordinary commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandIdentity {
    pub uid: u32,
    pub gid: u32,
}
impl CommandIdentity {
    pub(crate) fn check(self) -> Result<(), RuntimeError> {
        if self.uid == 0 || self.gid == 0 {
            Err(RuntimeError::Protocol("nonroot command uid/gid"))
        } else {
            Ok(())
        }
    }
}
/// One coherent runtime inspection; it never settles an earlier lost operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecInspection {
    pub container: ContainerId,
    pub exec: ExecId,
    pub running: bool,
    /// Null remains unavailable/unstarted; never substituted with zero.
    pub exit_code: Option<i32>,
    /// VM runtime namespace PID, diagnostic only; never used as a signal target.
    pub pid: Option<u32>,
}
impl ExecInspection {
    /// Actual known root exit code. Descendants, streams and filesystem owners are independent.
    pub fn known_root_exit(&self) -> Option<i32> {
        if self.running {
            None
        } else {
            self.exit_code
        }
    }
}
