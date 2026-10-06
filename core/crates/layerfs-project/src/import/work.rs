//! Count-driven namespace retention diagnostics; excludes opaque allocator/OS state.
/// High-water retained collection counts and explicitly chargeable capacities.
/// These fields do not establish a total importer memory bound.
#[derive(Clone, Copy, Debug, Default)]
pub struct NamespaceWork {
    /// Distinct native regular-file identities constructed once.
    pub unique_files: usize,
    /// Additional native paths sharing those regular-file identities.
    pub regular_aliases: usize,
    /// Prepared namespace entries retained after scan.
    pub entries: usize,
    /// Prepared-entry vector capacity bytes plus owned name/target capacities.
    pub entry_capacity_bytes: usize,
    /// File jobs retained before worker admission.
    pub jobs: usize,
    /// Job vector capacity bytes and owned path capacities.
    pub job_capacity_bytes: usize,
    /// Maximum directory frontier length.
    pub frontier: usize,
    /// Maximum frontier vector capacity plus owned path capacities.
    pub frontier_capacity_bytes: usize,
    /// Largest sorted directory-child collection.
    pub directory_children: usize,
    /// Child vector capacity bytes; DirEntry's opaque internals are excluded.
    pub child_vector_bytes: usize,
    /// Namespace serial vector capacity bytes.
    pub serial_capacity_bytes: usize,
    /// Namespace inode vector capacity bytes, excluding opaque referenced values.
    pub inode_capacity_bytes: usize,
    /// Directory binding rows retained before building the tree.
    pub directory_bindings: usize,
    /// Directory update vector capacity bytes.
    pub directory_capacity_bytes: usize,
    /// Directory change vector capacity bytes; owned PathName allocations excluded.
    pub change_capacity_bytes: usize,
}
