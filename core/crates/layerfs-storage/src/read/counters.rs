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
    /// Stored inode value lookups: one statement execution each. The field
    /// keeps the name the diagnostics schema publishes.
    pub value_groups: u64,
    /// Signature-ring reads.
    pub signatures: u64,
    /// Reservation calls.
    pub reserve: u64,
    /// Initial combined reservation attempts, exactly one per begun Save.
    pub initial_reservations: u64,
    /// Additional new block attempts; none is a retry of a failed reservation.
    pub reservation_refills: u64,
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
    /// Always zero: inode values are stored rows, not packs. Kept for the
    /// diagnostics schema.
    pub pooled_packs: u64,
    /// Inode value rows acknowledged by registration.
    pub pooled_groups: u64,
    /// Always zero, as `pooled_packs`.
    pub reserved_directory_bytes: u64,
    /// Reservation calls containing ordinal allocation.
    pub ordinal_reservations: u64,
    /// Cached locator consults.
    pub locator_hits: u64,
    /// Uncached locator consults.
    pub locator_misses: u64,
    /// Positive locators released for capacity, excluding explicit invalidation.
    pub locator_evictions: u64,
    /// Actual second-chance victim-selection probes, including protected hits.
    pub locator_eviction_probes: u64,
    /// Referenced unprotected entries granted one second chance.
    pub locator_second_chances: u64,
    /// Peak fixed/live locator bookkeeping field bytes; excludes BTree node
    /// capacity/allocator overhead and is not a process memory measurement.
    pub locator_bookkeeping_live_peak_bytes: u64,
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
    /// Located rows that came with chain members (#758).
    pub chain_rows: u64,
    /// Chain members received.
    pub chain_members: u64,
    /// Bases located from a member, with no locate.
    pub chain_hinted_bases: u64,
    /// Hints dropped at the walk or at reconstruction.
    pub chain_hint_stale: u64,
    /// Delta roots a read walked without a chain, level by level.
    pub chain_unhinted_roots: u64,
    /// `read_groups` calls.
    pub group_reads: u64,
    /// Packs those calls named.
    pub group_read_packs: u64,
    /// Ranges those calls named.
    pub group_read_ranges: u64,
    /// Bytes those calls returned.
    pub group_read_bytes: u64,
    /// Packs read whole by plan or by the provider's answer.
    pub group_read_whole: u64,
}
