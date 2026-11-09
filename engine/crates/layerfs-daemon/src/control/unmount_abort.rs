//! What one normal unmount observed when its drain stalled at `Join` and it
//! took the abort path (issues #426, #460; observation added for #2).
//! Observation only: it is built where that path ends, is encoded in no reply
//! field, and carries no authority to retry, abort again or release an owner.

#[cfg(target_os = "linux")]
use layerfs_bridge::control::{MountWork, TeardownStage, WorkspaceToken};
#[cfg(target_os = "linux")]
use layerfs_fuse::mount::AbortWrite;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// The abort path of one normal unmount, as it stood when the unmount was
/// completed or stopped. The same facts reach a traced caller as the
/// `unmount` telemetry section and the daemon's one stderr line.
#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
pub struct UnmountAbort {
    /// The exact incarnation whose connection was aborted.
    pub token: WorkspaceToken,
    /// The boundary the unmount was at: `Join` when the one abort write ended
    /// a stalled drain, `Abort` when that write was short or failed, and
    /// `Revoke` or `Close` when a later step stopped with it recorded.
    pub stage: TeardownStage,
    /// The connection's single abort write and its original result.
    pub abort: AbortWrite,
    /// Kernel detach was already known when the abort was made.
    pub detached: bool,
    /// Loop, lane and request counters observed at the stop or completion.
    pub work: MountWork,
    /// Time from the stalled drain to the end of the abort path.
    pub waited: Duration,
}

/// Kernel sessions exist only on Linux, so no other platform reaches the
/// abort path and none carries an observation.
#[cfg(not(target_os = "linux"))]
#[derive(Clone, Debug)]
pub enum UnmountAbort {}
