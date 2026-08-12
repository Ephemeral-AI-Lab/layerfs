//! Bounded CDC engines and borrowed streaming adapters.
//!
//! FastCDC/OF is the selected canonical implementation. SeqCDC/OS is the
//! closed alternate selected explicitly by the qualification operation; the
//! implementation has no external CDC runtime dependency or fallback path.

use crate::{CoreError, CoreResult};

mod engine;
mod fastcdc;
mod resync;
#[cfg(feature = "operation-polymorphism")]
mod seqcdc;

pub use fastcdc::{FastCdcV1, FastCdcV1Stream};
pub use resync::{
    MAX_UPDATE_ANCHOR_SCAN_BYTES, MAX_UPDATE_REJOIN_VERIFICATION_BYTES,
    MAX_UPDATE_RESYNCHRONIZATION_BYTES,
};
#[cfg(feature = "operation-polymorphism")]
pub use seqcdc::{SeqCdcV1, SeqCdcV1Stream};

pub const MINIMUM_CHUNK_BYTES: usize = 8_192;
pub const TARGET_CHUNK_BYTES: usize = 16_384;
pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;
pub const NORMALIZATION_SHIFT: u32 = 2;
pub const PROFILE_SEED: u64 = 0;
pub const SMALL_MASK: u64 = 0x0000_d903_0353_7000;
pub const LARGE_MASK: u64 = 0x0000_d901_0353_0000;
pub const SHIFTED_SMALL_MASK: u64 = 0x0001_b206_06a6_e000;
pub const SHIFTED_LARGE_MASK: u64 = 0x0001_b202_06a6_0000;
pub(crate) const FASTCDC_ALGORITHM_TAG_V1: [u8; 8] = fastcdc::ALGORITHM_TAG;

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

/// Closed CDC algorithm set used by the bounded create operation.
///
/// This is not a provider registry: the choice is explicit, statically
/// dispatched, and cannot fall back or redispatch during an operation.
#[cfg(feature = "operation-polymorphism")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcAlgorithmV1 {
    FastCdc,
    SeqCdc,
}

#[cfg(feature = "operation-polymorphism")]
impl CdcAlgorithmV1 {
    pub const fn evidence_tag(self) -> [u8; 8] {
        match self {
            Self::FastCdc => fastcdc::ALGORITHM_TAG,
            Self::SeqCdc => seqcdc::ALGORITHM_TAG,
        }
    }

    pub fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<CdcStreamV1<'ring>> {
        match self {
            Self::FastCdc => FastCdcV1::new()
                .stream(ring, control)
                .map(CdcStreamV1::FastCdc),
            Self::SeqCdc => SeqCdcV1::new()
                .stream(ring, control)
                .map(CdcStreamV1::SeqCdc),
        }
    }
}

#[cfg(feature = "operation-polymorphism")]
pub enum CdcStreamV1<'ring> {
    FastCdc(FastCdcV1Stream<'ring>),
    SeqCdc(SeqCdcV1Stream<'ring>),
}

#[cfg(feature = "operation-polymorphism")]
impl CdcStreamV1<'_> {
    pub const fn counters(&self) -> CdcStreamCountersV1 {
        match self {
            Self::FastCdc(stream) => stream.counters(),
            Self::SeqCdc(stream) => stream.counters(),
        }
    }

    pub const fn seqcdc_counters(&self) -> Option<SeqCdcCountersV1> {
        match self {
            Self::FastCdc(_) => None,
            Self::SeqCdc(stream) => Some(stream.seqcdc_counters()),
        }
    }

    pub fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.push(fragment, control, consumer),
            Self::SeqCdc(stream) => stream.push(fragment, control, consumer),
        }
    }

    pub fn push_until_consumer_pause<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<usize> {
        match self {
            Self::FastCdc(stream) => stream.push_until_consumer_pause(fragment, control, consumer),
            Self::SeqCdc(stream) => stream.push_until_consumer_pause(fragment, control, consumer),
        }
    }

    pub fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.finish(control, consumer),
            Self::SeqCdc(stream) => stream.finish(control, consumer),
        }
    }

    pub fn finish_at_accepted_boundary<C: CdcControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.finish_at_accepted_boundary(control),
            Self::SeqCdc(stream) => stream.finish_at_accepted_boundary(control),
        }
    }
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
