//! Real single-turn observations and borrowed permission for an idle rotation.
use super::{AttachmentId, SupervisorEvent};
use crate::runtime::{service::Ticket, wake::Signal};
use layerfs_bridge::contract::{FrameError, MessageClass};
use std::time::Instant;

/// The selected owner's existing phase before its one bounded turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorStage {
    /// Awaiting/admitting the original header and transferring its grant/refusal.
    Header,
    /// Waiting for the original grant's send receipt before permitting body receive.
    Grant,
    /// Awaiting and decoding the original complete credited request body.
    Body,
    /// Reserving final output and submitting the decoded request once.
    Ready,
    /// Original request admitted under the existing fair provider-service ticket.
    Queued,
    /// Encoding/transferring the original completion or service-admission refusal.
    Result,
    /// Waiting for the final local-send receipt to return its original Delivery.
    Reply,
    /// Waiting for a denied header's send receipt before input refusal/fencing.
    Refusal,
    /// Exact connection already fenced; original native worker joins are observed.
    Fence,
}
/// An observed unavailable prerequisite, not a native error or publication outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorWait {
    /// No original header event was available from the bounded input receiver.
    InputHeader,
    /// No complete original body event was available from the bounded input receiver.
    InputBody,
    /// No original grant send receipt was available from the output receiver.
    GrantReceipt,
    /// No original pre-body refusal send receipt was available from the output receiver.
    RefusalReceipt,
    /// No original final-reply send receipt was available from the output receiver.
    ReplyReceipt,
    /// The existing output count/byte window could not reserve this original reply.
    OutputCredit {
        /// Actual output reserve class; this is distinct from provider service class.
        class: MessageClass,
        /// Requested packet-capacity allowance before fixed output ownership charges.
        capacity: usize,
    },
    /// Needs the existing local service turn, not an asynchronous completion wait.
    ServiceTurn,
    /// Actual joins remain pending. This never permits parking the Supervisor.
    WorkerJoin {
        /// True while the original input worker's actual join remains unobserved.
        input: bool,
        /// True while the original output worker's actual join remains unobserved.
        output: bool,
    },
}
/// Selected attachment observation before the separate provider-service step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentTurn {
    /// Exact initialized attachment selected by the ordinary rotating turn.
    pub attachment: AttachmentId,
    /// Original exchange correlation, absent before an original header is acquired.
    pub correlation: Option<u64>,
    /// Existing phase before the selected turn, not a synthesized provider outcome.
    pub stage: SupervisorStage,
    /// Header acquisition and partial joins count even without a completed event.
    pub progressed: bool,
    /// Progress and waiting can coexist, for example a newly acquired parked header.
    pub wait: Option<SupervisorWait>,
}
/// The one original provider unit dispatched after the selected attachment turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderTurn {
    /// Exact attachment whose admitted original request was dispatched.
    pub attachment: AttachmentId,
    /// Original correlation retained by that attachment's exchange.
    pub correlation: u64,
    /// Exact service-minted ticket dispatched once; it is not a replay capability.
    pub ticket: Ticket,
}
/// Prerequisite classes seen in one complete occupied rotation with no progress.
/// These flags do not attribute socket, CPU, resource or physical latency causes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParkReasons {
    /// At least one occupied owner had no original header event in this rotation.
    pub input_header: bool,
    /// At least one occupied owner had no complete original body event.
    pub input_body: bool,
    /// At least one original grant/refusal/final send receipt was unavailable.
    pub output_receipt: bool,
    /// At least one original reply reservation lacked output count/byte credit.
    pub output_credit: bool,
    /// The initialized attachment registry was empty; no global quiescence is implied.
    pub no_attachments: bool,
}
impl ParkReasons {
    pub(super) fn include(&mut self, wait: SupervisorWait) {
        match wait {
            SupervisorWait::InputHeader => self.input_header = true,
            SupervisorWait::InputBody => self.input_body = true,
            SupervisorWait::GrantReceipt
            | SupervisorWait::RefusalReceipt
            | SupervisorWait::ReplyReceipt => self.output_receipt = true,
            SupervisorWait::OutputCredit { .. } => self.output_credit = true,
            SupervisorWait::ServiceTurn | SupervisorWait::WorkerJoin { .. } => (),
        }
    }
}
/// Non-clonable permission borrowing the owner after one complete idle rotation.
/// Its borrow prevents step/attach/fence from invalidating it before the wait.
pub struct SupervisorPark<'s> {
    pub(super) signal: &'s Signal,
    pub(super) reasons: ParkReasons,
}
impl SupervisorPark<'_> {
    /// Prerequisite classes observed during the complete no-progress rotation.
    pub const fn reasons(&self) -> ParkReasons {
        self.reasons
    }
    /// Wait only until the caller's bound. No provider/SQL/Workspace lock is held;
    /// notification is checked under the same latch lock used by producers.
    /// Inspect application-owned queues after obtaining this permit and before
    /// waiting; the Supervisor's idle rotation cannot inspect those queues.
    pub fn wait_until(self, deadline: Instant) -> Result<super::SupervisorWake, FrameError> {
        self.signal.wait_until(deadline)
    }
}
/// One identical fair turn, its exact event and optional global park permission.
pub struct SupervisorTurn<'s> {
    /// Original completed local Delivery/fence, absent even when a phase advanced.
    pub event: Option<SupervisorEvent>,
    /// Selected phase observation, absent only when no occupied attachment was selected.
    pub attachment: Option<AttachmentTurn>,
    /// The one dispatched original provider unit, including its original error outcome.
    pub provider: Option<ProviderTurn>,
    /// Available only after a complete no-progress rotation with no pending joins.
    pub park: Option<SupervisorPark<'s>>,
    /// Latch availability is separate from native/provider outcomes and custody.
    pub wake_error: Option<FrameError>,
}
impl SupervisorTurn<'_> {
    /// Whether a selected phase/join, original event or provider unit advanced.
    /// Unconsumed background socket activity alone is not observed by this method.
    pub fn progressed(&self) -> bool {
        self.provider.is_some()
            || self.event.is_some()
            || self.attachment.is_some_and(|turn| turn.progressed)
    }
}
