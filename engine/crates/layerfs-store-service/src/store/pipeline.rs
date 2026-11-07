//! One Commit's construction on its own thread, its Save on the calling one.
//!
//! The construction producer runs on one spawned scoped thread and sends its
//! finalized objects, in emission order, through a bounded queue to the
//! calling thread, which owns the Save (`!Send`) and accepts them while the
//! producer builds the next ones. A same-Save read is a request on the same
//! queue: the producer first hands over its pending batch, then waits for the
//! answer, so the Save has accepted every object emitted before the read,
//! exactly as when one thread did both. The queue holds at most
//! [`QUEUE_SLOTS`] messages, and a batch at most [`BATCH_BYTES`] or one larger
//! object alone, the bounds Project Init uses; nothing grows with the Commit.
//!
//! The first failure ends the pipeline, with no retry or replay. A refused
//! object stops the calling thread's acceptance and closes the queue, so the
//! producer's next hand-over is refused and construction ends; what it built
//! after the refused object is dropped. A failed construction ends the
//! producer, and the calling thread abandons the Save as before.
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_storage::Save;
use std::{
    cell::RefCell,
    sync::mpsc,
    time::{Duration, Instant},
};

/// Most canonical bytes in one batch; a larger object travels alone.
const BATCH_BYTES: usize = 256 * 1024;
/// Most objects in one batch.
const BATCH_OBJECTS: usize = 512;
/// Messages in flight to the Save: 8 MiB of objects at the default small-file
/// threshold, 32 MiB at its largest, as for Init's constructors. One Save wave
/// is about 4 MiB, so the producer fills the next wave while this one is
/// published.
const QUEUE_SLOTS: usize = 32;

/// How long a wait polls before it parks its thread.
const POLL: Duration = Duration::from_micros(200);

type Reply = ContentResult<Vec<Vec<u8>>>;

enum Message {
    /// Objects in emission order.
    Objects(Vec<FinalizedObject>),
    /// A same-Save read, answered once on its own channel.
    Read(Vec<ObjectId>, mpsc::SyncSender<Reply>),
}

/// What the calling thread observed while the producer ran.
pub(super) struct Piped<T> {
    /// The producer's own result.
    pub(super) produced: T,
    /// The Save refused an object: the original error is the Save's, and
    /// the producer's result is not a complete construction.
    pub(super) rejected: bool,
    /// Time the calling thread spent accepting objects and serving reads.
    pub(super) accept: Duration,
}

/// The producer's ends of the queue: the objects it has not handed over yet,
/// and the read provider that hands them over first.
struct Pipe {
    sender: mpsc::SyncSender<Message>,
    pending: RefCell<(Vec<FinalizedObject>, usize)>,
}
impl Pipe {
    /// Hands the pending batch over; refused once the Save side stopped.
    fn flush(&self) -> ContentResult<()> {
        let objects = {
            let mut pending = self.pending.borrow_mut();
            if pending.0.is_empty() {
                return Ok(());
            }
            pending.1 = 0;
            std::mem::take(&mut pending.0)
        };
        self.send(Message::Objects(objects))
    }
    fn send(&self, message: Message) -> ContentResult<()> {
        self.sender
            .send(message)
            .map_err(|_| ContentError::OutputRejected)
    }
    fn push(&self, object: FinalizedObject) -> ContentResult<()> {
        let bytes = object.canonical_len();
        if bytes > BATCH_BYTES {
            self.flush()?;
            return self.send(Message::Objects(vec![object]));
        }
        let full = {
            let pending = self.pending.borrow();
            !pending.0.is_empty()
                && (pending.0.len() == BATCH_OBJECTS || pending.1 + bytes > BATCH_BYTES)
        };
        if full {
            self.flush()?;
        }
        let mut pending = self.pending.borrow_mut();
        pending.0.push(object);
        pending.1 += bytes;
        Ok(())
    }
}
impl AuthenticatedObjects for Pipe {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.flush()?;
        let (reply, answer) = mpsc::sync_channel(1);
        self.send(Message::Read(ids.to_vec(), reply))?;
        next(&answer).ok_or(ContentError::OutputRejected)?
    }
}
/// The producer's consumer: each object joins the pending batch.
struct PipeSink<'p>(&'p Pipe);
impl FinalizedConsumer for PipeSink<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object)
    }
}

/// The next message, or None once its sender is gone. A hand-over or a
/// read's answer usually follows within microseconds, while waking a parked
/// thread costs much more: a small Commit makes a dozen such exchanges, so
/// each wait first polls for [`POLL`] before it parks.
fn next<T>(receiver: &mpsc::Receiver<T>) -> Option<T> {
    let started = Instant::now();
    loop {
        match receiver.try_recv() {
            Ok(message) => return Some(message),
            Err(mpsc::TryRecvError::Disconnected) => return None,
            Err(mpsc::TryRecvError::Empty) if started.elapsed() < POLL => {
                std::thread::yield_now();
            }
            Err(mpsc::TryRecvError::Empty) => return receiver.recv().ok(),
        }
    }
}

/// Runs `produce` on one spawned thread over a read provider and a consumer
/// that feed `save` and `consumer` on this thread. Returns once the producer
/// has ended; its panic resumes here. A thread that cannot be started is the
/// spawn's error, before any construction.
pub(super) fn construct<T: Send>(
    save: &Save<'_>,
    consumer: &mut dyn FinalizedConsumer,
    produce: impl FnOnce(&dyn AuthenticatedObjects, &mut dyn FinalizedConsumer) -> T + Send,
) -> std::io::Result<Piped<T>> {
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel::<Message>(QUEUE_SLOTS);
        let producer = std::thread::Builder::new()
            .name("layerfs-commit-construct".into())
            .spawn_scoped(scope, move || {
                let pipe = Pipe {
                    sender,
                    pending: RefCell::new((Vec::new(), 0)),
                };
                let produced = produce(&pipe, &mut PipeSink(&pipe));
                // A refused hand-over means the Save side already stopped,
                // and it reports that itself.
                let _ = pipe.flush();
                produced
            })?;
        let mut rejected = false;
        let mut accept = Duration::ZERO;
        // Ends when the producer drops its sender, or at the first refusal.
        while let Some(message) = next(&receiver) {
            let started = Instant::now();
            match message {
                Message::Objects(objects) => {
                    for object in objects {
                        if consumer.accept(object).is_err() {
                            rejected = true;
                            break;
                        }
                    }
                }
                // The Save keeps a failed read's original error, as before.
                Message::Read(ids, reply) => {
                    let _ = reply.send(save.read_canonical_batch(&ids));
                }
            }
            accept += started.elapsed();
            if rejected {
                break;
            }
        }
        // A stopped Save side refuses every later hand-over and read.
        drop(receiver);
        let produced = producer
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        Ok(Piped {
            produced,
            rejected,
            accept,
        })
    })
}
