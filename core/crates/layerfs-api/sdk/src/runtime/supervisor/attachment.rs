//! Bounded per-connection exchange state, independently fenced native directions.
use super::{AttachmentId, RequestCustody, SupervisorFailure};
use crate::runtime::service::*;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Phase {
    Header,
    Grant,
    Body,
    Ready,
    Queued,
    Result,
    Reply,
    Refusal,
}
pub(super) struct Exchange {
    pub custody: RequestCustody,
    pub phase: Phase,
    pub ticket: Option<Ticket>,
}
pub(super) struct Attachment {
    pub id: AttachmentId,
    pub input: NativeInput,
    pub output: NativeOutput,
    pub exchange: Option<Exchange>,
    pub service_fence: Option<DisconnectFence>,
    pub input_fence: Option<InputFence>,
    pub output_fence: Option<OutputFence>,
    pub failure: Option<SupervisorFailure>,
    pub input_events: Vec<InputEvent>,
    pub output_receipts: Vec<OutputReceipt>,
}
