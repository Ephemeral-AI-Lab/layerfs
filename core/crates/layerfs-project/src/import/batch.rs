//! Bounded ordered messages from file constructors to the single Save owner.
use crate::ProjectError;
use layerfs_content::{ContentError, FinalizedConsumer, FinalizedObject, ObjectId};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};

const BATCH_BYTES: usize = 256 * 1024;
const BATCH_OBJECTS: usize = 512;
const BATCH_COMPLETIONS: usize = 512;
pub(crate) const QUEUE_SLOTS: usize = 4;

pub(crate) enum Event {
    Object(FinalizedObject),
    Done(u64, Result<ObjectId, ProjectError>),
}

#[derive(Default)]
pub(crate) struct ImportBatch {
    pub(crate) events: Vec<Event>,
    bytes: usize,
    objects: usize,
    completions: usize,
}

pub(crate) struct BatchProducer<'a> {
    sender: &'a mpsc::SyncSender<ImportBatch>,
    demand: &'a AtomicU64,
    pending: ImportBatch,
    first_pending_done: Option<u64>,
}

impl<'a> BatchProducer<'a> {
    pub(crate) fn new(sender: &'a mpsc::SyncSender<ImportBatch>, demand: &'a AtomicU64) -> Self {
        Self {
            sender,
            demand,
            pending: ImportBatch::default(),
            first_pending_done: None,
        }
    }

    pub(crate) fn flush(&mut self) -> Result<(), ()> {
        if self.pending.events.is_empty() {
            return Ok(());
        }
        self.first_pending_done = None;
        self.sender
            .send(std::mem::take(&mut self.pending))
            .map_err(|_| ())
    }

    /// Per-worker job IDs increase. The canonical owner cannot consume a Done
    /// whose frame is still pending, so its next demand never passes a valid
    /// unobserved earlier Done. A conservative <= test also flushes malformed
    /// demand without creating a second completion index.
    pub(crate) fn flush_demanded(&mut self) -> Result<(), ()> {
        if self
            .first_pending_done
            .is_some_and(|first| first <= self.demand.load(Ordering::Acquire))
        {
            self.flush()?;
        }
        Ok(())
    }

    /// Hand over earlier completions before opening a known large next file.
    /// Small files keep coalescing; subsequent read/accept callbacks also poll
    /// demand because it may advance after this boundary.
    pub(crate) fn before_file(&mut self, logical_len: u64) -> Result<(), ContentError> {
        let flushed = if logical_len > BATCH_BYTES as u64 {
            self.flush()
        } else {
            self.flush_demanded()
        };
        flushed.map_err(|_| ContentError::OutputRejected)
    }

    pub(crate) fn done(
        &mut self,
        index: u64,
        result: Result<ObjectId, ProjectError>,
    ) -> Result<(), ()> {
        if self.pending.completions == BATCH_COMPLETIONS
            || self.pending.events.len() == BATCH_OBJECTS + BATCH_COMPLETIONS
        {
            self.flush()?;
        }
        let failed = result.is_err();
        self.pending.events.push(Event::Done(index, result));
        self.pending.completions += 1;
        self.first_pending_done.get_or_insert(index);
        // Errors never wait for canonical demand. Successful completions share
        // the bounded object frames unless the owner is already waiting.
        if failed {
            self.flush()
        } else {
            self.flush_demanded()
        }
    }
}

impl FinalizedConsumer for BatchProducer<'_> {
    fn accept(&mut self, object: FinalizedObject) -> Result<(), ContentError> {
        self.flush_demanded()
            .map_err(|_| ContentError::OutputRejected)?;
        let bytes = object.canonical_len();
        if bytes > layerfs_storage::policy::CANONICAL_LIMIT {
            return Err(ContentError::ObjectLimitExceeded {
                limit: layerfs_storage::policy::CANONICAL_LIMIT,
                actual: bytes,
            });
        }
        if bytes > BATCH_BYTES {
            self.flush().map_err(|_| ContentError::OutputRejected)?;
            return self
                .sender
                .send(ImportBatch {
                    events: vec![Event::Object(object)],
                    bytes,
                    objects: 1,
                    completions: 0,
                })
                .map_err(|_| ContentError::OutputRejected);
        }
        if !self.pending.events.is_empty()
            && (self.pending.objects == BATCH_OBJECTS
                || self.pending.events.len() == BATCH_OBJECTS + BATCH_COMPLETIONS
                || self.pending.bytes + bytes > BATCH_BYTES)
        {
            self.flush().map_err(|_| ContentError::OutputRejected)?;
        }
        self.pending.events.push(Event::Object(object));
        self.pending.bytes += bytes;
        self.pending.objects += 1;
        Ok(())
    }
}
