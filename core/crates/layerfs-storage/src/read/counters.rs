//! Count diagnostics collected at the actual port and cache boundaries.

/// Cumulative per-handle operation counts; these are diagnostics, never timings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    /// Profile reads.
    pub policy: u64,
    /// Batched locator calls.
    pub locate: u64,
    /// Batched metadata-body calls.
    pub read_packs: u64,
    /// Catalogue set/page calls.
    pub value_groups: u64,
    /// Signature-ring reads.
    pub signatures: u64,
    /// Reservation calls.
    pub reserve: u64,
    /// Atomic registration calls.
    pub register: u64,
    /// Conditional payload creates.
    pub puts: u64,
    /// Payload reads.
    pub gets: u64,
    /// Payload length queries.
    pub heads: u64,
    /// Payload bytes submitted.
    pub put_bytes: u64,
    /// Payload bytes returned.
    pub get_bytes: u64,
    /// Metadata body bytes returned.
    pub metadata_bytes: u64,
    /// Cached locator consults.
    pub locator_hits: u64,
    /// Uncached locator consults.
    pub locator_misses: u64,
    /// Cached pack consults.
    pub pack_hits: u64,
    /// Uncached pack consults.
    pub pack_misses: u64,
    /// Lanes sealed because registration requires reference closure.
    pub forced_seals: u64,
}
