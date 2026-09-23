//! Bounded ordered messages from file constructors to the single Save owner.
use layerfs_bridge::contract::Failure;
use layerfs_content::{ContentError, FinalizedConsumer, FinalizedObject, ObjectId};
use std::sync::mpsc;

const BATCH_BYTES: usize = 256 * 1024;
const BATCH_OBJECTS: usize = 512;
const BATCH_COMPLETIONS: usize = 512;
pub(crate) const QUEUE_SLOTS: usize = 4;

pub(crate) enum Event {
    Object(FinalizedObject),
    Done(usize, Result<ObjectId, Failure>),
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
    pending: ImportBatch,
}

impl<'a> BatchProducer<'a> {
    pub(crate) fn new(sender: &'a mpsc::SyncSender<ImportBatch>) -> Self {
        Self {
            sender,
            pending: ImportBatch::default(),
        }
    }

    pub(crate) fn flush(&mut self) -> Result<(), ()> {
        if self.pending.events.is_empty() {
            return Ok(());
        }
        self.sender
            .send(std::mem::take(&mut self.pending))
            .map_err(|_| ())
    }

    pub(crate) fn done(
        &mut self,
        index: usize,
        result: Result<ObjectId, Failure>,
    ) -> Result<(), ()> {
        if self.pending.completions == BATCH_COMPLETIONS
            || self.pending.events.len() == BATCH_OBJECTS + BATCH_COMPLETIONS
        {
            self.flush()?;
        }
        self.pending.events.push(Event::Done(index, result));
        self.pending.completions += 1;
        Ok(())
    }
}

impl FinalizedConsumer for BatchProducer<'_> {
    fn accept(&mut self, object: FinalizedObject) -> Result<(), ContentError> {
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
