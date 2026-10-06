//! Exact original attachment, request, delivery and fence ownership.
use crate::{
    client::RequestHeader,
    runtime::{service::*, RuntimeError, WireRequest},
};
use layerfs_bridge::{
    codec::{Message, MessageLease, ReassemblyConfig},
    contract::{Envelope, FrameError, MessageKind},
    native::{Connection, Sender},
};

/// Live windows for the initialized host serving scope; no lifetime flow cap.
pub struct SupervisorConfig {
    /// Existing bounded fair provider service windows.
    pub service: ServiceConfig,
    /// Shared authenticated request-body admission across input workers/reports.
    pub input: ReassemblyConfig,
    /// Shared reply admission across workers, permits, packets and receipts.
    pub output: OutputConfig,
}
/// Exact original initialized-supervisor configuration/allocation refusal.
#[derive(Debug)]
pub enum SupervisorStartError {
    /// Existing runtime service or fixed-registry admission refusal.
    Runtime(RuntimeError),
    /// Existing authenticated transport-window configuration refusal.
    Frame(FrameError),
}
impl From<RuntimeError> for SupervisorStartError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}
impl From<FrameError> for SupervisorStartError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}
impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            service: ServiceConfig::default(),
            input: ReassemblyConfig {
                kind: MessageKind::Request,
                messages: 128,
                demand_messages: 1,
                control_messages: 1,
                message_bytes: (16 << 20) + crate::client::REQUEST_HEADER_BYTES,
                bytes: 128 << 20,
                demand_reserve: 256 << 10,
                control_reserve: 128 << 10,
            },
            output: OutputConfig {
                owners: 32,
                messages: 128,
                demand_messages: 1,
                control_messages: 1,
                bytes: 128 << 20,
                demand_reserve: 34 << 20,
                control_reserve: 128 << 10,
            },
        }
    }
}
/// Original authenticated attachment; private identity prevents a forged route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentId(pub(super) ConnectionId);
/// Original unattempted attach refusal, including any already-started I/O owner.
pub enum AttachFailure {
    /// Binding/service admission refused before moving the native connection.
    Runtime {
        /// Original runtime/binding admission refusal.
        error: RuntimeError,
        /// Original authenticated native owner, without socket I/O.
        connection: Connection,
    },
    /// Input startup refused with the original authenticated connection.
    Input {
        /// Original input admission/start refusal.
        error: InputFailure,
        /// Original authenticated native owner.
        connection: Connection,
    },
    /// Output startup refused after input started. Input has been fenced; its
    /// owner and original sender return to the caller for explicit join/custody.
    Output {
        /// Original output admission/start refusal.
        error: OutputStartError,
        /// Original fenced input owner, awaiting explicit detach/join.
        input: NativeInput,
        /// Original unattempted send direction.
        sender: Sender,
    },
}
/// Original fully received body refused before adapter dispatch.
pub struct RefusedInput {
    /// Exact deciding service admission error.
    pub error: RuntimeError,
    /// Original decoded request, never submitted again by the supervisor.
    pub request: Request,
    /// Original receive lease while the body remains caller-owned.
    pub lease: MessageLease,
}
/// Original request custody returned only after the attachment is fenced.
pub struct RequestCustody {
    /// Exact original correlation/class/message/length facts.
    pub envelope: Envelope,
    /// Original checked header, with no refreshed expectations.
    pub header: RequestHeader,
    /// Received, decoded input not yet admitted to provider service.
    pub input: Option<WireRequest>,
    /// Original malformed complete input and its receive credit.
    pub rejected: Option<Message>,
    /// Original typed result/cancelled request and service credit.
    pub completion: Option<ServiceCompletion>,
    /// Original fully received request refused without provider invocation.
    pub refused: Option<RefusedInput>,
    /// Original pre-body admission refusal, when applicable.
    pub admission_error: Option<RuntimeError>,
    /// Encoded packet not transferred to output, with its reserved capacity.
    pub unsent: Option<(OutputPacket, OutputPermit)>,
    /// Reserved capacity whose original reply has not yet been encoded.
    pub reservation: Option<OutputPermit>,
}
/// Exact local send completion; holding it retains provider and output credits.
/// Complete local socket write does not prove remote consumption/publication.
pub struct Delivery {
    /// Original attachment.
    pub attachment: AttachmentId,
    /// Original checked request facts.
    pub header: RequestHeader,
    /// Original typed adapter result, absent for pre-dispatch refusal.
    pub completion: Option<ServiceCompletion>,
    /// Original body refused before provider dispatch, when present.
    pub refused: Option<RefusedInput>,
    /// Original local socket-send receipt and packet bytes/credit.
    pub output: OutputReceipt,
}
/// Exact local orchestration failure; native worker causes remain in their fences.
pub enum SupervisorFailure {
    /// Checked frame/encoding/decision refusal.
    Frame(FrameError),
    /// Original service/capability failure.
    Runtime(RuntimeError),
    /// Original one-attempt output transfer refusal.
    Output(OutputAdmissionError),
    /// Input event channel exited; the joined report supplies its cause.
    InputStopped,
    /// Output receipt channel exited; the joined report supplies its cause.
    OutputStopped,
}
/// Joined original input/output and provider disconnect custody.
pub struct AttachmentFence {
    /// Exact revoked attachment.
    pub attachment: AttachmentId,
    /// Provider work was cancelled/returned before this fence was issued.
    pub service: DisconnectFence,
    /// Fully joined input owner and original partial/undelivered events.
    pub input: InputFence,
    /// Fully joined output owner and original partial/unsent packets.
    pub output: OutputFence,
    /// Original in-flight request and any original typed result/unsent reply.
    pub request: Option<RequestCustody>,
    /// Original orchestration failure; None means explicit caller fencing.
    pub failure: Option<SupervisorFailure>,
}
/// One caller-owned event from a bounded host service turn.
// Keep original custody inline: returning a fence cannot first require another
// error/report allocation in a resource-admission failure path.
#[allow(clippy::large_enum_variant)]
pub enum SupervisorEvent {
    /// One original final result finished local socket write.
    Delivered(Delivery),
    /// One attachment's workers exited and all original custody was returned.
    Fenced(AttachmentFence),
}
/// Actual composition counts, separate from provider/native/allocator costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SupervisorWork {
    /// Bounded turns; each polls one rotating attachment and invokes at most one job.
    pub turns: u64,
    /// Actual attach successes.
    pub attached: u64,
    /// Current native attachment owners, including pending fences.
    pub attachments: usize,
    /// Header authority checks before body allocation.
    pub headers: u64,
    /// Complete original inputs decoded before provider admission.
    pub received: u64,
    /// Readiness observations while reserved output capacity is occupied.
    pub output_waits: u64,
    /// Decoding's canonical header-removal memmove bytes.
    pub input_copied_bytes: u64,
    /// Canonical bytes copied by original reply encoding.
    pub reply_copied_bytes: u64,
    /// Final local socket-write receipts transferred to caller custody.
    pub delivered: u64,
    /// Explicit joined attachment fences transferred to caller custody.
    pub fenced: u64,
    /// Fixed attachment registry allocation capacity, excluding allocator overhead.
    pub registry_capacity_bytes: usize,
}
