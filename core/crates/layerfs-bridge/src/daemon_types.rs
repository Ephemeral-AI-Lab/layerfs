//! Authenticated daemon startup observation, separate from native Workspace Ready.
/// Maintained application phase; observing it never settles an earlier unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DaemonPhase {
    /// Overlay/listener exist; no installed Store or control Service is claimed.
    InstallPending = 1,
    /// One original installation conversation is admitted.
    Installing = 2,
    /// Original startup failure/custody is retained, without a fallback open.
    Retained = 3,
    /// Actual direct Store and Overlay-backed control Service are published.
    ControlReady = 4,
}
/// One correlated startup observation or explicitly requested startup wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelloRequest {
    /// Previously observed incarnation, or None for first authenticated discovery.
    pub expected_instance: Option<[u8; 32]>,
    /// Wait for the one startup outcome event; no attempted operation is replayed.
    pub wait_for_store: bool,
}
/// Actual daemon lifecycle and provider facts, with no credentials or FUSE claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonStatus {
    /// Fresh process incarnation; unrelated to history/PID/time allocation.
    pub instance: [u8; 32],
    /// Maintained startup/service phase.
    pub phase: DaemonPhase,
    /// Actual initialized Overlay version when established.
    pub overlay_sqlite: Option<String>,
    /// Actual validated global Store version when established.
    pub store_sqlite: Option<String>,
}
