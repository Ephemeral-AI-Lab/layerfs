//! Shared standard Engine multiplexed header; lengths remain counters.
use crate::RuntimeError;
pub(super) fn header(bytes: [u8; 8]) -> Result<(u8, u32), RuntimeError> {
    if bytes[1..4] != [0; 3] || bytes[0] > 3 {
        return Err(RuntimeError::Protocol("standard runtime frame header"));
    }
    Ok((
        bytes[0],
        u32::from_be_bytes(bytes[4..8].try_into().expect("four bytes")),
    ))
}
