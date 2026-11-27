//! Live counts of one running Init, read by another thread while it runs.
//!
//! Init updates them as it goes; they are observations for display, not
//! results. Totals are the caller's to know: the counts say how far Init has
//! come, not how far it has to go. [`NamespaceWork`] holds Init's final
//! count-driven acquisition diagnostics; it excludes opaque allocator/OS state.
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

/// Backed row counts, port unit counts and resident window high-water marks.
///
/// Counts grow with the acquired root because they describe rows held by the
/// acquisition backing. Window fields describe the single resident read and
/// write windows. They exclude the owned names and paths of rows being
/// processed and do not establish a total importer memory bound. Backing
/// charges are the provider's exact logical row and payload-byte charges; they
/// are not pages, file growth or resident memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct NamespaceWork {
    /// Distinct host regular-file identities constructed once.
    pub unique_files: usize,
    /// Additional host paths sharing those regular-file identities.
    pub regular_aliases: usize,
    /// Namespace entries placed in backing, including the root directory.
    pub entries: usize,
    /// Host regular-file paths bound to an identity in backing.
    pub jobs: usize,
    /// Most directories that were positioned in backing but not yet read.
    pub frontier: usize,
    /// Largest directory's child count.
    pub directory_children: usize,
    /// Directories wider than the resident child window, ordered in backing.
    pub wide_directories: usize,
    /// Most children resident at once while one directory was read.
    pub child_window_rows: usize,
    /// Directory binding rows streamed into the sorted constructors.
    pub directory_bindings: usize,
    /// Read units made on the backing, each one bounded window.
    pub read_units: u64,
    /// Write units made on the backing before cleanup, each one bounded window.
    pub write_units: u64,
    /// Most rows any one read window returned.
    pub read_window_rows: usize,
    /// Most rows any one write window carried.
    pub write_window_rows: usize,
    /// Most charged payload bytes any one write window carried.
    pub write_window_bytes: usize,
    /// Budgeted removal units cleanup made.
    pub discard_units: u64,
    /// Working rows cleanup removed: every row the operation held.
    pub backing_rows: u64,
    /// Payload bytes of those rows, as the backing charged them.
    pub backing_bytes: u64,
}
