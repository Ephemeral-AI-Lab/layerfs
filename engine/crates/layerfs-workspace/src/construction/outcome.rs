//! One namespace attempt's result, original custody and counted adapter work.
use crate::{CapturedFileCustody, ConstructionBackingCustody, WorkspaceError};
use layerfs_content::{ContentResult, FilesystemResult};
use layerfs_overlay::{CapturedReader, OperationOwner};
use std::time::Instant;

/// Nanoseconds since `started`, saturating: an observation, never a limit.
pub(crate) fn nanos(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Actual adapter visits only; not whole-operation I/O, copies or residency.
/// Rows and pages come from successful returned units: failed original work
/// stays with its retained cause and is not declared zero here. Content's own
/// filesystem work is in the result's counters, each file's in its own custody.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapturedNamespaceWork {
    /// Captured inode rows the normalization pass scanned, tombstones included.
    pub inode_rows: u64,
    /// Captured name rows the normalization pass scanned, whiteouts included.
    pub entry_rows: u64,
    /// Captured inode pages requested. A page shorter than the service
    /// window ends the sealed sequence, so an empty page is requested only
    /// when the row count is an exact multiple of the window, zero included.
    pub inode_pages: u64,
    /// Whole-capture name pages requested by normalization, ended the same way.
    pub entry_pages: u64,
    /// Parent-local name pages requested by the change cursor, per pass ended
    /// the same way.
    pub change_pages: u64,
    /// Largest captured page of either kind and either route, in rows.
    pub largest_page: u64,
    /// Captured inode points that decided whether a parent keeps its header.
    pub parent_points: u64,
    /// Grouped immutable-base demands, one per page that names a base inode.
    pub base_lookups: u64,
    /// Sealed headers, the zero-change ones of fresh directories included.
    pub headers_written: u64,
    /// Parents whose names were dropped because the parent is a fresh tombstone.
    pub headers_dropped: u64,
    /// Sealed typed inode values.
    pub values_written: u64,
    /// Sealed fresh serial declarations.
    pub fresh_serials: u64,
    /// Non-root rows with no remaining link: no value, declaration or content.
    pub tombstones_skipped: u64,
    /// Successful changed-file constructions, unchanged results included.
    pub files_constructed: u64,
    /// Constructions that returned the retained base file root unchanged.
    pub files_unchanged: u64,
    /// Successful bounded shifted-file edge comparison windows.
    pub file_edge_windows: u64,
    /// Logical bytes acquired from both sources for those windows.
    pub file_edge_read_bytes: u64,
    /// File bytes retaining their base mapping after authenticated trimming.
    pub file_edge_retained_bytes: u64,
    /// Captured targets emitted for fresh symlinks.
    pub symlinks: u64,
    /// Fresh portable metadata trees built.
    pub metadata_built: u64,
    /// Base metadata trees patched with the row's mode and mtime only.
    pub metadata_patched: u64,
    /// Kept-base (mtime-only) inodes whose live row was carried past the
    /// install (STORAGE_BATCH_SPEC §4.8).
    pub live_rows_kept: u64,
    /// Fresh files whose whole-content digest was recorded in the Store.
    pub digests_recorded: u64,
    /// Fresh files that reused a saved root with the same bytes (#364).
    pub files_reused: u64,
    /// Record jobs of this adapter's normalization and cursor: a point the
    /// cursor answers from its retained window is not a job. Content's own
    /// state jobs in the same scope are in the records custody.
    pub record_jobs: u64,
    /// Header cursor passes opened.
    pub header_opens: u64,
    /// Header rows served across all passes.
    pub header_rows: u64,
    /// Header point lookups answered.
    pub header_points: u64,
    /// Parent-local change cursors opened.
    pub change_opens: u64,
    /// Changed-name rows served across all passes.
    pub change_rows: u64,
    /// Changed-name point lookups answered.
    pub change_points: u64,
    /// Typed value cursor passes opened.
    pub value_opens: u64,
    /// Typed value rows served across all passes.
    pub value_rows: u64,
    /// Typed value point lookups answered.
    pub value_points: u64,
    /// Fresh serial cursor passes opened.
    pub fresh_opens: u64,
    /// Fresh serial rows served across all passes.
    pub fresh_rows: u64,
    /// Fresh rank point lookups answered.
    pub fresh_points: u64,
    /// Record windows read, by passes and by points; each is one of the
    /// record jobs.
    pub window_jobs: u64,
    /// Wall time of name normalization, the values pass (changed files
    /// included) and Content's update, in nanoseconds. Observations only.
    pub normalize_ns: u64,
    pub values_ns: u64,
    pub update_ns: u64,
    /// The values pass split, in nanoseconds: captured inode pages with their
    /// grouped base demands; whole changed-file constructions; their binding
    /// (captured inode and base file lookups) and run reads (payload and
    /// committed bytes); objects handed to the consumer; and the pass's own
    /// record jobs. Observations only.
    pub inode_page_ns: u64,
    pub file_ns: u64,
    pub file_bind_ns: u64,
    pub file_source_ns: u64,
    pub values_emit_ns: u64,
    pub values_record_ns: u64,
    /// The update split, in nanoseconds: the cursor's record jobs, its captured
    /// name reads, and objects handed to the consumer. Observations only.
    pub update_record_ns: u64,
    pub update_capture_ns: u64,
    pub update_emit_ns: u64,
    /// Content's own state in the same scope: calls, keys read or changed
    /// (one statement each in the overlay) and their time in nanoseconds.
    pub backing_calls: u64,
    pub backing_keys: u64,
    pub backing_ns: u64,
}
/// Exact retained reader, operation owner and first-original failure for the
/// caller's fences. Exactly one of `file`, `failure` and `records.failure`
/// holds the first failure. This value and its Drop perform no reader or
/// operation release and no retry: the caller drops it, then releases the
/// reader, then the operation owner.
#[derive(Debug)]
pub struct CapturedNamespaceCustody {
    pub reader: CapturedReader,
    pub operation: OperationOwner,
    /// The namespace's own record scope; record failures remain in its failure.
    pub records: ConstructionBackingCustody,
    /// The failing file's whole original custody, when a file failed first.
    pub file: Option<Box<CapturedFileCustody>>,
    /// First captured-reader, namespace or content cause.
    pub failure: Option<WorkspaceError>,
    pub work: CapturedNamespaceWork,
}
/// One canonical update result and all original local custody, including on
/// failure, where the result is the Content label of the retained cause.
#[derive(Debug)]
pub struct CapturedNamespaceAttempt {
    pub result: ContentResult<FilesystemResult>,
    pub custody: CapturedNamespaceCustody,
}
