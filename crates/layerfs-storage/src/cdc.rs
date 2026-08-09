//! Exact FastCDC V1 boundary engine and bounded borrowed streaming adapter.
//!
//! The implementation has no FastCDC runtime dependency. It reads the single
//! frozen GEAR authority also used to encode `ChunkerSpecV1`.

use crate::{CoreError, CoreResult};

#[cfg(feature = "c3-polymorphism")]
#[doc(hidden)]
#[path = "cdc/algorithms.rs"]
pub mod algorithms;
#[path = "cdc/engine.rs"]
mod engine;
#[path = "cdc/fastcdc.rs"]
mod fastcdc;
#[cfg(feature = "c3-polymorphism")]
#[path = "cdc/seqcdc.rs"]
mod seqcdc;

pub use fastcdc::{FastCdcV1, FastCdcV1Stream};

pub const MINIMUM_CHUNK_BYTES: usize = 8_192;
pub const TARGET_CHUNK_BYTES: usize = 16_384;
pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;
pub const NORMALIZATION_SHIFT: u32 = 2;
pub const PROFILE_SEED: u64 = 0;
pub const SMALL_MASK: u64 = 0x0000_d903_0353_7000;
pub const LARGE_MASK: u64 = 0x0000_d901_0353_0000;
pub const SHIFTED_SMALL_MASK: u64 = 0x0001_b206_06a6_e000;
pub const SHIFTED_LARGE_MASK: u64 = 0x0001_b202_06a6_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcSourceErrorV1 {
    Failure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcBoundaryConsumerErrorV1 {
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkBoundaryV1 {
    start: u64,
    end: u64,
}

#[allow(clippy::len_without_is_empty)]
impl ChunkBoundaryV1 {
    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end(self) -> u64 {
        self.end
    }

    pub const fn len(self) -> u64 {
        self.end - self.start
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BorrowedChunkV1<'chunk> {
    first: &'chunk [u8],
    second: &'chunk [u8],
}

#[allow(clippy::len_without_is_empty)]
impl<'chunk> BorrowedChunkV1<'chunk> {
    pub const fn first(self) -> &'chunk [u8] {
        self.first
    }

    pub const fn second(self) -> &'chunk [u8] {
        self.second
    }

    pub const fn len(self) -> usize {
        self.first.len() + self.second.len()
    }
}

pub trait BoundaryConsumerV1 {
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1>;

    /// A consumer may request a non-terminal pause immediately after it has
    /// accepted a boundary. The default preserves the ordinary streaming
    /// lifecycle. Range resynchronization uses this to stop at its proven
    /// rejoin without publishing or copying any later suffix chunk.
    fn pause_after_accepted_boundary(&self) -> bool {
        false
    }
}

pub trait CdcControlV1 {
    fn cancellation_requested(&mut self) -> bool;
    fn deadline_exceeded(&mut self) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContinueCdcControlV1;

impl CdcControlV1 for ContinueCdcControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

/// Exact scalar work totals for one CDC stream. These counters are not part
/// of canonical output and retain no payload or boundary vector.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CdcStreamCountersV1 {
    pub ring_fills: u64,
    pub ring_wrap_spans: u64,
    pub scan_calls: u64,
    pub scan_bytes: u64,
    pub boundary_inspected_bytes: u64,
}

impl CdcStreamCountersV1 {
    pub fn checked_delta(self, earlier: Self) -> CoreResult<Self> {
        Ok(Self {
            ring_fills: self
                .ring_fills
                .checked_sub(earlier.ring_fills)
                .ok_or(CoreError::IntegerOverflow)?,
            ring_wrap_spans: self
                .ring_wrap_spans
                .checked_sub(earlier.ring_wrap_spans)
                .ok_or(CoreError::IntegerOverflow)?,
            scan_calls: self
                .scan_calls
                .checked_sub(earlier.scan_calls)
                .ok_or(CoreError::IntegerOverflow)?,
            scan_bytes: self
                .scan_bytes
                .checked_sub(earlier.scan_bytes)
                .ok_or(CoreError::IntegerOverflow)?,
            boundary_inspected_bytes: self
                .boundary_inspected_bytes
                .checked_sub(earlier.boundary_inspected_bytes)
                .ok_or(CoreError::IntegerOverflow)?,
        })
    }
}

/// SeqCDC-only scalar work totals. Keeping these separate makes the canonical
/// FastCDC stream pay zero storage cost for the alternative algorithm.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeqCdcCountersV1 {
    pub comparisons: u64,
    pub equal_absorptions: u64,
    pub opposing_slopes: u64,
    pub jumps: u64,
    pub jump_bytes: u64,
}

fn sample_control<C: CdcControlV1 + ?Sized>(control: &mut C) -> CoreResult<()> {
    if control.cancellation_requested() {
        Err(CoreError::Cancelled)
    } else if control.deadline_exceeded() {
        Err(CoreError::Deadline)
    } else {
        Ok(())
    }
}
