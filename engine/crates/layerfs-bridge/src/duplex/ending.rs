//! Original terminal socket observation, separate from filesystem ownership/drain.
use super::{ChannelError, ChannelWork};
/// Exact original terminal boundary; a read error and an EOF followed by failed fence differ.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerEndPhase {
    /// Owner was already terminal before any socket read.
    Admit,
    /// One original terminal read or trailing-input refusal; channel error retains any failed fence.
    Read,
    /// Clean peer EOF was observed, then the original local fence boundary entered.
    Fence,
}
/// One terminal EOF/read/fence failure; it never resumes record reception.
#[derive(Debug)]
pub struct PeerEndFailure {
    /// Original socket/protocol/fence error.
    pub cause: ChannelError,
    /// Exact original phase, never inferred from error code.
    pub phase: PeerEndPhase,
    /// Whether this original terminal operation actually entered shutdown; false on prior quarantine.
    pub fence_attempted: bool,
    /// Original single trailing byte if the peer sent input after the final reply.
    pub received: Option<u8>,
    /// Actual cumulative receive work at the original stopping point.
    pub work: ChannelWork,
}
