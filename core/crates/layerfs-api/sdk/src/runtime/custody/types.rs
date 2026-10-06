//! Original fixed-slot capabilities, receipts and exact pre-effect refusals.
use super::FailureKnowledge;
use crate::runtime::{Binding, Completion, HistoryReceipts, SaveId, Sessions};
use std::{collections::TryReserveError, fmt};

/// Local disposition after the serving scope has explicitly ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustodyDisposition {
    /// An active producer ended without synthesizing a Finish or Abort receipt.
    /// Earlier acknowledged publication waves can still exist.
    ActiveProducerEnded,
    /// Original terminal result is known. An acknowledged stage may still be held;
    /// this classification does not grant release/discard or publication success.
    KnownTerminal,
    /// Known failure whose cleanup remains incomplete, with original errors retained.
    RetainedFailure,
    /// Exact original Save/history uncertainty remains terminal; no resolver is supplied.
    TerminalUnknown,
}
/// Original occupied processing slot after its producer and serving scope ended.
#[derive(Debug)]
pub struct SlotCustody {
    pub(super) binding: Binding,
    pub(super) save: SaveId,
    pub(super) completion: Option<Completion>,
    pub(super) history: HistoryReceipts,
    pub(super) producer_ended: bool,
    pub(super) disposition: CustodyDisposition,
    pub(super) save_knowledge: FailureKnowledge,
    pub(super) retains_custody: bool,
}
impl SlotCustody {
    /// Original binding, including captured Branch/root/scope/profile expectations.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    /// Original local capability; token() exposes its exact original wire identity.
    pub const fn save(&self) -> SaveId {
        self.save
    }
    /// Original completion and phase, absent if no terminal attempt was recorded.
    pub fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }
    /// Original independent Stage/Commit/Discard receipts and exact token/disposition.
    pub fn history(&self) -> &HistoryReceipts {
        &self.history
    }
    /// True only when this fence actually dropped a remaining local Save producer.
    pub const fn producer_ended(&self) -> bool {
        self.producer_ended
    }
    /// Exact local classification, without inferring remote consumption/publication.
    pub const fn disposition(&self) -> CustodyDisposition {
        self.disposition
    }
    /// Original Save failure knowledge, independent of every history receipt.
    pub const fn save_knowledge(&self) -> FailureKnowledge {
        self.save_knowledge
    }
    /// Whether an original recorded history attempt has unknown persistence outcome.
    /// False means no recorded history uncertainty; it is not a fresh Branch read.
    pub fn history_unknown(&self) -> bool {
        self.history.terminal_unknown()
    }
    /// Original release gate, including known acknowledged stages and cleanup failure.
    /// Ending the scope never resolves these owners or authorizes their deletion.
    pub const fn retains_custody(&self) -> bool {
        self.retains_custody
    }
    /// Moves every original capability/result to the application's chosen owner.
    /// No errors are cloned and no provider action occurs.
    pub fn into_parts(self) -> (Binding, SaveId, Option<Completion>, HistoryReceipts) {
        (self.binding, self.save, self.completion, self.history)
    }
}
/// Original per-fixed-slot custody after one explicit consuming local scope fence.
/// This is application-owned data, with no borrowed Storage/Save or recovery authority.
#[derive(Debug)]
pub struct ServingCustody {
    pub(super) incarnation: [u8; 32],
    pub(super) slots: Vec<Option<SlotCustody>>,
}
impl ServingCustody {
    /// Original authority-assigned runtime incarnation, never refreshed by fencing.
    pub const fn incarnation(&self) -> [u8; 32] {
        self.incarnation
    }
    /// Every original processing slot in order, including its vacant positions.
    pub fn slots(&self) -> &[Option<SlotCustody>] {
        &self.slots
    }
    /// Moves the original fixed-slot records out once, without replay or reattachment.
    pub fn into_slots(self) -> Vec<Option<SlotCustody>> {
        self.slots
    }
}
/// Original scope-fence refusal before any producer or receipt is moved/dropped.
#[derive(Debug)]
pub enum ScopeFenceError {
    /// A Service/job/caller-held completion still owns this serving scope.
    RetainedCustody {
        /// Exact service-owner gauge at the refusal boundary.
        owners: usize,
    },
    /// Original fixed report allocation refusal, before changing any slot.
    Allocation {
        /// Requested fixed processing-slot positions.
        slots: usize,
        /// Original allocator refusal.
        error: TryReserveError,
    },
}
impl fmt::Display for ScopeFenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetainedCustody { owners } => {
                write!(f, "serving scope retains {owners} service owners")
            }
            Self::Allocation { slots, error } => {
                write!(f, "serving custody allocation for {slots} slots: {error}")
            }
        }
    }
}
impl std::error::Error for ScopeFenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation { error, .. } => Some(error),
            _ => None,
        }
    }
}
/// Original scope returned intact after a pre-effect fence refusal.
pub struct ScopeFenceRefusal<'a> {
    /// Exact deciding owner/allocation refusal.
    pub error: ScopeFenceError,
    /// Original registry, producers, receipts and borrowed initialized owners.
    pub sessions: Sessions<'a>,
}
impl fmt::Debug for ScopeFenceRefusal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeFenceRefusal")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
