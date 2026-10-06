//! One bounded native exchange per caller, with original failure/partial custody.
use super::{
    ClientReceiveError, ClientReceiver, ClientRequest, ClientSendError, ClientSender,
    FailureOrigin, Operation, ReplyView,
};
use layerfs_bridge::{
    codec::Message,
    contract::{FrameError, MessageClass, MessageKind},
    native::{CloseHandle, FramingError, VerifiedPeer},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
/// Exact exchange boundary at which the original request stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallPhase {
    /// Input/owner admission before any header send.
    Admission,
    /// Original header transmission.
    Header,
    /// Awaiting the original header grant or pre-dispatch refusal.
    Grant,
    /// Original bounded body transmission.
    Body,
    /// Awaiting the original adapter/receipt response.
    Reply,
}
/// Exact underlying exchange failure, never a publication-success inference.
pub enum CallError {
    /// Original checked protocol/owner failure.
    Frame(FrameError),
    /// Original native send failure, including complete-fragment progress.
    Send(ClientSendError),
    /// Original receive failure, including any malformed complete body and close.
    Receive(ClientReceiveError),
}
/// Original failed request facts and any received body/explicit close failure.
/// Callers retain this record rather than reconstructing a cause from Display.
pub struct CallFailure {
    /// Exact original operation/capability/object/length facts.
    pub request: super::RequestHeader,
    /// Original request correlation, absent before it was minted/sent.
    pub correlation: Option<u64>,
    /// Stopping boundary; a missing reply never proves unattempted publication.
    pub phase: CallPhase,
    /// Exact original failure.
    pub error: CallError,
    /// Original credited reply/grant when its shape did not match the request.
    pub received: Option<Message>,
    /// Original explicit channel-fence failure, if a protocol failure closed it.
    pub close: Option<FramingError>,
}
struct Driver {
    send: ClientSender,
    receive: ClientReceiver,
    stopped: bool,
}
/// Native runtime exchange owner suitable for Send/Sync immutable demand ports.
///
/// A mutex covers one existing bounded adapter unit, never a whole Save/Commit.
/// Raw independent directions remain available for application multiplexing.
/// Supply directions from one authenticated Connection and its VerifiedPeer;
/// provisioning, attachment and original binding belong to the application.
pub struct Calls {
    driver: Mutex<Driver>,
    peer: VerifiedPeer,
    fence: CloseHandle,
    closed: AtomicBool,
}
impl Calls {
    /// Takes the original directions once, without reconnect or provider bootstrap.
    pub fn new(
        send: ClientSender,
        receive: ClientReceiver,
        peer: VerifiedPeer,
        fence: CloseHandle,
    ) -> Self {
        Self {
            driver: Mutex::new(Driver {
                send,
                receive,
                stopped: false,
            }),
            peer,
            fence,
            closed: AtomicBool::new(false),
        }
    }
    /// Authenticated remote static identity, never a raw-input peer assertion.
    pub const fn peer(&self) -> VerifiedPeer {
        self.peer
    }
    /// Attempts one original bounded request. Any typed remote refusal/error is
    /// returned in its credited Message. Original transport/protocol failure is
    /// terminal for this owner; there is no retry, refresh, reconnect or resend.
    pub fn call(&self, request: ClientRequest<'_>) -> Result<Message, CallFailure> {
        let header = request.header;
        let failure = |phase, correlation, error, received, close| CallFailure {
            request: header,
            correlation,
            phase,
            error,
            received,
            close,
        };
        request.validate().map_err(|error| {
            failure(
                CallPhase::Admission,
                None,
                CallError::Frame(error),
                None,
                None,
            )
        })?;
        let mut driver = self.driver.lock().map_err(|_| {
            failure(
                CallPhase::Admission,
                None,
                CallError::Frame(FrameError::Poisoned),
                None,
                None,
            )
        })?;
        if driver.stopped || self.closed.load(Ordering::Acquire) {
            return Err(failure(
                CallPhase::Admission,
                None,
                CallError::Frame(FrameError::Invalid("runtime exchange stopped")),
                None,
                None,
            ));
        }
        let mut pending = match driver.send.begin(request) {
            Ok(pending) => pending,
            Err(error) => {
                let correlation = match &error {
                    ClientSendError::Native(FramingError::Send { envelope, .. }) => {
                        Some(envelope.correlation)
                    }
                    _ => None,
                };
                driver.stopped = true;
                return Err(failure(
                    CallPhase::Header,
                    correlation,
                    CallError::Send(error),
                    None,
                    None,
                ));
            }
        };
        let correlation = Some(pending.correlation());
        let grant = match driver.receive.receive() {
            Ok(grant) => grant,
            Err(error) => {
                driver.stopped = true;
                return Err(failure(
                    CallPhase::Grant,
                    correlation,
                    CallError::Receive(error),
                    None,
                    None,
                ));
            }
        };
        let envelope = grant.envelope();
        if envelope.kind != MessageKind::Reply
            || envelope.class != MessageClass::Control
            || Some(envelope.correlation) != correlation
        {
            driver.stopped = true;
            let close = driver.receive.close().err();
            return Err(failure(
                CallPhase::Grant,
                correlation,
                CallError::Frame(FrameError::Invalid("runtime grant identity")),
                Some(grant),
                close,
            ));
        }
        match ReplyView::decode(grant.bytes()) {
            Ok(ReplyView::Failure {
                origin: FailureOrigin::Admission,
                ..
            }) => {
                // NativeInput fences a refused first header; it did not receive a
                // body or invoke the adapter. Preserve that distinct original reply.
                driver.stopped = true;
                return Ok(grant);
            }
            Ok(ReplyView::Granted) => (),
            _ => {
                driver.stopped = true;
                let close = driver.receive.close().err();
                return Err(failure(
                    CallPhase::Grant,
                    correlation,
                    CallError::Frame(FrameError::Invalid("runtime grant result")),
                    Some(grant),
                    close,
                ));
            }
        }
        if let Err(error) = driver.send.send_body(&mut pending, &grant) {
            driver.stopped = true;
            return Err(failure(
                CallPhase::Body,
                correlation,
                CallError::Send(error),
                Some(grant),
                None,
            ));
        }
        drop(grant);
        let reply = match driver.receive.receive() {
            Ok(reply) => reply,
            Err(error) => {
                driver.stopped = true;
                return Err(failure(
                    CallPhase::Reply,
                    correlation,
                    CallError::Receive(error),
                    None,
                    None,
                ));
            }
        };
        let envelope = reply.envelope();
        let checked = envelope.kind == MessageKind::Reply
            && Some(envelope.correlation) == correlation
            && envelope.class == header.operation.class()
            && ReplyView::decode(reply.bytes()).is_ok_and(|view| matches_result(request, &view));
        if !checked {
            driver.stopped = true;
            let close = driver.receive.close().err();
            return Err(failure(
                CallPhase::Reply,
                correlation,
                CallError::Frame(FrameError::Invalid("runtime result identity/shape")),
                Some(reply),
                close,
            ));
        }
        Ok(reply)
    }
    /// Explicit native fence once; does not abort a Save or unmount a Workspace.
    pub fn close(&self) -> Result<(), FramingError> {
        self.closed.store(true, Ordering::Release);
        self.fence.close().map_err(FramingError::Native)
    }
    /// Returns original incomplete replies after native I/O is fenced. No
    /// response absence or publication outcome is inferred from an empty list.
    pub fn drain_partial(&self) -> Result<Vec<Message>, FrameError> {
        let mut driver = self.driver.try_lock().map_err(|error| match error {
            std::sync::TryLockError::Poisoned(_) => FrameError::Poisoned,
            std::sync::TryLockError::WouldBlock => FrameError::AdmissionUnavailable,
        })?;
        if !driver.stopped && !self.closed.load(Ordering::Acquire) {
            return Err(FrameError::Invalid("runtime exchange not fenced"));
        }
        Ok(driver.receive.drain_partial())
    }
}
fn matches_result(request: ClientRequest<'_>, reply: &ReplyView<'_>) -> bool {
    use super::RequestBody;
    match (request.header.operation, reply) {
        (_, ReplyView::Failure { .. } | ReplyView::Unattempted) => true,
        (Operation::Policy, ReplyView::Policy(_))
        | (Operation::Binding, ReplyView::Binding(_))
        | (Operation::Begin, ReplyView::Begun(_))
        | (Operation::Release, ReplyView::Released) => true,
        (Operation::ReserveInodes, ReplyView::Serials { count, .. }) => {
            *count == request.header.value
        }
        (Operation::Accept, ReplyView::Accepted(id)) => Some(*id) == request.header.object,
        (Operation::Objects, ReplyView::Objects(values)) => match request.body {
            RequestBody::Ids(ids) => {
                ids.len() == values.len()
                    && ids.iter().copied().eq(values.clone().map(|(id, _)| id))
            }
            _ => false,
        },
        (Operation::Lengths, ReplyView::Lengths(values)) => match request.body {
            RequestBody::Ids(ids) => {
                ids.len() == values.len()
                    && ids.iter().copied().eq(values.clone().map(|(id, _)| id))
            }
            _ => false,
        },
        (
            op @ (Operation::Finish | Operation::Abort | Operation::Completion),
            ReplyView::Completion { save, receipt },
        ) => {
            Some(*save) == request.header.save
                && receipt.as_ref().map_or(true, |receipt| match op {
                    Operation::Finish => receipt.phase == crate::CompletionPhase::Finish,
                    Operation::Abort => receipt.phase == crate::CompletionPhase::Abort,
                    _ => true,
                })
        }
        (
            op @ (Operation::Stage | Operation::Commit | Operation::Discard | Operation::History),
            ReplyView::History { save, receipt },
        ) => {
            Some(*save) == request.header.save
                && receipt.as_ref().map_or(true, |receipt| match op {
                    Operation::Stage => receipt.stage.is_some(),
                    Operation::Commit => receipt.commit.is_some(),
                    Operation::Discard => receipt.discard.is_some(),
                    _ => true,
                })
        }
        _ => false,
    }
}
