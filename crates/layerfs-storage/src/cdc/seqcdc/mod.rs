//! Scalar SeqCDC alternative algorithm.
//!
//! This is a bounded Rust port of the increasing-mode boundary loop in UWASL
//! `dedup-bench` commit `8e2697cbf6332ac5da6dc615bfab82a720e820e4`,
//! `dedup/src/chunking/seq_chunking.cpp`, used under Apache-2.0. It has been
//! changed to consume the shared caller-owned ring and checked Rust offsets.

mod rejoin;
mod scanner;

pub(super) use rejoin::ALGORITHM_TAG;
use scanner::SeqCdcScannerV1;

use super::engine::{EngineV1, RingSpansV1, ScannerV1};
use super::{
    BoundaryConsumerV1, CdcControlV1, CdcSourceErrorV1, CdcStreamCountersV1, SeqCdcCountersV1,
    MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES,
};
use crate::CoreResult;

/// Exact scalar SeqCDC candidate with the frozen 16 KiB comparison profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeqCdcV1;

impl SeqCdcV1 {
    pub const fn new() -> Self {
        Self
    }

    pub fn cut(self, source: &[u8]) -> CoreResult<usize> {
        if source.len() <= MINIMUM_CHUNK_BYTES {
            return Ok(source.len());
        }
        let retained = source.len().min(MAXIMUM_CHUNK_BYTES);
        let spans = RingSpansV1::contiguous(&source[..retained]);
        let mut scanner = SeqCdcScannerV1::new();
        Ok(scanner.scan(spans)?.cut.unwrap_or(retained))
    }

    pub fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<SeqCdcV1Stream<'ring>> {
        EngineV1::new(ring, SeqCdcScannerV1::new(), control).map(SeqCdcV1Stream)
    }
}

pub struct SeqCdcV1Stream<'ring>(EngineV1<'ring, SeqCdcScannerV1>);

impl SeqCdcV1Stream<'_> {
    pub const fn counters(&self) -> CdcStreamCountersV1 {
        self.0.counters()
    }

    pub const fn seqcdc_counters(&self) -> SeqCdcCountersV1 {
        self.0.scanner().counters
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
