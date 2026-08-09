//! Canonical optimized FastCDC implementation.

mod gear;
mod rejoin;
mod scanner;

pub(super) use rejoin::ALGORITHM_TAG;
use scanner::FastCdcScannerV1;

use super::engine::{EngineV1, RingSpansV1, ScannerV1};
use super::{
    BoundaryConsumerV1, CdcControlV1, CdcSourceErrorV1, CdcStreamCountersV1, MAXIMUM_CHUNK_BYTES,
    MINIMUM_CHUNK_BYTES,
};
use crate::CoreResult;

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
