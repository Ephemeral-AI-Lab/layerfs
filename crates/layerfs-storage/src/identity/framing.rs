//! Shared framed and structural BLAKE3 hashing primitives.

use super::{COMPARISON_WINDOW_BYTES, DIGEST_BYTES, ELSHASH1};
use crate::{CoreError, CoreResult};

pub(crate) fn hash_frame(tag: u8, payload_len: u64) -> [u8; 20] {
    let mut frame = [0_u8; 20];
    frame[..8].copy_from_slice(&ELSHASH1);
    frame[8] = tag;
    frame[12..].copy_from_slice(&payload_len.to_be_bytes());
    frame
}

/// Exact-length streaming `ELSHASH1` adapter used by direct object writers.
pub(crate) struct FramedHasherV1 {
    hasher: StructuralHasher,
    expected_len: u64,
    written: u64,
}

impl FramedHasherV1 {
    pub(crate) fn new(tag: u8, payload_len: u64) -> Self {
        let mut hasher = StructuralHasher::new();
        hasher.write(&hash_frame(tag, payload_len));
        Self {
            hasher,
            expected_len: payload_len,
            written: 0,
        }
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> CoreResult<()> {
        let len = u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.written = self
            .written
            .checked_add(len)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.written > self.expected_len {
            return Err(CoreError::TrailingBytes);
        }
        self.hasher.write(bytes);
        Ok(())
    }

    pub(crate) fn finish(self) -> CoreResult<[u8; DIGEST_BYTES]> {
        if self.written != self.expected_len {
            return Err(CoreError::Truncated);
        }
        Ok(self.hasher.finish())
    }
}

pub(crate) fn derive_framed_bytes(tag: u8, payload: &[u8]) -> CoreResult<[u8; DIGEST_BYTES]> {
    let payload_len = u64::try_from(payload.len()).map_err(|_| CoreError::IntegerOverflow)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(&hash_frame(tag, payload_len));
    for block in payload.chunks(COMPARISON_WINDOW_BYTES) {
        hasher.write(block);
    }
    Ok(hasher.finish())
}

pub(super) struct StructuralHasher(blake3::Hasher);

impl StructuralHasher {
    pub(super) fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    pub(super) fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub(super) fn finish(self) -> [u8; DIGEST_BYTES] {
        *self.0.finalize().as_bytes()
    }
}
