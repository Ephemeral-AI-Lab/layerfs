//! Zero periods in the frozen scanner, preserving both boundary alignments.
use super::{CdcCounters, Scanner, MAXIMUM_CHUNK_BYTES};
use crate::{ContentError, ContentResult};

impl Scanner {
    /// Processes one hole, handing full interior zero chunks to the callback as
    /// a repeat count. Returns actual zero bytes scanned. One maximum chunk
    /// clears preceding nonzero bytes; the remaining zero-only scanner state
    /// repeats every 32768 bytes, including its odd pending byte and hash.
    pub(crate) fn zeros(
        &mut self,
        length: u64,
        counters: &mut CdcCounters,
        on_chunks: &mut impl FnMut(&[u8], u64) -> ContentResult<()>,
    ) -> ContentResult<u64> {
        let zeros = [0; MAXIMUM_CHUNK_BYTES];
        let head = length.min(MAXIMUM_CHUNK_BYTES as u64);
        self.consume(
            &zeros[..head as usize],
            &mut |chunk| on_chunks(chunk, 1),
            counters,
        )?;
        let remaining = length - head;
        let repeats = remaining / MAXIMUM_CHUNK_BYTES as u64;
        if repeats != 0 {
            on_chunks(&zeros, repeats)?;
            counters.chunks_emitted = counters
                .chunks_emitted
                .checked_add(repeats)
                .ok_or(ContentError::LengthOverflow)?;
        }
        let tail = remaining % MAXIMUM_CHUNK_BYTES as u64;
        if tail != 0 {
            self.consume(
                &zeros[..tail as usize],
                &mut |chunk| on_chunks(chunk, 1),
                counters,
            )?;
        }
        Ok(head + tail)
    }
}
