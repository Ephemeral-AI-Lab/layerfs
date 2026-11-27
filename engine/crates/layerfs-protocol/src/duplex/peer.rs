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
    /// Original send/receive API calls, including pre-I/O quarantine/size refusal.
    pub channel_calls: u64,
    /// Original record I/O attempts, including failed prefix/body transfer.
    pub record_io_attempts: u64,
    /// Successfully completed encrypted records.
    pub records: u64,
    /// Completed logical bytes. Cumulative counters saturate without refusing data.
    pub plaintext_bytes: u64,
    /// Actual positive socket I/O calls, including partial-progress calls.
    pub io_calls: u64,
    /// All socket Read/Write invocations, including zero/error results.
    pub io_attempts: u64,
    /// Actual transferred encrypted bytes and length prefixes, including failures.
    pub wire_bytes: u64,
    /// Exact new bytes initialized to zero in first-party stack/Vec scratch.
    pub zeroed_bytes: u64,
    /// Original Noise read/write calls, including failed authentication/encryption.
    pub crypto_attempts: u64,
    /// Exact bytes passed to those Noise calls, including failed calls.
    pub crypto_input_bytes: u64,
    /// Actual successful Noise output bytes, distinct from successful socket write.
    pub crypto_output_bytes: u64,
    /// Original first-party heap scratch allocation requests before using buffers.
    pub buffer_allocation_attempts: u64,
    /// Declared bytes of those heap scratch requests, not resident/committed heap.
    pub requested_buffer_bytes: u64,
    /// Fixed first-party stack scratch windows retained by this handshake owner.
    pub stack_window_bytes: usize,
    /// Currently owned canonical/plain buffer capacity.
    pub plain_capacity: usize,
    /// Currently owned encrypted buffer capacity.
    pub sealed_capacity: usize,
}
