//! Scalar observation of ordinary FileBacking calls; no synthetic I/O outcomes.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use layerfs_content::filesystem::references::{
    FileBacking, OrderingBacking, OrderingRun, ROW_BYTES,
};
use layerfs_content::ContentResult;

#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub creates: u64,
    pub reads: u64,
    pub requested_bytes: u64,
    pub successful_bytes: u64,
    pub maximum_request: usize,
    pub single_row_reads: u64,
    pub appends: u64,
    pub appended_bytes: u64,
    pub flushes: u64,
    pub releases: u64,
}

#[derive(Clone, Default)]
pub struct Observer(Rc<Cell<Counts>>);

impl Observer {
    pub fn counts(&self) -> Counts {
        self.0.get()
    }
    fn record(&self, update: impl FnOnce(&mut Counts)) {
        let mut counts = self.0.get();
        update(&mut counts);
        self.0.set(counts);
    }
}

pub struct ObservedBacking {
    inner: FileBacking,
    observer: Observer,
}

impl ObservedBacking {
    pub fn new(directory: impl AsRef<Path>, capacity: u64) -> Self {
        Self {
            inner: FileBacking::with_capacity(directory, capacity),
            observer: Observer::default(),
        }
    }
    pub fn observer(&self) -> Observer {
        self.observer.clone()
    }
}

impl OrderingBacking for ObservedBacking {
    fn create_run(&mut self) -> ContentResult<Box<dyn OrderingRun>> {
        let inner = self.inner.create_run()?;
        self.observer.record(|counts| counts.creates += 1);
        Ok(Box::new(ObservedRun {
            inner,
            observer: self.observer.clone(),
        }))
    }
    fn held_bytes(&self) -> u64 {
        self.inner.held_bytes()
    }
    fn peak_bytes(&self) -> u64 {
        self.inner.peak_bytes()
    }
    fn capacity_bytes(&self) -> Option<u64> {
        self.inner.capacity_bytes()
    }
    fn cleanup_failed(&self) -> bool {
        self.inner.cleanup_failed()
    }
    fn release(&mut self) -> ContentResult<()> {
        self.observer.record(|counts| counts.releases += 1);
        self.inner.release()
    }
}

struct ObservedRun {
    inner: Box<dyn OrderingRun>,
    observer: Observer,
}

impl OrderingRun for ObservedRun {
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()> {
        self.observer.record(|counts| {
            counts.appends += 1;
            counts.appended_bytes += bytes.len() as u64;
        });
        self.inner.append(bytes)
    }
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> ContentResult<()> {
        self.observer.record(|counts| {
            counts.reads += 1;
            counts.requested_bytes += buffer.len() as u64;
            counts.maximum_request = counts.maximum_request.max(buffer.len());
            counts.single_row_reads += u64::from(buffer.len() == ROW_BYTES);
        });
        self.inner.read_at(offset, buffer)?;
        self.observer
            .record(|counts| counts.successful_bytes += buffer.len() as u64);
        Ok(())
    }
    fn flush(&mut self) -> ContentResult<()> {
        self.observer.record(|counts| counts.flushes += 1);
        self.inner.flush()
    }
    fn len(&self) -> u64 {
        self.inner.len()
    }
}
