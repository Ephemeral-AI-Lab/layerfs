//! Closed CDC algorithm set used by the bounded C3 operation.
//!
//! This is not a provider registry: the choice is explicit, statically
//! dispatched, and cannot fall back or redispatch during an operation.

use super::{
    BoundaryConsumerV1, CdcControlV1, CdcSourceErrorV1, CdcStreamCountersV1, FastCdcV1,
    FastCdcV1Stream, SeqCdcCountersV1,
};
use crate::CoreResult;

pub use super::seqcdc::{SeqCdcV1, SeqCdcV1Stream};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum C3CdcAlgorithmV1 {
    FastCdc,
    SeqCdc,
}

impl C3CdcAlgorithmV1 {
    pub const fn evidence_tag(self) -> [u8; 8] {
        match self {
            Self::FastCdc => *b"OFCDC001",
            Self::SeqCdc => *b"OSCDC001",
        }
    }

    pub fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<C3CdcStreamV1<'ring>> {
        match self {
            Self::FastCdc => FastCdcV1::new()
                .stream(ring, control)
                .map(C3CdcStreamV1::FastCdc),
            Self::SeqCdc => SeqCdcV1::new()
                .stream(ring, control)
                .map(C3CdcStreamV1::SeqCdc),
        }
    }
}

pub enum C3CdcStreamV1<'ring> {
    FastCdc(FastCdcV1Stream<'ring>),
    SeqCdc(SeqCdcV1Stream<'ring>),
}

impl C3CdcStreamV1<'_> {
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
