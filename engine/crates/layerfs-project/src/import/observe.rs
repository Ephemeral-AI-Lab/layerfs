//! Live counts of one running Init, read by another thread while it runs.
//!
//! Init updates them as it goes; they are observations for display, not
//! results. Totals are the caller's to know: the counts say how far Init has
//! come, not how far it has to go.
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering::Relaxed};

/// The part of Init that is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitStep {
    /// Reading the source tree and its attributes.
    Scan,
    /// Constructing each regular-file identity's content.
    Files,
    /// Consuming inode serials and building the namespace tree.
    Tree,
    /// Finishing the tree save and publishing the genesis LayerStack.
    Publish,
    /// Organizing the first Branch and sealing the Store (the caller's step).
    Seal,
}
const STEPS: [InitStep; 5] = [
    InitStep::Scan,
    InitStep::Files,
    InitStep::Tree,
    InitStep::Publish,
    InitStep::Seal,
];

/// Counts one Init updates while it runs; any thread may read them.
#[derive(Debug, Default)]
pub struct InitCounters {
    step: AtomicU8,
    entries: AtomicU64,
    files: AtomicU64,
    bytes: AtomicU64,
}

/// One reading of [`InitCounters`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitCounts {
    /// The running step.
    pub step: InitStep,
    /// Source entries the scan has observed, including the root directory.
    pub entries: u64,
    /// Regular-file identities constructed; later paths of one identity are not counted again.
    pub files: u64,
    /// Bytes of the constructed identities.
    pub bytes: u64,
}

impl InitCounters {
    /// Counters at the start of a scan, with nothing counted.
    pub fn new() -> Self {
        Self::default()
    }
    /// The counts now. Each field is read on its own, so a reading taken
    /// while Init runs may be a moment behind in one of them.
    pub fn read(&self) -> InitCounts {
        InitCounts {
            step: STEPS[usize::from(self.step.load(Relaxed)).min(STEPS.len() - 1)],
            entries: self.entries.load(Relaxed),
            files: self.files.load(Relaxed),
            bytes: self.bytes.load(Relaxed),
        }
    }
    /// Marks the step now running.
    pub fn step(&self, step: InitStep) {
        let index = STEPS.iter().position(|known| *known == step).unwrap_or(0);
        self.step.store(index as u8, Relaxed);
    }
    /// One more source entry observed.
    pub(crate) fn entry(&self) {
        self.entries.fetch_add(1, Relaxed);
    }
    /// One more regular-file identity constructed, of `len` bytes.
    pub(crate) fn built(&self, len: u64) {
        self.files.fetch_add(1, Relaxed);
        self.bytes.fetch_add(len, Relaxed);
    }
}
