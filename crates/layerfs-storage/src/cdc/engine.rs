//! One-ring lifecycle shared by the statically selected CDC algorithms.

use super::{
    sample_control, BorrowedChunkV1, BoundaryConsumerV1, CdcControlV1, CdcSourceErrorV1,
    CdcStreamCountersV1, ChunkBoundaryV1, MAXIMUM_CHUNK_BYTES,
};
use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RingSpansV1<'a> {
    first: &'a [u8],
    second: &'a [u8],
}

impl RingSpansV1<'_> {
    pub(crate) const fn contiguous(bytes: &[u8]) -> RingSpansV1<'_> {
        RingSpansV1 {
            first: bytes,
            second: &[],
        }
    }

    pub(crate) const fn len(self) -> usize {
        self.first.len() + self.second.len()
    }

    #[inline]
    pub(crate) fn byte(self, ordinal: usize) -> CoreResult<u8> {
        if ordinal < self.first.len() {
            Ok(self.first[ordinal])
        } else {
            self.second
                .get(ordinal - self.first.len())
                .copied()
                .ok_or(CoreError::IntegerOverflow)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScanStepV1 {
    pub(crate) cut: Option<usize>,
    pub(crate) inspected_bytes: u64,
}

/// Crate-private static scanner contract. Implementations are fixed-size and
/// copied into the engine; no registry, trait object, or runtime selection is
/// present in the byte path.
pub(crate) trait ScannerV1: Copy {
    fn reset(&mut self);
    fn scan(&mut self, spans: RingSpansV1<'_>) -> CoreResult<ScanStepV1>;
    fn pausable_fill_limit(&self, retained: usize) -> CoreResult<usize>;
    fn maximum_pause_lookahead(&self) -> usize;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamStateV1 {
    Active,
    Finished,
    Poisoned(CoreError),
}

pub(crate) struct EngineV1<'ring, S: ScannerV1> {
    ring: &'ring mut [u8],
    start: usize,
    len: usize,
    chunk_start: u64,
    scanner: S,
    state: StreamStateV1,
    counters: CdcStreamCountersV1,
}

impl<'ring, S: ScannerV1> EngineV1<'ring, S> {
    pub(crate) fn new<C: CdcControlV1 + ?Sized>(
        ring: &'ring mut [u8],
        scanner: S,
        control: &mut C,
    ) -> CoreResult<Self> {
        sample_control(control)?;
        if ring.len() != MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        Ok(Self {
            ring,
            start: 0,
            len: 0,
            chunk_start: 0,
            scanner,
            state: StreamStateV1::Active,
            counters: CdcStreamCountersV1::default(),
        })
    }

    pub(crate) const fn counters(&self) -> CdcStreamCountersV1 {
        self.counters
    }

    pub(crate) const fn scanner(&self) -> S {
        self.scanner
    }

    pub(crate) fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        self.push_inner(fragment, control, consumer, false)
            .map(|_| ())
    }

    pub(crate) fn push_until_consumer_pause<
        C: CdcControlV1 + ?Sized,
        B: BoundaryConsumerV1 + ?Sized,
    >(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<usize> {
        self.push_inner(fragment, control, consumer, true)
    }

    fn push_inner<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
        pausable: bool,
    ) -> CoreResult<usize> {
        self.require_active()?;
        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        let bytes = match fragment {
            Ok(bytes) => bytes,
            Err(CdcSourceErrorV1::Failure) => return self.poison(CoreError::SourceFailure),
        };
        self.counters.scan_bytes = self
            .counters
            .scan_bytes
            .checked_add(u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?)
            .ok_or(CoreError::IntegerOverflow)?;

        let mut consumed = 0_usize;
        while consumed < bytes.len() {
            if let Err(error) = sample_control(control) {
                return self.poison(error);
            }
            let capacity = MAXIMUM_CHUNK_BYTES
                .checked_sub(self.len)
                .ok_or(CoreError::IntegerOverflow)?;
            if capacity == 0 {
                return self.poison(CoreError::ChunkCap);
            }
            let write_index = self.circular_index(self.len)?;
            let contiguous = MAXIMUM_CHUNK_BYTES - write_index;
            let mut take = (bytes.len() - consumed).min(capacity).min(contiguous);
            if pausable {
                take = take.min(self.scanner.pausable_fill_limit(self.len)?);
            }
            if take == 0 {
                return self.poison(CoreError::IntegerOverflow);
            }
            self.ring[write_index..write_index + take]
                .copy_from_slice(&bytes[consumed..consumed + take]);
            self.len = self
                .len
                .checked_add(take)
                .ok_or(CoreError::IntegerOverflow)?;
            consumed = consumed
                .checked_add(take)
                .ok_or(CoreError::IntegerOverflow)?;
            self.counters.ring_fills = self
                .counters
                .ring_fills
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;

            loop {
                let step = self.scan_once()?;
                let Some(cut) = step.cut else {
                    break;
                };
                if cut == 0 || cut > self.len {
                    return self.poison(CoreError::ChunkLength);
                }
                if let Err(error) = self.publish(cut, consumer) {
                    return self.poison(error);
                }
                self.scanner.reset();
                if pausable && consumer.pause_after_accepted_boundary() {
                    return Ok(consumed);
                }
            }
        }
        Ok(consumed)
    }

    pub(crate) fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self.state {
            StreamStateV1::Poisoned(error) => return Err(error),
            StreamStateV1::Finished => return Ok(()),
            StreamStateV1::Active => {}
        }
        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        if self.len != 0 {
            if let Err(error) = self.publish(self.len, consumer) {
                return self.poison(error);
            }
        }
        self.state = StreamStateV1::Finished;
        Ok(())
    }

    pub(crate) fn finish_at_accepted_boundary<C: CdcControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
    ) -> CoreResult<()> {
        match self.state {
            StreamStateV1::Poisoned(error) => return Err(error),
            StreamStateV1::Finished => return Ok(()),
            StreamStateV1::Active => {}
        }
        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        if self.len > self.scanner.maximum_pause_lookahead() {
            return self.poison(CoreError::RangeResyncFailed);
        }
        self.len = 0;
        self.state = StreamStateV1::Finished;
        Ok(())
    }

    fn scan_once(&mut self) -> CoreResult<ScanStepV1> {
        let first_len = self.len.min(MAXIMUM_CHUNK_BYTES - self.start);
        let spans = RingSpansV1 {
            first: &self.ring[self.start..self.start + first_len],
            second: &self.ring[..self.len - first_len],
        };
        self.counters.scan_calls = self
            .counters
            .scan_calls
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let step = self.scanner.scan(spans)?;
        self.counters.boundary_inspected_bytes = self
            .counters
            .boundary_inspected_bytes
            .checked_add(step.inspected_bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(step)
    }

    fn publish<B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        chunk_len: usize,
        consumer: &mut B,
    ) -> CoreResult<()> {
        let chunk_len_u64 = u64::try_from(chunk_len).map_err(|_| CoreError::IntegerOverflow)?;
        let chunk_end = self
            .chunk_start
            .checked_add(chunk_len_u64)
            .ok_or(CoreError::IntegerOverflow)?;
        let first_len = chunk_len.min(MAXIMUM_CHUNK_BYTES - self.start);
        let second_len = chunk_len - first_len;
        if second_len != 0 {
            self.counters.ring_wrap_spans = self
                .counters
                .ring_wrap_spans
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        consumer
            .accept(
                ChunkBoundaryV1 {
                    start: self.chunk_start,
                    end: chunk_end,
                },
                BorrowedChunkV1 {
                    first: &self.ring[self.start..self.start + first_len],
                    second: &self.ring[..second_len],
                },
            )
            .map_err(|_| CoreError::SinkRefused)?;
        self.start = self.circular_index(chunk_len)?;
        self.len -= chunk_len;
        self.chunk_start = chunk_end;
        Ok(())
    }

    fn require_active(&self) -> CoreResult<()> {
        match self.state {
            StreamStateV1::Active => Ok(()),
            StreamStateV1::Finished => Err(CoreError::TrailingBytes),
            StreamStateV1::Poisoned(error) => Err(error),
        }
    }

    fn circular_index(&self, ordinal: usize) -> CoreResult<usize> {
        let linear = self
            .start
            .checked_add(ordinal)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(if linear >= MAXIMUM_CHUNK_BYTES {
            linear - MAXIMUM_CHUNK_BYTES
        } else {
            linear
        })
    }

    fn poison<T>(&mut self, error: CoreError) -> CoreResult<T> {
        self.state = StreamStateV1::Poisoned(error);
        Err(error)
    }
}
