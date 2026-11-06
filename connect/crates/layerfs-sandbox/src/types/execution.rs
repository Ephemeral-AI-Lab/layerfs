//! Process observation is independent of pipe EOF and filesystem drain.
#[cfg(unix)]
use super::RuntimeError;
use super::{ContainerId, ExecId};
use std::time::Duration;
/// Explicit nonroot identity for ordinary commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandIdentity {
    pub uid: u32,
    pub gid: u32,
}
impl CommandIdentity {
    /// Read by the backends only, which are Unix only.
    #[cfg(unix)]
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

/// Where a running command reports that its process is still alive, and how
/// often. This is a liveness source, never an operation duration: a caller
/// that forwards it as progress keeps waiting while the process lives, and a
/// process that has exited (including a zombie) stops the reports.
pub struct Liveness<'a> {
    /// How often the process is observed. A report is at most this late.
    pub every: Duration,
    /// Called while that process is alive, with its id when the backend knows
    /// one. It never blocks the command: the observer runs beside the streams.
    pub report: &'a mut (dyn FnMut(Option<u32>) + Send),
}
