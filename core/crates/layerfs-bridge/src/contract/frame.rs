//! Checked borrowed fragments; transport IDs and RPC correlation are distinct.
use super::{FrameError, FrameResult};
/// Noise's largest u16 record less its authentication tag.
pub const MAX_RECORD_BYTES: usize = u16::MAX as usize - 16;
/// Fixed logical header width, paid by every fragment.
pub const HEADER_BYTES: usize = 40;
/// Largest fragment body in one authenticated native record.
pub const MAX_FRAGMENT_BYTES: usize = MAX_RECORD_BYTES - HEADER_BYTES;
/// Original direction of the logical message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageKind {
    /// One adapter request; invoking it belongs to the SDK owner.
    Request = 1,
    /// Response to an original correlated request, not evidence of publication by itself.
    Reply = 2,
}
/// Admission class; actual adapter fairness remains with the serving owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageClass {
    /// Demand object/metadata delivery.
    Demand = 1,
    /// Policy/serial/finish/history/control capacity.
    Control = 2,
    /// Canonical Save accepts.
    Save = 3,
}
/// One message's immutable envelope. IDs increase when their first record is sent;
/// correlation names the original RPC and allows replies in a different order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Envelope {
    /// Fresh direction-local transport message ID, never reused.
    pub message: u64,
    /// Exact original request identity; no inferred resend/refresh.
    pub correlation: u64,
    /// Direction/kind of this message.
    pub kind: MessageKind,
    /// Declared processing/admission class.
    pub class: MessageClass,
    /// Bytes in this bounded logical adapter message, not total file/flow size.
    pub total_bytes: u64,
}
/// One borrowed authenticated fragment; no input-sized allocation during decode.
#[derive(Clone, Copy, Debug)]
pub struct Fragment<'a> {
    /// Immutable original envelope.
    pub envelope: Envelope,
    /// Exact body offset; gaps, overlaps and envelope changes are invalid.
    pub offset: u64,
    /// Original bytes borrowed from the record owner.
    pub bytes: &'a [u8],
}
impl Fragment<'_> {
    /// Validates physical framing and range without allocating or invoking adapters.
    pub fn validate(&self) -> FrameResult<()> {
        let e = self.envelope;
        let end = self
            .offset
            .checked_add(self.bytes.len() as u64)
            .ok_or(FrameError::Invalid("fragment overflow"))?;
        if e.message == 0
            || e.correlation == 0
            || self.bytes.len() > MAX_FRAGMENT_BYTES
            || end > e.total_bytes
            || (self.bytes.is_empty() && (self.offset != 0 || e.total_bytes != 0))
        {
            return Err(FrameError::Invalid("fragment range/identity"));
        }
        Ok(())
    }
    /// True exactly when this fragment reaches its declared message end.
    pub fn is_end(&self) -> bool {
        self.offset.checked_add(self.bytes.len() as u64) == Some(self.envelope.total_bytes)
    }
}
