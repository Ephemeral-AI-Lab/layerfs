//! Blocking native input runs independently of the host provider/dispatch owner.
use crate::{client::RequestHeader, runtime::check_header};
use layerfs_bridge::{
    codec::{Message, Reassembly, ReassemblyWork, ReceiveBudget},
    contract::{Envelope, FrameError},
    native::{ChannelError, ChannelWork, CloseHandle, Connection, FramingError, RecordReceiver},
};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
};
/// One bounded event; admission is delivered before body allocation/copying.
pub enum InputEvent {
    /// Parsed first header awaiting host binding/capability validation.
    Admission {
        /// Exact original native logical identity.
        envelope: Envelope,
        /// Checked operation/size/class, still untrusted application authority.
        header: RequestHeader,
    },
    /// Complete original credited request, not an attempted adapter operation.
    Ready(Message),
}
/// Original worker termination cause; no provider outcome is inferred from it.
pub enum InputFailure {
    /// Native record/framing failure including original quarantine-close failure.
    Native(FramingError),
    /// Exact logical reassembly or request header refusal.
    Frame(FrameError),
    /// Host refused input before receive allocation; no adapter was invoked.
    Refused(Envelope),
    /// Host event/admission owner disconnected while this input was waiting.
    Detached,
    /// Original OS refusal to create an input worker, before socket receive.
    Thread(std::io::Error),
}
/// Joined socket-input ownership, original partial bodies and observations.
pub struct InputReport {
    /// Exact stopping cause.
    pub failure: InputFailure,
    /// Original incomplete inputs, never submitted to an adapter.
    pub partial: Vec<Message>,
    /// Complete input not delivered because the host detached its event receiver.
    pub undelivered: Option<Message>,
    /// Original worker-side explicit close result, if protocol rejection closed it.
    pub close_failure: Option<FramingError>,
    /// Actual authenticated receive work including failed partial socket calls.
    pub native: ChannelWork,
    /// Aggregate receive ownership/copies; poison remains unavailable.
    pub reassembly: Result<ReassemblyWork, FrameError>,
    _lease: Arc<InputLease>,
}
/// Full local input fence: workers joined, event originals returned, close retained.
pub struct InputFence {
    /// Events queued before the fence; Ready messages remain unattempted input.
    pub events: Vec<InputEvent>,
    /// Exact original explicit close result, absent when no close was requested.
    pub close: Option<Result<(), ChannelError>>,
    /// Exact report or original panic payload from the joined worker.
    pub worker: Result<InputReport, Box<dyn Any + Send>>,
}
/// Authenticated input I/O owner, polled fairly by the application's service loop.
/// Its event queue and checked decision channel each hold at most one element.
pub struct NativeInput {
    events: Option<mpsc::Receiver<InputEvent>>,
    decision: Option<mpsc::SyncSender<bool>>,
    worker: Option<JoinHandle<InputReport>>,
    close: CloseHandle,
    close_result: Option<Result<(), ChannelError>>,
    fenced: bool,
    pending: Option<Envelope>,
    _lease: Arc<InputLease>,
}
struct InputLease {
    live: Arc<AtomicUsize>,
}
impl Drop for InputLease {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::AcqRel);
    }
}
/// Shared bounded thread/connection admission and aggregate receive allocation.
/// Retained joined reports keep their admission until released by the caller.
pub struct InputPool {
    limit: usize,
    live: Arc<AtomicUsize>,
    budget: ReceiveBudget,
}
impl InputPool {
    /// Selects simultaneous input owners once. It does not cap lifetime flows.
    pub fn new(limit: usize, budget: ReceiveBudget) -> Result<Self, FrameError> {
        if limit == 0 || budget.config().kind != layerfs_bridge::contract::MessageKind::Request {
            return Err(FrameError::Invalid("input owner window"));
        }
        Ok(Self {
            limit,
            live: Arc::new(AtomicUsize::new(0)),
            budget,
        })
    }
    /// Acquires actual shared connection credit before creating an I/O worker.
    /// Any admission/start refusal returns original authenticated socket ownership.
    pub fn start(
        &self,
        connection: Connection,
    ) -> Result<
        (
            NativeInput,
            layerfs_bridge::native::Sender,
            layerfs_bridge::native::VerifiedPeer,
        ),
        (InputFailure, Connection),
    > {
        if self
            .live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                if n < self.limit {
                    Some(n + 1)
                } else {
                    None
                }
            })
            .is_err()
        {
            return Err((
                InputFailure::Frame(FrameError::AdmissionUnavailable),
                connection,
            ));
        }
        let lease = Arc::new(InputLease {
            live: self.live.clone(),
        });
        NativeInput::start(connection, self.budget.clone(), lease)
    }
    /// Actual input owners/reports still holding their admission lease.
    pub fn outstanding(&self) -> usize {
        self.live.load(Ordering::Acquire)
    }
}
impl NativeInput {
    /// Moves only receive ownership to one worker. The host keeps the returned
    /// send direction for separate output ownership and the verified peer for
    /// binding. No provider, Save registry or whole-Commit lock moves to this thread.
    fn start(
        connection: Connection,
        budget: ReceiveBudget,
        lease: Arc<InputLease>,
    ) -> Result<
        (
            Self,
            layerfs_bridge::native::Sender,
            layerfs_bridge::native::VerifiedPeer,
        ),
        (InputFailure, Connection),
    > {
        let close = match connection.close_handle() {
            Ok(close) => close,
            Err(e) => return Err((InputFailure::Native(FramingError::Native(e)), connection)),
        };
        let collector = match Reassembly::with_budget(budget) {
            Ok(c) => c,
            Err(e) => return Err((InputFailure::Frame(e), connection)),
        };
        let (event_send, events) = mpsc::sync_channel(1);
        let (decision, decision_receive) = mpsc::sync_channel(1);
        // Keep the original receiver recoverable if the OS refuses thread creation.
        let parts = Arc::new(Mutex::new(Some((
            connection.receive,
            collector,
            event_send,
            decision_receive,
        ))));
        let worker_parts = parts.clone();
        let worker_lease = lease.clone();
        let worker = match thread::Builder::new()
            .name("layerfs-runtime-input".into())
            .spawn(move || {
                let (receive, collector, events, decisions) = worker_parts
                    .lock()
                    .expect("unattempted worker parts")
                    .take()
                    .expect("single input owner");
                run(
                    RecordReceiver::new(receive),
                    collector,
                    events,
                    decisions,
                    worker_lease,
                )
            }) {
            Ok(worker) => worker,
            Err(error) => {
                let (receive, _, _, _) = parts
                    .lock()
                    .expect("worker was not created")
                    .take()
                    .expect("unattempted receive ownership");
                return Err((
                    InputFailure::Thread(error),
                    Connection {
                        receive,
                        send: connection.send,
                        peer: connection.peer,
                        handshake_work: connection.handshake_work,
                    },
                ));
            }
        };
        Ok((
            Self {
                events: Some(events),
                decision: Some(decision),
                worker: Some(worker),
                close,
                close_result: None,
                fenced: false,
                pending: None,
                _lease: lease,
            },
            connection.send,
            connection.peer,
        ))
    }
    /// Nonblocking event observation. Empty is not a fence or publication outcome.
    pub fn try_event(&mut self) -> Result<InputEvent, mpsc::TryRecvError> {
        let event = match &self.events {
            Some(events) => events.try_recv()?,
            None => return Err(mpsc::TryRecvError::Disconnected),
        };
        if let InputEvent::Admission { envelope, .. } = &event {
            self.pending = Some(*envelope);
        }
        Ok(event)
    }
    /// Answers the one current admission rendezvous after host validation. A
    /// dropped receiver is an input fence, never permission to resend the header.
    pub fn decide(&mut self, envelope: Envelope, accepted: bool) -> Result<(), FrameError> {
        if self.pending != Some(envelope) || self.fenced {
            return Err(FrameError::Invalid("input admission ownership/state"));
        }
        self.pending = None;
        self.decision
            .as_ref()
            .ok_or(FrameError::Invalid("detached input"))?
            .try_send(accepted)
            .map_err(|_| FrameError::Invalid("input decision not delivered"))
    }
    /// Begins the explicit local input fence once. Closing the socket wakes reads;
    /// detaching event/decision owners wakes blocked delivery/admission. Joining is
    /// checked separately and cannot claim completion merely from shutdown success.
    pub fn fence(&mut self) {
        if !self.fenced {
            self.fenced = true;
            self.close_result = Some(self.close.close());
            self.decision.take();
            self.pending = None;
        }
    }
    /// Checks worker exit without a blocking join. After fence, drain and detach
    /// the bounded event receiver to wake a sender before checking again. Original
    /// queued events move to the caller; an empty queue does not imply dispatch.
    pub fn detach_events(&mut self) -> Vec<InputEvent> {
        match self.events.take() {
            Some(events) => events.try_iter().collect(),
            None => Vec::new(),
        }
    }
    /// Joins only after the worker has already finished. Exact close/error/partial
    /// knowledge moves to the caller once. There is no hidden wait/deadline/replay.
    pub fn try_join(&mut self) -> Option<InputFence> {
        if !self.worker.as_ref()?.is_finished() {
            return None;
        }
        let events = self.detach_events();
        Some(InputFence {
            events,
            close: self.close_result.take(),
            worker: self.worker.take().expect("checked worker").join(),
        })
    }
}
impl Drop for NativeInput {
    fn drop(&mut self) {
        // A dropped transport owner cannot leave blocked socket/event workers.
        // This revokes input only, never terminates Bash/unmounts a Workspace or
        // aborts its Saves. Applications use explicit fence/join to retain reports.
        self.fence();
        self.events.take();
        self.decision.take();
    }
}
fn run(
    mut receive: RecordReceiver,
    mut collector: Reassembly,
    events: mpsc::SyncSender<InputEvent>,
    decision: mpsc::Receiver<bool>,
    lease: Arc<InputLease>,
) -> InputReport {
    let mut undelivered = None;
    let failure = loop {
        let fragment = match receive.receive() {
            Ok(f) => f,
            Err(e) => break InputFailure::Native(e),
        };
        if fragment.offset == 0 {
            let header = match check_header(fragment.envelope, fragment.bytes) {
                Ok(header) => header,
                Err(e) => break InputFailure::Frame(e),
            };
            if events
                .send(InputEvent::Admission {
                    envelope: fragment.envelope,
                    header,
                })
                .is_err()
            {
                break InputFailure::Detached;
            }
            match decision.recv() {
                Ok(true) => (),
                Ok(false) => break InputFailure::Refused(fragment.envelope),
                Err(_) => break InputFailure::Detached,
            }
        }
        match collector.push(fragment) {
            Ok(Some(message)) => {
                if let Err(error) = events.send(InputEvent::Ready(message)) {
                    if let InputEvent::Ready(message) = error.0 {
                        undelivered = Some(message);
                    }
                    break InputFailure::Detached;
                }
            }
            Ok(None) => (),
            Err(e) => break InputFailure::Frame(e),
        }
    };
    let close_failure = match &failure {
        InputFailure::Native(_) => None,
        _ => receive.close().err(),
    };
    let partial = collector.drain_partial();
    InputReport {
        failure,
        partial,
        undelivered,
        close_failure,
        native: receive.work(),
        reassembly: collector.work(),
        _lease: lease,
    }
}
