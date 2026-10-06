//! Count-driven acquisition diagnostics; excludes opaque allocator/OS state.
/// Backed row counts and high-water resident buffer capacities.
///
/// Counts grow with the acquired root because they describe rows held in the
/// operation's backed scratch. Capacity fields describe resident read buffers,
/// queues and sort chunks. They exclude the owned names, targets and paths of
/// rows being processed and do not establish a total importer memory bound.
#[derive(Clone, Copy, Debug, Default)]
pub struct NamespaceWork {
    /// Distinct native regular-file identities constructed once.
    pub unique_files: usize,
    /// Additional native paths sharing those regular-file identities.
    pub regular_aliases: usize,
    /// Namespace entries placed in backing, including the root directory.
    pub entries: usize,
    /// Largest resident read-buffer capacity of the entry and alias streams.
    pub entry_capacity_bytes: usize,
    /// Native regular-file paths placed in backing before alias grouping.
    pub jobs: usize,
    /// Job stream read-buffer capacity plus the fixed worker queue's slot bytes.
    pub job_capacity_bytes: usize,
    /// Maximum backed directory frontier length.
    pub frontier: usize,
    /// Read-buffer capacity of the one frontier depth being scanned.
    pub frontier_capacity_bytes: usize,
    /// Largest directory's child count; wide directories are ordered in backing.
    pub directory_children: usize,
    /// Resident child buffer capacity bytes, at most one window of children.
    pub child_vector_bytes: usize,
    /// Always zero: serials are derived from backed order, never collected.
    pub serial_capacity_bytes: usize,
    /// Read-buffer capacities of the streams merged into the inode table.
    pub inode_capacity_bytes: usize,
    /// Directory binding rows streamed into the sorted constructors.
    pub directory_bindings: usize,
    /// Always zero: no directory update collection is resident.
    pub directory_capacity_bytes: usize,
    /// Always zero: bindings are streamed from backing, never collected.
    pub change_capacity_bytes: usize,
    /// Largest resident sort chunk plus merge buffers of any backed ordering.
    pub sort_capacity_bytes: usize,
    /// Largest simultaneous scratch run bytes the operation owned.
    pub backing_bytes: u64,
}
