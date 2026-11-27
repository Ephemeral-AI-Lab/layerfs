//! Exact fixed-window undelivered request bytes with redacted diagnostics.
use std::fmt;
/// Original consumed bytes never credited as socket delivery. No retry API is implied.
#[derive(Default)]
pub struct PendingRequest {
    pub(crate) bytes: Vec<u8>,
}
impl PendingRequest {
    /// Original retained pending bytes, available only through explicit caller inspection.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl fmt::Debug for PendingRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingRequest")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}
