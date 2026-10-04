//! Count diagnostics collected at the actual port and cache boundaries.

/// Cumulative per-handle operation counts; these are diagnostics, never timings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    /// Profile reads.
    pub policy: u64,
    /// Batched locator calls.
    pub locate: u64,
    /// Batched physical-body calls.
    pub read_packs: u64,
    /// Whole/selected acquisition calls through the backend-neutral read port.
    pub read_pack_selections: u64,
    /// Actual whole materialization selections.
    pub whole_selected: u64,
    /// Whole selections for density/coalescing-call policy.
    pub whole_due_density: u64,
    /// Whole selections for the isolated singleton contract.
    pub whole_due_singleton: u64,
    /// Whole selections whose body already fits the acquired prefix.
    pub whole_due_small: u64,
    /// Whole selections after another selected group was retained by this owner.
    pub whole_due_reuse: u64,
    /// Actual selected complete-group materializations.
    pub range_selected: u64,
    /// Complete bytes scanned/hashed for selected acquisitions, not device bytes.
    pub range_scan_bytes: u64,
    /// Actual localized acquisition bytes including controls, excluding unread bytes.
    pub range_acquired_bytes: u64,
    /// Prefix plus selected bytes returned, separately from scanned bytes.
    pub range_materialized_bytes: u64,
    /// Catalogue set/page calls.
    pub value_groups: u64,
    /// Signature-ring reads.
    pub signatures: u64,
    /// Reservation calls.
    pub reserve: u64,
    /// Atomic registration calls.
    pub publish: u64,
    /// Payload reads.
    pub payload_reads: u64,
    /// Payload bytes returned.
    pub payload_read_bytes: u64,
    /// Physical body bytes returned.
    pub pack_read_bytes: u64,
    /// Physical body bytes acknowledged by publication.
    pub pack_write_bytes: u64,
    /// Sealed pooled packs acknowledged by registration.
    pub pooled_packs: u64,
    /// Value catalogue rows acknowledged by registration.
    pub pooled_groups: u64,
    /// Actual reserved directory bytes in acknowledged pooled packs.
    pub reserved_directory_bytes: u64,
    /// Reservation calls containing ordinal allocation.
    pub ordinal_reservations: u64,
    /// Cached locator consults.
    pub locator_hits: u64,
    /// Uncached locator consults.
    pub locator_misses: u64,
    /// Cached pack consults.
    pub pack_hits: u64,
    /// Uncached pack consults.
    pub pack_misses: u64,
    /// Selective capacity evictions across operation-owned body caches.
    pub pack_evictions: u64,
    /// Body bytes released by selective capacity eviction.
    pub pack_evicted_bytes: u64,
    /// Full-directory validation attempts in the encoded body cache.
    pub directory_validations: u64,
    /// Equivalent single-group walks for those same demands.
    pub directory_unshared_walks: u64,
    /// Sum of declared directory entry bounds, not entries visited on failure.
    pub directory_entry_bounds: u64,
    /// Actual group decompressions during dependency prefetch.
    pub prefetch_group_decodes: u64,
    /// Lanes sealed because registration requires reference closure.
    pub forced_seals: u64,
}
