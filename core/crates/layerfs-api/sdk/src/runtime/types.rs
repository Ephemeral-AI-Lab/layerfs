//! Local capabilities and borrowed completion/delivery ownership.
use super::{RuntimeError, RuntimeResult};
use layerfs_content::ObjectId;
use layerfs_storage::save::WriteOutcome;

/// Typed local Save capability; no public untrusted-field constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SaveId {
    pub(super) owner: [u8; 32],
    pub(super) slot: usize,
    pub(super) serial: u64,
}

/// Retained exact one-attempt Save completion, borrowed by the caller.
#[derive(Debug)]
pub struct Completion {
    pub(super) phase: CompletionPhase,
    pub(super) outcome: Result<WriteOutcome, RuntimeError>,
}
/// Attempt which produced a retained terminal receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionPhase {
    /// Object admission/accept refused and ended the producer.
    Accept,
    /// Save finish was attempted once.
    Finish,
    /// Explicit abort ended the producer without deleting acknowledged output.
    Abort,
}
impl Completion {
    /// Exact phase; an accept failure is not a failed history transition.
    pub const fn phase(&self) -> CompletionPhase {
        self.phase
    }
    /// Original success or failure; inspecting this does not replay finish.
    pub fn outcome(&self) -> &Result<WriteOutcome, RuntimeError> {
        &self.outcome
    }
    pub(super) fn retains_custody(&self) -> bool {
        match &self.outcome {
            Err(RuntimeError::Storage(error)) => matches!(
                error.as_ref(),
                layerfs_storage::StorageError::UnknownOutcome { .. }
                    | layerfs_storage::StorageError::CleanupFailed { .. }
            ),
            _ => false,
        }
    }
}

/// Synchronous reply sink, after provider work releases its locks.
pub trait ObjectReply {
    /// Consumes one authenticated object in demand order without taking ownership.
    /// Transport must reserve its own bounded delivery capacity before copying.
    fn object(&mut self, id: ObjectId, canonical: &[u8]) -> RuntimeResult<()>;
}
/// Synchronous metadata delivery after owning provider work releases its locks.
pub trait LengthReply {
    /// Delivers one trusted Store file-length fact in demand order.
    fn file_length(&mut self, id: ObjectId, logical_len: u64) -> RuntimeResult<()>;
}
