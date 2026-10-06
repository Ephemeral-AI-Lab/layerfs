//! Received bodies retain live transport credit through caller custody.
use super::credits::Credit;
use crate::contract::{Envelope, MessageKind};
/// Selected live receive windows; no total operation/flow/file/duration ceiling.
#[derive(Clone, Copy, Debug)]
pub struct ReassemblyConfig {
    /// Expected direction on this receive owner.
    pub kind: MessageKind,
    /// Simultaneous partial or caller-retained messages.
    pub messages: usize,
    /// Message slots Save bodies cannot consume, preserving demand progress.
    pub demand_messages: usize,
    /// Message slots Save/demand bodies cannot consume, preserving control.
    pub control_messages: usize,
    /// Largest bounded adapter message, distinct from a complete file/Save.
    pub message_bytes: usize,
    /// Aggregate body/receipt byte credit.
    pub bytes: usize,
    /// Save bodies cannot consume this demand allowance.
    pub demand_reserve: usize,
    /// Save/demand bodies cannot consume this control allowance.
    pub control_reserve: usize,
}
/// Fixed observations from actual fragment admission/copying.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReassemblyWork {
    /// Admitted logical messages.
    pub admitted: u64,
    /// Original body allocation attempts, including failed allocation.
    pub allocation_attempts: u64,
    /// Declared body capacity requested, not newly resident heap.
    pub requested_body_bytes: u64,
    /// Credit refusals, without adapter work.
    pub refused: u64,
    /// Copied authenticated body fragments.
    pub fragments: u64,
    /// Exact fragment bytes copied into owned bodies.
    pub copied_bytes: u64,
    /// Messages owning live credit, including caller-retained completions.
    pub live_messages: usize,
    /// Current body and fixed receipt credit.
    pub credited_bytes: usize,
    /// Largest observed credit, not process/kernel residency.
    pub peak_credited_bytes: usize,
}
/// Original complete or disconnect-fenced partial message and its byte credit.
/// Partial bodies are unattempted input, never a completed adapter response.
pub struct Message {
    pub(super) envelope: Envelope,
    pub(super) body: Vec<u8>,
    pub(super) credit: Credit,
}
impl Message {
    /// Original immutable envelope.
    pub const fn envelope(&self) -> Envelope {
        self.envelope
    }
    /// Actual owned bytes; a partial body is retained without padding/replay.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
    /// True only when all declared bytes are present.
    pub fn complete(&self) -> bool {
        self.body.len() as u64 == self.envelope.total_bytes
    }
    /// Real body allocation capacity under this message's retained credit.
    pub fn capacity(&self) -> usize {
        self.body.capacity()
    }
    /// Borrow mutable complete bytes for an owning codec without another payload copy.
    /// The message and its credit must stay alive while those bytes are used.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.body
    }
}

/// Opaque receive allocation/receipt credit, transferable with decoded payload.
/// Keep this lease until body ownership has moved to an admitted downstream owner.
pub struct MessageLease {
    pub(super) credit: Credit,
}
impl Message {
    /// Moves the original body without a full payload copy while keeping its
    /// aggregate transport credit in an independent opaque lease.
    pub fn into_parts(self) -> (Envelope, Vec<u8>, MessageLease) {
        (
            self.envelope,
            self.body,
            MessageLease {
                credit: self.credit,
            },
        )
    }
}
impl MessageLease {
    /// Current aggregate credit observation while this original body is retained.
    pub fn work(&self) -> crate::contract::FrameResult<ReassemblyWork> {
        Ok(self
            .credit
            .ledger
            .lock()
            .map_err(|_| crate::contract::FrameError::Poisoned)?
            .work)
    }
}
