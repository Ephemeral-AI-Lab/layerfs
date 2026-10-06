//! A fixed admission window released only by canonical inode consumption.
use super::Job;
use crate::backing::{Backing, Stream, WINDOW_ROWS};
use crate::batch::{Event, ImportBatch, QUEUE_SLOTS};
use crate::error::{malformed, storage, ProjectError as Failure};
use crate::NamespaceWork;
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition;
use layerfs_storage::Save;
use std::{
    collections::BTreeMap,
    sync::mpsc,
    time::{Duration, Instant},
};

struct Slot {
    aliases: u64,
    root: Option<ObjectId>,
}

/// Queued, running and completed roots occupy the same fixed admission slots.
/// Objects are accepted only into this stream's Save before a Done is usable.
pub(crate) struct FileCompletions<'b, 'p, 's> {
    jobs: Stream<'b, 'p, acquisition::Job, u64>,
    feed: Option<mpsc::SyncSender<Job>>,
    receiver: Option<mpsc::Receiver<ImportBatch>>,
    slots: BTreeMap<u64, Slot>,
    save: &'b Save<'s>,
    deadline: Instant,
    last_admitted: Option<u64>,
    admitted_peak: usize,
    completed: usize,
    completed_peak: usize,
    batches: u64,
}

impl<'b, 'p, 's> FileCompletions<'b, 'p, 's> {
    pub(super) fn new(
        backing: &'b Backing<'p>,
        save: &'b Save<'s>,
        deadline: Instant,
        feed: mpsc::SyncSender<Job>,
        receiver: mpsc::Receiver<ImportBatch>,
    ) -> Self {
        Self {
            jobs: backing.jobs(),
            feed: Some(feed),
            receiver: Some(receiver),
            slots: BTreeMap::new(),
            save,
            deadline,
            last_admitted: None,
            admitted_peak: 0,
            completed: 0,
            completed_peak: 0,
            batches: 0,
        }
    }

    fn tick(&self) -> Result<(), Failure> {
        if Instant::now() >= self.deadline {
            Err(Failure::Deadline)
        } else {
            Ok(())
        }
    }

    /// Queue admission cannot exceed the slots: a queued job keeps its slot
    /// through construction and completion until the inode stream consumes it.
    pub(super) fn admit(&mut self) -> Result<(), Failure> {
        while self.feed.is_some() && self.slots.len() < WINDOW_ROWS {
            self.tick()?;
            let Some(job) = self.jobs.next()? else {
                self.feed = None;
                break;
            };
            if job.position == 0 || self.last_admitted.is_some_and(|held| held >= job.position) {
                return Err(malformed());
            }
            let id = job.position;
            let previous = self.slots.insert(
                id,
                Slot {
                    aliases: job.aliases,
                    root: None,
                },
            );
            if previous.is_some() {
                return Err(malformed());
            }
            // The channel also has WINDOW_ROWS capacity. Before this offer its
            // queued population is less than the registered slot population,
            // so Full is an invariant refusal, never a send to replay.
            self.feed
                .as_ref()
                .ok_or(Failure::WorkerUnavailable)?
                .try_send(Job::of(job))
                .map_err(|_| Failure::WorkerUnavailable)?;
            self.last_admitted = Some(id);
            self.admitted_peak = self.admitted_peak.max(self.slots.len());
        }
        Ok(())
    }

    fn accept(&mut self, batch: ImportBatch) -> Result<(), Failure> {
        self.batches += 1;
        // Drain the entire frame, including a later error, before returning a
        // requested successful root. Per-constructor channel order is exact.
        for event in batch.events {
            self.tick()?;
            match event {
                Event::Object(object) => self.save.accept(object).map_err(storage)?,
                Event::Done(id, result) => {
                    let root = result?;
                    let slot = self.slots.get_mut(&id).ok_or_else(malformed)?;
                    if slot.root.replace(root).is_some() {
                        return Err(malformed());
                    }
                    self.completed += 1;
                    self.completed_peak = self.completed_peak.max(self.completed);
                }
            }
        }
        Ok(())
    }

    /// Observe currently queued output without waiting for an unrelated job.
    /// At most one channel's capacity is drained before canonical consumption.
    fn ready(&mut self) -> Result<(), Failure> {
        for _ in 0..QUEUE_SLOTS {
            self.tick()?;
            let received = self
                .receiver
                .as_ref()
                .ok_or(Failure::WorkerUnavailable)?
                .try_recv();
            match received {
                Ok(batch) => self.accept(batch)?,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        }
        Ok(())
    }

    /// Consumes the next canonical identity, releasing exactly its one slot.
    /// A slow earliest file does not stop Object or later Done/error draining.
    pub(crate) fn root(&mut self, position: u64) -> Result<(ObjectId, u64), Failure> {
        self.admit()?;
        if self.slots.first_key_value().map(|(id, _)| *id) != Some(position) {
            return Err(malformed());
        }
        loop {
            self.ready()?;
            if self
                .slots
                .get(&position)
                .is_some_and(|slot| slot.root.is_some())
            {
                let slot = self.slots.remove(&position).ok_or_else(malformed)?;
                self.completed -= 1;
                return Ok((slot.root.ok_or_else(malformed)?, slot.aliases));
            }
            self.tick()?;
            let received = self
                .receiver
                .as_ref()
                .ok_or(Failure::WorkerUnavailable)?
                .recv_timeout(Duration::from_millis(500));
            match received {
                Ok(batch) => self.accept(batch)?,
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Failure::WorkerUnavailable)
                }
            }
        }
    }

    /// No undispatched identity or unconsumed root may escape table completion.
    pub(super) fn finish(&mut self) -> Result<(), Failure> {
        self.admit()?;
        self.ready()?;
        if self.feed.is_some() || !self.slots.is_empty() || self.completed != 0 {
            return Err(malformed());
        }
        Ok(())
    }

    pub(super) fn counters(&self, work: &mut NamespaceWork) {
        work.file_admission_rows = work.file_admission_rows.max(self.admitted_peak);
        work.file_completed_rows = work.file_completed_rows.max(self.completed_peak);
        work.file_output_batches += self.batches;
    }

    /// Called before scoped joins: a blocked job receive or output send loses
    /// its peer, so shutdown does not depend on further canonical consumption.
    pub(super) fn close(&mut self) {
        self.feed = None;
        self.receiver = None;
    }
}

impl Drop for FileCompletions<'_, '_, '_> {
    fn drop(&mut self) {
        self.close();
    }
}
