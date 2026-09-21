//! The fixture as a **stream** rather than a buffer.
//!
//! The 100,000-entry pipeline row's fixture is 502,914,928 canonical bytes. Today the harness
//! materialises every file's bytes with `fixture::noise(file.size, …)` and hands the slice to
//! `construct_bytes`, so the whole fixture is a live allocation before the timer and the row's lifetime
//! peak is 885,325,824 B. The reference product does not work that way: its ingest opens each file and
//! drives construction from a reader, which is why the same declared workload peaked at **83,148,800 B**
//! for v0.1.6 (`benchmark-results/issue152/g1/namespace-100000-r4/perf.jsonl`).
//!
//! The replacement product already offers the same shape - `construct_stream<R: Read>`
//! (`core/crates/layerfs-content/src/file/content.rs:279`, exported at `lib.rs:27`) buffers at most
//! `small_file_threshold_bytes` = 131,072 B for its probe and then hands the reader to the chunker. So
//! closing a 10x memory gap needs no product change at all: it needs the harness to feed a reader
//! instead of a slice. That is what this module provides.
//!
//! **Byte-for-byte identical to [`crate::fixture::noise`].** [`Rng::fill`] consumes one 64-bit value per
//! eight output bytes and copies a little-endian prefix into each chunk, so the stream at any position is
//! a function of the position alone - a reader that refills in different sized windows yields exactly the
//! same bytes as one call with the whole length. `a_streamed_file_is_byte_identical_to_a_materialised_one`
//! holds that to the row's own sizes rather than to the argument.

use std::io::Read;

use crate::fixture::Rng;

/// Bytes the reader stages at a time. One mebibyte, the same window the harness already uses for
/// ordering backings and the same size the reference harness's fixture scratch is declared at.
pub const STREAM_WINDOW_BYTES: usize = 1 << 20;

/// One fixture file's bytes, generated on demand.
///
/// The reader holds a window, not the file: a 100,000,000-byte anchor costs one window plus the
/// chunker's own buffers, where the slice it replaces cost the whole file.
pub struct NoiseReader {
    rng: Rng,
    /// Bytes not yet produced.
    remaining: u64,
    /// Staged bytes not yet handed out, and how much of the stage is live.
    stage: Vec<u8>,
    staged: usize,
    cursor: usize,
}

impl NoiseReader {
    /// A reader for `len` bytes of the fixture stream seeded with `seed`.
    pub fn new(len: u64, seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            remaining: len,
            // A small file needs no window at all; a large one is capped so the reader's own footprint is
            // bounded by the window and not by the file.
            stage: vec![0_u8; STREAM_WINDOW_BYTES.min(len as usize).max(1)],
            staged: 0,
            cursor: 0,
        }
    }

    /// Bytes this reader has not produced yet.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Fills the stage from the generator.
    fn refill(&mut self) {
        let want = (self.stage.len() as u64).min(self.remaining) as usize;
        self.stage[..want].fill(0);
        self.rng.fill(&mut self.stage[..want]);
        self.remaining -= want as u64;
        self.staged = want;
        self.cursor = 0;
    }
}

impl Read for NoiseReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.cursor == self.staged {
            if self.remaining == 0 {
                return Ok(0);
            }
            self.refill();
        }
        let take = out.len().min(self.staged - self.cursor);
        out[..take].copy_from_slice(&self.stage[self.cursor..self.cursor + take]);
        self.cursor += take;
        Ok(take)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    /// Reads a whole stream in `window`-sized requests, which is how a chunker consumes it.
    fn streamed(len: u64, seed: u64, window: usize) -> Vec<u8> {
        let mut reader = NoiseReader::new(len, seed);
        let mut out = Vec::with_capacity(len as usize);
        let mut buffer = vec![0_u8; window];
        loop {
            let read = reader.read(&mut buffer).expect("stream read");
            if read == 0 {
                break;
            }
            out.extend_from_slice(&buffer[..read]);
        }
        out
    }

    #[test]
    fn a_streamed_file_is_byte_identical_to_a_materialised_one() {
        // The row's own sizes: a whole-file member, a chunked member, the largest anchor, empty, and the
        // awkward lengths where a window boundary does not land on an 8-byte generator step.
        for len in [0_u64, 1, 7, 8, 9, 4_598, 131_071, 131_072, 131_073, 1_000_000, 100_000_000] {
            let materialised = fixture::noise(len, 0x1234_5678_9abc_def0);
            assert_eq!(
                streamed(len, 0x1234_5678_9abc_def0, STREAM_WINDOW_BYTES),
                materialised,
                "default window differs at len {len}"
            );
            // A window that cuts across the generator's own step must not change a byte.
            for window in [1_usize, 3, 8, 4096, 1 << 20] {
                assert_eq!(
                    streamed(len, 0x1234_5678_9abc_def0, window),
                    materialised,
                    "window {window} differs at len {len}"
                );
            }
        }
    }

    #[test]
    fn distinct_seeds_produce_distinct_bytes() {
        assert_ne!(streamed(4096, 1, 4096), streamed(4096, 2, 4096));
    }
}
