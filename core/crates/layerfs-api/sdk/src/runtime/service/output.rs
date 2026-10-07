//! Socket output is independent of provider dispatch and rotates bounded fragments.
use crate::runtime::wake::{PublishedSender, Signal};
use layerfs_bridge::{
    contract::{FrameError, MessageClass, MessageKind, MAX_FRAGMENT_BYTES},
    native::{
        ChannelError, ChannelWork, CloseHandle, FramingError, FramingWork, RecordSender,
        SendProgress, Sender,
    },
};
use std::{
    any::Any,
    collections::VecDeque,
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
};
/// Shared simultaneous output admission; these limits do not cap lifetime flows.
#[derive(Clone, Copy, Debug)]
pub struct OutputConfig {
    /// Live socket output workers, including retained exit reports.
    pub owners: usize,
    /// Queued/sending/caller-held original packets across all workers.
    pub messages: usize,
    /// Message slots reserved from ordinary Save replies for demand progress.
    pub demand_messages: usize,
    /// Message slots reserved from Save/demand replies for control progress.
    pub control_messages: usize,
    /// Aggregate original packet allocation/receipt capacity.
    pub bytes: usize,
    /// Byte capacity ordinary Save output cannot consume.
    pub demand_reserve: usize,
    /// Byte capacity Save/demand output cannot consume.
    pub control_reserve: usize,
}
/// Actual fixed send ownership/copy observations; no RSS/latency claim follows.
#[derive(Clone, Copy, Debug, Default)]
pub struct OutputWork {
    /// Original submit calls, including pre-credit/queue refusals.
    pub submissions: u64,
    /// Original output admission refusals with no socket write.
    pub refused: u64,
    /// Successfully credited packets.
    pub admitted: u64,
    /// Simultaneous worker/report admission still owned.
    pub owners: usize,
    /// Current packets still holding output credit, including caller receipts.
    pub messages: usize,
    /// Packet capacities, pre-encoding reservations and fixed ownership charges.
    pub credited_bytes: usize,
    /// Current packet Vec capacities, including retained unsent originals.
    pub packet_capacity_bytes: usize,
    /// Pre-encoding capacity credits whose packet has not yet been allocated.
    pub reserved_bytes: usize,
    /// Largest aggregate credit observation, not whole-system residency.
    pub peak_credited_bytes: usize,
}
/// Exact unattempted output startup refusal; native ownership is returned separately.
#[derive(Debug)]
pub enum OutputStartError {
    /// Original logical admission/allocation refusal.
    Frame(FrameError),
    /// Original socket descriptor refusal.
    Native(ChannelError),
    /// Original OS worker creation refusal.
    Thread(std::io::Error),
}
/// Exact packet admission/delivery-queue refusal before any socket write.
#[derive(Debug)]
pub enum OutputAdmissionError {
    /// Original selected live credit/format refusal.
    Frame(FrameError),
    /// Output worker already detached; the original packet was never transferred.
    Detached,
}
struct Ledger {
    config: OutputConfig,
    work: OutputWork,
    wake: Option<Arc<Signal>>,
}
struct OwnerLease(Arc<Mutex<Ledger>>);
impl Drop for OwnerLease {
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.0.lock() {
            ledger.work.owners -= 1;
        }
    }
}
struct Credit {
    ledger: Arc<Mutex<Ledger>>,
    bytes: usize,
    packet_capacity: usize,
    reserved: bool,
}
/// Reserved output capacity acquired before provider invocation or reply encoding.
/// A missing permit is a readiness wait, with no socket send or operation attempt.
pub struct OutputPermit {
    correlation: u64,
    class: MessageClass,
    capacity: usize,
    credit: Credit,
    owner: Arc<OwnerLease>,
}
impl Drop for Credit {
    fn drop(&mut self) {
        let wake = match self.ledger.lock() {
            Ok(mut ledger) => {
                ledger.work.messages -= 1;
                ledger.work.credited_bytes -= self.bytes;
                ledger.work.packet_capacity_bytes -= self.packet_capacity;
                if self.reserved {
                    ledger.work.reserved_bytes -= self.bytes;
                }
                ledger.wake.clone()
            }
            // Only retrieve the notifier. Poison remains set and accounting is
            // unavailable; the next original reserve/work observation reports it.
            Err(error) => error.get_ref().wake.clone(),
        };
        // Capacity becomes visible before notification; the ledger is unlocked.
        if let Some(wake) = wake {
            let _ = wake.notify();
        }
    }
}
/// Original owned bounded result/grant, with no borrowed provider state.
pub struct OutputPacket {
    /// Exact request correlation; native message IDs are assigned in send order.
    pub correlation: u64,
    /// Authenticated reserve class, supplied by the owning host operation.
    pub class: MessageClass,
    /// Original encoded bytes. The host retains its typed completion separately.
    pub bytes: Vec<u8>,
}
struct Job {
    packet: OutputPacket,
    progress: Option<SendProgress>,
    _credit: Credit,
}
/// Original completed or stopped send. Local socket completion does not prove
/// remote runtime consumption or change Save/history publication knowledge.
pub struct OutputReceipt {
    /// Original owned result/grant bytes.
    pub packet: OutputPacket,
    /// Complete plaintext bytes whose original records finished socket write.
    pub completed_bytes: u64,
    /// True only when the whole original packet finished local socket write.
    pub complete: bool,
    _credit: Credit,
}
impl Job {
    fn receipt(self, complete: bool) -> OutputReceipt {
        OutputReceipt {
            completed_bytes: self
                .progress
                .as_ref()
                .map_or(0, SendProgress::completed_bytes),
            packet: self.packet,
            complete,
            _credit: self._credit,
        }
    }
}
/// Exact joined output failure and all retained undelivered packet ownership.
pub struct OutputReport {
    /// Original send/explicit-channel failure; None means orderly input detach.
    pub failure: Option<FramingError>,
    /// Failed/partial/queued packets preserved without replay or guessed success.
    pub retained: Vec<OutputReceipt>,
    /// Exact frame copies and current scratch.
    pub framing: FramingWork,
    /// Exact encrypted I/O including partial failure calls.
    pub native: ChannelWork,
    _owner: Arc<OwnerLease>,
}
/// Complete local output fence after the I/O worker actually exited.
pub struct OutputFence {
    /// Original receipts queued before output owner detach.
    pub receipts: Vec<OutputReceipt>,
    /// Explicit close result, absent if no close was requested.
    pub close: Option<Result<(), ChannelError>>,
    /// Exact worker report or original panic payload.
    pub worker: Result<OutputReport, Box<dyn Any + Send>>,
}
/// Shared admission used by every host attachment's independent output worker.
pub struct OutputPool {
    ledger: Arc<Mutex<Ledger>>,
}
impl OutputPool {
    /// Selects and validates positive reserves before any worker/packet allocation.
    pub fn new(config: OutputConfig) -> Result<Self, FrameError> {
        let count = config
            .demand_messages
            .checked_add(config.control_messages)
            .ok_or(FrameError::Invalid("output count reserves"))?;
        let bytes = config
            .demand_reserve
            .checked_add(config.control_reserve)
            .ok_or(FrameError::Invalid("output byte reserves"))?;
        if config.owners == 0
            || config.demand_reserve == 0
            || config.demand_messages == 0
            || config.control_messages == 0
            || config.messages <= count
            || config.bytes <= bytes
            || config.control_reserve < std::mem::size_of::<Job>()
        {
            return Err(FrameError::Invalid("output admission windows"));
        }
        Ok(Self {
            ledger: Arc::new(Mutex::new(Ledger {
                config,
                work: OutputWork::default(),
                wake: None,
            })),
        })
    }
    pub(crate) fn set_wake(&mut self, wake: Arc<Signal>) -> Result<(), FrameError> {
        let mut ledger = self.ledger.lock().map_err(|_| FrameError::Poisoned)?;
        if ledger.work.owners != 0 {
            return Err(FrameError::Invalid("output wake after worker startup"));
        }
        ledger.wake = Some(wake);
        Ok(())
    }
    /// Starts one credited worker or returns the original send direction unchanged.
    pub fn start(&self, sender: Sender) -> Result<NativeOutput, (OutputStartError, Sender)> {
        let close = match sender.close_handle() {
            Ok(c) => c,
            Err(e) => return Err((OutputStartError::Native(e), sender)),
        };
        let (config, wake) = {
            let mut ledger = match self.ledger.lock() {
                Ok(l) => l,
                Err(_) => return Err((OutputStartError::Frame(FrameError::Poisoned), sender)),
            };
            if ledger.work.owners >= ledger.config.owners {
                return Err((
                    OutputStartError::Frame(FrameError::AdmissionUnavailable),
                    sender,
                ));
            }
            ledger.work.owners += 1;
            (ledger.config, ledger.wake.clone())
        };
        let owner = Arc::new(OwnerLease(self.ledger.clone()));
        let writer = match RecordSender::new(sender) {
            Ok(w) => w,
            Err((e, sender)) => return Err((OutputStartError::Frame(e), sender)),
        };
        let (jobs, incoming) = mpsc::sync_channel(config.messages);
        let (outgoing, receipts) = mpsc::sync_channel(1);
        let outgoing = PublishedSender::new(outgoing, wake);
        let parts = Arc::new(Mutex::new(Some((writer, incoming, outgoing))));
        let thread_parts = parts.clone();
        let thread_owner = owner.clone();
        let worker = match thread::Builder::new()
            .name("layerfs-runtime-output".into())
            .spawn(move || {
                let (writer, incoming, outgoing) = thread_parts
                    .lock()
                    .expect("unattempted output parts")
                    .take()
                    .expect("single writer");
                run(writer, incoming, outgoing, thread_owner)
            }) {
            Ok(worker) => worker,
            Err(error) => {
                let (writer, _, _) = parts
                    .lock()
                    .expect("output worker not created")
                    .take()
                    .expect("original writer");
                return Err((OutputStartError::Thread(error), writer.into_native()));
            }
        };
        Ok(NativeOutput {
            jobs: Some(jobs),
            receipts: Some(receipts),
            worker: Some(worker),
            close,
            close_result: None,
            owner,
            ledger: self.ledger.clone(),
            fenced: false,
        })
    }
    /// Actual shared credits; a poisoned observation stays explicitly unavailable.
    pub fn work(&self) -> Result<OutputWork, FrameError> {
        Ok(self.ledger.lock().map_err(|_| FrameError::Poisoned)?.work)
    }
}
/// One socket output owner, polled by the host without blocking provider dispatch.
pub struct NativeOutput {
    jobs: Option<mpsc::SyncSender<Job>>,
    receipts: Option<mpsc::Receiver<OutputReceipt>>,
    worker: Option<JoinHandle<OutputReport>>,
    close: CloseHandle,
    close_result: Option<Result<(), ChannelError>>,
    owner: Arc<OwnerLease>,
    ledger: Arc<Mutex<Ledger>>,
    fenced: bool,
}
impl NativeOutput {
    /// Reserves one original reply's maximum allocation before encoding. `None`
    /// means the bounded delivery window is occupied; no packet was submitted.
    /// The permit is bound to this output pool and cannot replay a prior send.
    pub fn reserve(
        &self,
        correlation: u64,
        class: MessageClass,
        capacity: usize,
    ) -> Result<Option<OutputPermit>, FrameError> {
        if correlation == 0 || self.fenced {
            return Err(FrameError::Invalid("output reservation identity/state"));
        }
        let mut ledger = self.ledger.lock().map_err(|_| FrameError::Poisoned)?;
        let config = ledger.config;
        let (count, bytes) = match class {
            MessageClass::Save => (
                config.messages - config.demand_messages - config.control_messages,
                config.bytes - config.demand_reserve - config.control_reserve,
            ),
            MessageClass::Demand => (
                config.messages - config.control_messages,
                config.bytes - config.control_reserve,
            ),
            MessageClass::Control => (config.messages, config.bytes),
        };
        let charge = capacity
            .checked_add(std::mem::size_of::<Job>() + std::mem::size_of::<OutputReceipt>())
            .ok_or(FrameError::Invalid("output reservation charge"))?;
        if charge > bytes {
            return Err(FrameError::Invalid(
                "output reservation exceeds class window",
            ));
        }
        if ledger.work.credited_bytes > bytes - charge || ledger.work.messages >= count {
            return Ok(None);
        }
        ledger.work.messages += 1;
        ledger.work.credited_bytes += charge;
        ledger.work.reserved_bytes += charge;
        ledger.work.peak_credited_bytes = ledger
            .work
            .peak_credited_bytes
            .max(ledger.work.credited_bytes);
        Ok(Some(OutputPermit {
            correlation,
            class,
            capacity,
            credit: Credit {
                ledger: self.ledger.clone(),
                bytes: charge,
                packet_capacity: 0,
                reserved: true,
            },
            owner: self.owner.clone(),
        }))
    }

