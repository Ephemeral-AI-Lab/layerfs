//! One-attempt native client directions and original caller request ownership.
use super::{ClientRequest, ReplyView, RequestBody};
use layerfs_bridge::{
    codec::{Message, Reassembly, ReceiveBudget},
    contract::{FrameError, MessageClass, MessageKind, MAX_FRAGMENT_BYTES},
    native::{
        ChannelWork, FramingError, FramingWork, Receiver, RecordReceiver, RecordSender,
        SendProgress, Sender,
    },
};
use std::sync::Arc;
/// Exact original send failure; caller retains PendingRequest and its input.
#[derive(Debug)]
pub enum ClientSendError {
    /// Checked local protocol/state failure before native write.
    Frame(FrameError),
    /// Original attempted native write, with complete-fragment progress only.
    Native(FramingError),
}
/// Exact receive failure; malformed complete reply retains original body/credit.
pub enum ClientReceiveError {
    /// Original socket/framing failure.
    Native(FramingError),
    /// Original reassembly refusal, with prior partial bodies retained by receiver.
    Frame(FrameError),
    /// Original complete malformed result and its explicit socket close result.
    Rejected {
        /// Exact codec failure.
        error: FrameError,
        /// Original credited body.
        message: Message,
        /// Original close failure if any.
        close: Option<FramingError>,
    },
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Header,
    Body,
    Failed,
}
/// Original borrowed input and unclonable transport attempt. No automatic resend,
/// refresh, provider retry or inferred rollback follows a missing reply.
pub struct PendingRequest<'a> {
    request: ClientRequest<'a>,
    progress: SendProgress,
    state: State,
    owner: Arc<()>,
}
impl PendingRequest<'_> {
    /// Exact original request correlation, independent of reply completion order.
    pub fn correlation(&self) -> u64 {
        self.progress.envelope().correlation
    }
    /// True only after this original complete body was locally sent once.
    /// It is not remote invocation/publication knowledge.
    pub fn body_sent(&self) -> bool {
        self.state == State::Body
    }
    /// Original caller-owned request; refusal/uncertainty never consumes its body.
    pub const fn request(&self) -> ClientRequest<'_> {
        self.request
    }
}
/// Client send-direction owner; can move independently to a bounded I/O worker.
pub struct ClientSender {
    sender: RecordSender,
    next: u64,
    ids: [u8; MAX_FRAGMENT_BYTES],
    id_copied_bytes: u64,
    owner: Arc<()>,
}
impl ClientSender {
    /// Wraps the authenticated direction or returns its original ownership.
    // Allocation refusal returns the original owner inline without allocating
    // another error container merely to satisfy an enum-size lint.
    #[allow(clippy::result_large_err)]
    pub fn new(sender: Sender) -> Result<Self, (FrameError, Sender)> {
        Ok(Self {
            sender: RecordSender::new(sender)?,
            next: 1,
            ids: [0; MAX_FRAGMENT_BYTES],
            id_copied_bytes: 0,
            owner: Arc::new(()),
        })
    }
    /// Sends only the fixed header. The peer must authorize it before this
    /// caller permits body transmission; no complete-object upload is prebuffered.
    pub fn begin<'a>(
        &mut self,
        request: ClientRequest<'a>,
    ) -> Result<PendingRequest<'a>, ClientSendError> {
        request.validate().map_err(ClientSendError::Frame)?;
        let bytes = request.header.encode().map_err(ClientSendError::Frame)?;
        let total = request
            .header
            .total_bytes()
            .map_err(ClientSendError::Frame)?;
        let correlation = self.next;
        self.next = correlation
            .checked_add(1)
            .ok_or(ClientSendError::Frame(FrameError::IdentityExhausted))?;
        let progress = self
            .sender
            .begin(
                MessageKind::Request,
                request.header.operation.class(),
                correlation,
                total,
                &bytes,
            )
            .map_err(ClientSendError::Native)?;
        Ok(PendingRequest {
            request,
            progress,
            state: State::Header,
            owner: self.owner.clone(),
        })
    }
    /// Consumes an exact original grant and sends bounded body fragments once.
    /// Refused/malformed/foreign grants never transmit a body. Failed input cannot
    /// be replayed through this PendingRequest even if no final reply arrives.
    pub fn send_body(
        &mut self,
        pending: &mut PendingRequest<'_>,
        grant: &Message,
    ) -> Result<(), ClientSendError> {
        if !Arc::ptr_eq(&self.owner, &pending.owner) || pending.state != State::Header {
            return Err(ClientSendError::Frame(FrameError::Invalid(
                "request send state/owner",
            )));
        }
        pending.state = State::Failed;
        let envelope = grant.envelope();
        if !grant.complete()
            || envelope.kind != MessageKind::Reply
            || envelope.class != MessageClass::Control
            || envelope.correlation != pending.correlation()
            || !matches!(ReplyView::decode(grant.bytes()), Ok(ReplyView::Granted))
        {
            return Err(ClientSendError::Frame(FrameError::Invalid(
                "original header grant",
            )));
        }
        match pending.request.body {
            RequestBody::Empty => (),
            RequestBody::Canonical(bytes) => {
                for bytes in bytes.chunks(MAX_FRAGMENT_BYTES) {
                    self.sender
                        .continue_message(&mut pending.progress, bytes)
                        .map_err(ClientSendError::Native)?;
                }
            }
            RequestBody::Ids(ids) => {
                for ids in ids.chunks(MAX_FRAGMENT_BYTES / 32) {
                    for (at, id) in ids.iter().enumerate() {
                        self.ids[at * 32..(at + 1) * 32].copy_from_slice(id.as_bytes());
                    }
                    let n = ids.len() * 32;
                    self.id_copied_bytes = self.id_copied_bytes.saturating_add(n as u64);
                    self.sender
                        .continue_message(&mut pending.progress, &self.ids[..n])
                        .map_err(ClientSendError::Native)?;
                }
            }
        }
        pending.state = State::Body;
        Ok(())
    }
    /// Exact framing/native work; ID encoding copies are reported separately.
    pub fn work(&self) -> (FramingWork, ChannelWork, u64) {
        (
            self.sender.work(),
            self.sender.native_work(),
            self.id_copied_bytes,
        )
    }
    /// Explicit socket fence; it never aborts a Save or unmounts a Workspace.
    pub fn close(&self) -> Result<(), FramingError> {
        self.sender.close()
    }
}
/// Client receive-direction owner and bounded multiplexed partial/result credit.
pub struct ClientReceiver {
    receiver: RecordReceiver,
    bodies: Reassembly,
}
impl ClientReceiver {
    /// Selects the shared live reply budget before using the authenticated owner.
    #[allow(clippy::result_large_err)]
    pub fn new(receiver: Receiver, budget: ReceiveBudget) -> Result<Self, (FrameError, Receiver)> {
        match Reassembly::with_budget(budget) {
            Ok(bodies) => Ok(Self {
                receiver: RecordReceiver::new(receiver),
                bodies,
            }),
            Err(e) => Err((e, receiver)),
        }
    }
    /// Receives until one complete credited reply arrives. Interleaved partial
    /// results remain with this owner; no message/request is automatically repeated.
    /// Product calls have no implicit runtime deadline.
    pub fn receive(&mut self) -> Result<Message, ClientReceiveError> {
        loop {
            let fragment = self
                .receiver
                .receive()
                .map_err(ClientReceiveError::Native)?;
            if let Some(message) = self
                .bodies
                .push(fragment)
                .map_err(ClientReceiveError::Frame)?
            {
                if let Err(error) = ReplyView::decode(message.bytes()) {
                    return Err(ClientReceiveError::Rejected {
                        error,
                        message,
                        close: self.receiver.close().err(),
                    });
                }
                return Ok(message);
            }
        }
    }
    /// Actual native and shared body work, including caller-held results.
    pub fn work(
        &self,
    ) -> (
        ChannelWork,
        Result<layerfs_bridge::codec::ReassemblyWork, FrameError>,
    ) {
        (self.receiver.work(), self.bodies.work())
    }
    /// Explicit close once; caller then fences/drains original partial replies.
    pub fn close(&self) -> Result<(), FramingError> {
        self.receiver.close()
    }
    /// Returns retained incomplete replies after the socket I/O owner is fenced.
    pub fn drain_partial(&mut self) -> Vec<Message> {
        self.bodies.drain_partial()
    }
}
