//! Count-driven acquisition diagnostics; excludes opaque allocator/OS state.
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
    /// Distinct native regular-file identities constructed once.
    pub unique_files: usize,
    /// Additional native paths sharing those regular-file identities.
    pub regular_aliases: usize,
    /// Namespace entries placed in backing, including the root directory.
    pub entries: usize,
    /// Native regular-file paths bound to an identity in backing.
    pub jobs: usize,
    /// Most queued, constructing or completed-but-unconsumed file identities;
    /// one fixed admission window, independent of the total file count.
    pub file_admission_rows: usize,
    /// Most completed roots awaiting canonical inode consumption.
    pub file_completed_rows: usize,
    /// Bounded output frames drained from constructors, including completion
    /// frames and additional object-only frames for large files.
    pub file_output_batches: u64,
    /// Productive bounded job-admission refills; each one can offer several
    /// jobs while staying within the fixed aggregate admission window.
    pub file_admission_units: u64,
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