    /// Transfers one credited original packet once. Refusal returns packet and
    /// permit; an attempted transfer consumes its reservation state. A returned
    /// terminal permit refuses a second transfer before accounting or native I/O.
    #[allow(clippy::result_large_err)]
    pub fn submit_reserved(
        &self,
        permit: OutputPermit,
        packet: OutputPacket,
    ) -> Result<(), (OutputAdmissionError, OutputPacket, OutputPermit)> {
        if self.fenced
            || !permit.credit.reserved
            || !Arc::ptr_eq(&self.ledger, &permit.credit.ledger)
            || !Arc::ptr_eq(&self.owner, &permit.owner)
            || packet.correlation != permit.correlation
            || packet.class != permit.class
            || packet.bytes.capacity() > permit.capacity
        {
            return Err((
                OutputAdmissionError::Frame(FrameError::Invalid("output permit/packet")),
                packet,
                permit,
            ));
        }
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger.work.submissions = ledger.work.submissions.saturating_add(1);
        }
        let OutputPermit {
            mut credit,
            correlation,
            class,
            capacity,
            owner,
        } = permit;
        let actual_charge = packet.bytes.capacity()
            + std::mem::size_of::<Job>()
            + std::mem::size_of::<OutputReceipt>();
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger.work.reserved_bytes -= credit.bytes;
            ledger.work.credited_bytes -= credit.bytes - actual_charge;
            ledger.work.packet_capacity_bytes += packet.bytes.capacity();
        }
        credit.bytes = actual_charge;
        credit.packet_capacity = packet.bytes.capacity();
        credit.reserved = false;
        let job = Job {
            packet,
            progress: None,
            _credit: credit,
        };
        match self.jobs.as_ref().expect("unfenced jobs").try_send(job) {
            Ok(()) => {
                if let Ok(mut ledger) = self.ledger.lock() {
                    ledger.work.admitted = ledger.work.admitted.saturating_add(1);
                }
                Ok(())
            }
            Err(error) => {
                let (error, job) = match error {
                    mpsc::TrySendError::Full(job) => (
                        OutputAdmissionError::Frame(FrameError::AdmissionUnavailable),
                        job,
                    ),
                    mpsc::TrySendError::Disconnected(job) => (OutputAdmissionError::Detached, job),
                };
                if let Ok(mut ledger) = self.ledger.lock() {
                    ledger.work.refused = ledger.work.refused.saturating_add(1);
                }
                Err((
                    error,
                    job.packet,
                    OutputPermit {
                        correlation,
                        class,
                        capacity,
                        credit: job._credit,
                        owner,
                    },
                ))
            }
        }
    }
    /// Credits one original owned packet before nonblocking queue transfer. Any
    /// refusal returns its exact bytes without a native write or provider replay.
    pub fn try_submit(
        &self,
        packet: OutputPacket,
    ) -> Result<(), (OutputAdmissionError, OutputPacket)> {
        let credit = (|| {
            let mut ledger = self.ledger.lock().map_err(|_| FrameError::Poisoned)?;
            ledger.work.submissions = ledger.work.submissions.saturating_add(1);
            let config = ledger.config;
            let (count, bytes) = match packet.class {
                MessageClass::Save => (
                    config.messages - config.demand_messages - config.control_messages,
                    config.bytes - config.demand_reserve - config.control_reserve,
                ),
                MessageClass::Demand => (
                    config.messages - config.control_messages,
                    config.bytes - config.control_reserve,
                ),
                MessageClass::Control => (config.messages, config.bytes),
            };
            let charge = packet
                .bytes
                .capacity()
                .checked_add(std::mem::size_of::<Job>() + std::mem::size_of::<OutputReceipt>())
                .ok_or(FrameError::Invalid("output capacity charge"))?;
            if self.fenced
                || packet.correlation == 0
                || charge > bytes
                || ledger.work.credited_bytes > bytes - charge
                || ledger.work.messages >= count
            {
                ledger.work.refused = ledger.work.refused.saturating_add(1);
                return Err(FrameError::AdmissionUnavailable);
            }
            ledger.work.messages += 1;
            ledger.work.credited_bytes += charge;
            ledger.work.packet_capacity_bytes += packet.bytes.capacity();
            ledger.work.admitted = ledger.work.admitted.saturating_add(1);
            ledger.work.peak_credited_bytes = ledger
                .work
                .peak_credited_bytes
                .max(ledger.work.credited_bytes);
            Ok(Credit {
                ledger: self.ledger.clone(),
                bytes: charge,
                packet_capacity: packet.bytes.capacity(),
                reserved: false,
            })
        })();
        let credit = match credit {
            Ok(c) => c,
            Err(e) => return Err((OutputAdmissionError::Frame(e), packet)),
        };
        let job = Job {
            packet,
            progress: None,
            _credit: credit,
        };
        match self.jobs.as_ref().expect("unfenced jobs").try_send(job) {
            Ok(()) => Ok(()),
            Err(error) => {
                let (error, job) = match error {
                    mpsc::TrySendError::Full(job) => (
                        OutputAdmissionError::Frame(FrameError::AdmissionUnavailable),
                        job,
                    ),
                    mpsc::TrySendError::Disconnected(job) => (OutputAdmissionError::Detached, job),
                };
                if let Ok(mut ledger) = self.ledger.lock() {
                    ledger.work.refused = ledger.work.refused.saturating_add(1);
                }
                Err((error, job.packet))
            }
        }
    }
    /// Nonblocking original send receipt; holding it retains output byte credit.
    pub fn try_receipt(&self) -> Result<OutputReceipt, mpsc::TryRecvError> {
        self.receipts
            .as_ref()
            .ok_or(mpsc::TryRecvError::Disconnected)?
            .try_recv()
    }
    /// Starts explicit output detach/shutdown once; it never changes runtime custody.
    pub fn fence(&mut self) {
        if !self.fenced {
            self.fenced = true;
            self.close_result = Some(self.close.close());
            self.jobs.take();
        }
    }
    /// Returns queued receipts and detaches delivery to wake a blocked output worker.
    pub fn detach_receipts(&mut self) -> Vec<OutputReceipt> {
        self.receipts
            .take()
            .map_or_else(Vec::new, |receipts| receipts.try_iter().collect())
    }
    /// Joins only after observed thread exit, preserving original stopped packets.
    pub fn try_join(&mut self) -> Option<OutputFence> {
        if !self.worker.as_ref()?.is_finished() {
            return None;
        }
        Some(OutputFence {
            receipts: self.detach_receipts(),
            close: self.close_result.take(),
            worker: self.worker.take().expect("checked output worker").join(),
        })
    }
    /// Actual shared outstanding owner admission, including this owner.
    pub fn owners(&self) -> Result<usize, FrameError> {
        Ok(self
            .owner
            .0
            .lock()
            .map_err(|_| FrameError::Poisoned)?
            .work
            .owners)
    }
}
impl Drop for NativeOutput {
    fn drop(&mut self) {
        self.fence();
        self.receipts.take();
    }
}
fn class(class: MessageClass) -> usize {
    match class {
        MessageClass::Control => 0,
        MessageClass::Demand => 1,
        MessageClass::Save => 2,
    }
}
fn run(
    mut writer: RecordSender,
    incoming: mpsc::Receiver<Job>,
    outgoing: PublishedSender<OutputReceipt>,
    owner: Arc<OwnerLease>,
) -> OutputReport {
    let mut queues: [VecDeque<Job>; 3] = std::array::from_fn(|_| VecDeque::new());
    let mut next = 0;
    let mut failure = None;
    let mut retained = Vec::new();
    'serve: loop {
        if queues.iter().all(VecDeque::is_empty) {
            match incoming.recv() {
                Ok(job) => queues[class(job.packet.class)].push_back(job),
                Err(_) => break,
            }
        }
        for job in incoming.try_iter() {
            queues[class(job.packet.class)].push_back(job);
        }
        let selected = (0..3)
            .map(|i| (next + i) % 3)
            .find(|i| !queues[*i].is_empty())
            .expect("active output class");
        next = (selected + 1) % 3;
        let mut job = queues[selected].pop_front().expect("selected packet");
        let offset = job
            .progress
            .as_ref()
            .map_or(0, |p| p.completed_bytes() as usize);
        let end = job.packet.bytes.len().min(offset + MAX_FRAGMENT_BYTES);
        let bytes = &job.packet.bytes[offset..end];
        let result = match &mut job.progress {
            None => writer
                .begin(
                    MessageKind::Reply,
                    job.packet.class,
                    job.packet.correlation,
                    job.packet.bytes.len() as u64,
                    bytes,
                )
                .map(|progress| job.progress = Some(progress)),
            Some(progress) => writer.continue_message(progress, bytes),
        };
        if let Err(error) = result {
            // begin's failure carries original envelope/progress in the error.
            failure = Some(error);
            retained.push(job.receipt(false));
            break;
        }
        if job.progress.as_ref().expect("started packet").complete() {
            if let Err(error) = outgoing.send(job.receipt(true)) {
                retained.push(error.0);
                break 'serve;
            }
        } else {
            queues[selected].push_back(job);
        }
    }
    for queue in queues {
        for job in queue {
            retained.push(job.receipt(false));
        }
    }
    for job in incoming.try_iter() {
        retained.push(job.receipt(false));
    }
    OutputReport {
        failure,
        retained,
        framing: writer.work(),
        native: writer.native_work(),
        _owner: owner,
    }
}
