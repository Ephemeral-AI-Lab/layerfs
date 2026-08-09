//! Canonical optimized FastCDC implementation.

use super::engine::{EngineV1, RingSpansV1, ScanStepV1, ScannerV1};
use super::{
    BoundaryConsumerV1, CdcControlV1, CdcSourceErrorV1, CdcStreamCountersV1, MAXIMUM_CHUNK_BYTES,
    MINIMUM_CHUNK_BYTES, NORMALIZATION_SHIFT, PROFILE_SEED, SHIFTED_LARGE_MASK, SHIFTED_SMALL_MASK,
    TARGET_CHUNK_BYTES,
};
use crate::profile::GEAR;
use crate::{CoreError, CoreResult};

const GEAR_LS: [u64; 256] = shifted_gear();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FastCdcScannerV1 {
    hash: u64,
    next_even: usize,
}

impl FastCdcScannerV1 {
    const fn new() -> Self {
        Self {
            hash: PROFILE_SEED,
            next_even: MINIMUM_CHUNK_BYTES,
        }
    }
}

impl ScannerV1 for FastCdcScannerV1 {
    fn reset(&mut self) {
        *self = Self::new();
    }

    fn scan(&mut self, spans: RingSpansV1<'_>) -> CoreResult<ScanStepV1> {
        let mut inspected = 0_u64;
        while self.next_even < MAXIMUM_CHUNK_BYTES
            && self
                .next_even
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?
                < spans.len()
        {
            let even = spans.byte(self.next_even)?;
            let odd_ordinal = self
                .next_even
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            let odd = spans.byte(odd_ordinal)?;
            inspected = inspected.checked_add(2).ok_or(CoreError::IntegerOverflow)?;
            self.hash = self
                .hash
                .wrapping_shl(NORMALIZATION_SHIFT)
                .wrapping_add(GEAR_LS[usize::from(even)]);
            let shifted_mask = if self.next_even < TARGET_CHUNK_BYTES {
                SHIFTED_SMALL_MASK
            } else {
                SHIFTED_LARGE_MASK
            };
            if self.hash & shifted_mask == 0 {
                return Ok(ScanStepV1 {
                    cut: Some(self.next_even),
                    inspected_bytes: inspected,
                });
            }
            self.hash = self.hash.wrapping_add(GEAR[usize::from(odd)]);
            let mask = if self.next_even < TARGET_CHUNK_BYTES {
                super::SMALL_MASK
            } else {
                super::LARGE_MASK
            };
            if self.hash & mask == 0 {
                return Ok(ScanStepV1 {
                    cut: Some(odd_ordinal),
                    inspected_bytes: inspected,
                });
            }
            self.next_even = self
                .next_even
                .checked_add(2)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        Ok(ScanStepV1 {
            cut: (spans.len() == MAXIMUM_CHUNK_BYTES).then_some(MAXIMUM_CHUNK_BYTES),
            inspected_bytes: inspected,
        })
    }

    fn pausable_fill_limit(&self, retained: usize) -> CoreResult<usize> {
        self.next_even
            .checked_add(2)
            .and_then(|needed| needed.checked_sub(retained))
            .ok_or(CoreError::IntegerOverflow)
    }

    fn maximum_pause_lookahead(&self) -> usize {
        2
    }
}

/// Canonical production FastCDC implementation selected at L1.5 closure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FastCdcV1;

impl FastCdcV1 {
    pub const fn new() -> Self {
        Self
    }

    pub fn cut(self, source: &[u8]) -> CoreResult<usize> {
        if source.len() <= MINIMUM_CHUNK_BYTES {
            return Ok(source.len());
        }
        let retained = source.len().min(MAXIMUM_CHUNK_BYTES);
        let spans = RingSpansV1::contiguous(&source[..retained]);
        let mut scanner = FastCdcScannerV1::new();
        Ok(scanner.scan(spans)?.cut.unwrap_or(retained))
    }

    pub fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<FastCdcV1Stream<'ring>> {
        EngineV1::new(ring, FastCdcScannerV1::new(), control).map(FastCdcV1Stream)
    }
}

/// One active optimized FastCDC stream borrowing one caller-owned 32 KiB ring.
pub struct FastCdcV1Stream<'ring>(EngineV1<'ring, FastCdcScannerV1>);

impl FastCdcV1Stream<'_> {
    pub const fn counters(&self) -> CdcStreamCountersV1 {
        self.0.counters()
    }

    pub fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        self.0.push(fragment, control, consumer)
    }

    pub fn push_until_consumer_pause<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<usize> {
        self.0
            .push_until_consumer_pause(fragment, control, consumer)
    }

    pub fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        self.0.finish(control, consumer)
    }

    pub fn finish_at_accepted_boundary<C: CdcControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
    ) -> CoreResult<()> {
        self.0.finish_at_accepted_boundary(control)
    }
}

const fn shifted_gear() -> [u64; 256] {
    let mut shifted = [0_u64; 256];
    let mut index = 0;
    while index < GEAR.len() {
        shifted[index] = GEAR[index].wrapping_shl(1);
        index += 1;
    }
    shifted
}
