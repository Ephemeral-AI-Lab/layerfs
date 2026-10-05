//! Authenticated peer facts and fixed cumulative diagnostics.

/// Peer static key established by a completed KK handshake, never raw input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedPeer(pub(super) [u8; 32]);
impl VerifiedPeer {
    /// Authenticated peer identity for the application's authority adapter.
    pub const fn public_key(self) -> [u8; 32] {
        self.0
    }
}
/// Actual record work and retained buffers for one channel direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelWork {
    /// Successfully completed encrypted records.
    pub records: u64,
    /// Completed logical bytes. Cumulative counters saturate without refusing data.
    pub plaintext_bytes: u64,
    /// Actual positive socket I/O calls, including partial-progress calls.
    pub io_calls: u64,
    /// Actual transferred encrypted bytes and length prefixes, including failures.
    pub wire_bytes: u64,
    /// Currently owned canonical/plain buffer capacity.
    pub plain_capacity: usize,
    /// Currently owned encrypted buffer capacity.
    pub sealed_capacity: usize,
}
