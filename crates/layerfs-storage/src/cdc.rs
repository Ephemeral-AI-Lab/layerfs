//! Exact FastCDC V1 boundary engine and bounded borrowed streaming adapter.
//!
//! The implementation has no FastCDC runtime dependency. It reads the single
//! frozen GEAR authority also used to encode `ChunkerSpecV1`.

use crate::profile::GEAR;
use crate::{CoreError, CoreResult};

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

        let remaining = source.len().min(MAXIMUM_CHUNK_BYTES);
        let center = TARGET_CHUNK_BYTES.min(remaining);
        let mut index = MINIMUM_CHUNK_BYTES / 2;
        let mut hash = PROFILE_SEED;

        while index < center / 2 {
            let even = index.checked_mul(2).ok_or(CoreError::IntegerOverflow)?;
            hash = hash
                .wrapping_shl(NORMALIZATION_SHIFT)
                .wrapping_add(GEAR_LS[usize::from(source[even])]);
            if hash & SHIFTED_SMALL_MASK == 0 {
                return Ok(even);
            }
            let odd = even.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            hash = hash.wrapping_add(GEAR[usize::from(source[odd])]);
            if hash & SMALL_MASK == 0 {
                return Ok(odd);
            }
            index = index.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        }

        while index < remaining / 2 {
            let even = index.checked_mul(2).ok_or(CoreError::IntegerOverflow)?;
            hash = hash
                .wrapping_shl(NORMALIZATION_SHIFT)
                .wrapping_add(GEAR_LS[usize::from(source[even])]);
            if hash & SHIFTED_LARGE_MASK == 0 {
                return Ok(even);
            }
            let odd = even.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            hash = hash.wrapping_add(GEAR[usize::from(source[odd])]);
            if hash & LARGE_MASK == 0 {
                return Ok(odd);
            }
            index = index.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        }

        Ok(remaining)
    }

    pub fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<FastCdcV1Stream<'ring>> {
        sample_control(control)?;
        if ring.len() != MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        Ok(FastCdcV1Stream::new(ring))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamState {
    Active,
    Finished,
    Poisoned(CoreError),
}

/// One active stream borrowing exactly one caller-owned 32,768-byte ring.
pub struct FastCdcV1Stream<'ring> {
    ring: &'ring mut [u8],
    start: usize,
    len: usize,
    hash: u64,
    chunk_start: u64,
    state: StreamState,
}

impl<'ring> FastCdcV1Stream<'ring> {
    fn new(ring: &'ring mut [u8]) -> Self {
        Self {
            ring,
            start: 0,
            len: 0,
            hash: PROFILE_SEED,
            chunk_start: 0,
            state: StreamState::Active,
        }
    }

    pub fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self.state {
            StreamState::Poisoned(error) => return Err(error),
            StreamState::Finished => return Err(CoreError::TrailingBytes),
            StreamState::Active => {}
        }

        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        let bytes = match fragment {
            Ok(bytes) => bytes,
            Err(CdcSourceErrorV1::Failure) => return self.poison(CoreError::SourceFailure),
        };
        for &byte in bytes {
            if let Err(error) = self.append(byte, consumer) {
                return self.poison(error);
            }
        }
        Ok(())
    }

    /// Pushes one borrowed fragment until it is exhausted or the consumer
    /// requests a pause after accepting a boundary. The returned count is
    /// the exact prefix consumed from `fragment`; an ordinary consumer always
    /// consumes the complete fragment. A pause is active, not terminal, so
    /// callers may either resume with the suffix or finish at the accepted
    /// boundary.
    pub fn push_until_consumer_pause<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<usize> {
        match self.state {
            StreamState::Poisoned(error) => return Err(error),
            StreamState::Finished => return Err(CoreError::TrailingBytes),
            StreamState::Active => {}
        }

        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        let bytes = match fragment {
            Ok(bytes) => bytes,
            Err(CdcSourceErrorV1::Failure) => return self.poison(CoreError::SourceFailure),
        };
        for (index, &byte) in bytes.iter().enumerate() {
            if let Err(error) = self.append(byte, consumer) {
                return self.poison(error);
            }
            if consumer.pause_after_accepted_boundary() {
                return index.checked_add(1).ok_or(CoreError::IntegerOverflow);
            }
        }
        Ok(bytes.len())
    }

    pub fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self.state {
            StreamState::Poisoned(error) => return Err(error),
            StreamState::Finished => return Ok(()),
            StreamState::Active => {}
        }
        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        if self.len != 0 {
            let final_len = self.len;
            if let Err(error) = self.publish(final_len, consumer) {
                return self.poison(error);
            }
        }
        self.state = StreamState::Finished;
        Ok(())
    }

    /// Completes a stream immediately after the consumer has accepted a
    /// boundary. FastCDC's paired-byte scan may already hold at most two
    /// look-ahead bytes beyond that boundary. Range resynchronization uses
    /// this operation to discard only those proven suffix look-ahead bytes
    /// before structurally reusing the authenticated remainder.
    pub fn finish_at_accepted_boundary<C: CdcControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
    ) -> CoreResult<()> {
        match self.state {
            StreamState::Poisoned(error) => return Err(error),
            StreamState::Finished => return Ok(()),
            StreamState::Active => {}
        }
        if let Err(error) = sample_control(control) {
            return self.poison(error);
        }
        if self.len > 2 {
            return self.poison(CoreError::RangeResyncFailed);
        }
        self.len = 0;
        self.state = StreamState::Finished;
        Ok(())
    }

    fn append<B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        byte: u8,
        consumer: &mut B,
    ) -> CoreResult<()> {
        let ordinal = self.len;
        let linear = self
            .start
            .checked_add(ordinal)
            .ok_or(CoreError::IntegerOverflow)?;
        let write_index = if linear >= MAXIMUM_CHUNK_BYTES {
            linear - MAXIMUM_CHUNK_BYTES
        } else {
            linear
        };
        self.ring[write_index] = byte;
        self.len = self.len.checked_add(1).ok_or(CoreError::IntegerOverflow)?;

        if ordinal < MINIMUM_CHUNK_BYTES || ordinal % 2 == 0 {
            return Ok(());
        }
        let even_ordinal = ordinal.checked_sub(1).ok_or(CoreError::IntegerOverflow)?;
        let even_linear = self
            .start
            .checked_add(even_ordinal)
            .ok_or(CoreError::IntegerOverflow)?;
        let even_index = if even_linear >= MAXIMUM_CHUNK_BYTES {
            even_linear - MAXIMUM_CHUNK_BYTES
        } else {
            even_linear
        };
        let even_byte = self.ring[even_index];
        self.hash = self
            .hash
            .wrapping_shl(NORMALIZATION_SHIFT)
            .wrapping_add(GEAR_LS[usize::from(even_byte)]);
        let shifted_mask = if even_ordinal < TARGET_CHUNK_BYTES {
            SHIFTED_SMALL_MASK
        } else {
            SHIFTED_LARGE_MASK
        };
        if self.hash & shifted_mask == 0 {
            return self.publish(self.len - 2, consumer);
        }

        self.hash = self.hash.wrapping_add(GEAR[usize::from(byte)]);
        let mask = if even_ordinal < TARGET_CHUNK_BYTES {
            SMALL_MASK
        } else {
            LARGE_MASK
        };
        if self.hash & mask == 0 {
            self.publish(self.len - 1, consumer)
        } else if self.len == MAXIMUM_CHUNK_BYTES {
            self.publish(MAXIMUM_CHUNK_BYTES, consumer)
        } else {
            Ok(())
        }
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
        let chunk = BorrowedChunkV1 {
            first: &self.ring[self.start..self.start + first_len],
            second: &self.ring[..second_len],
        };
        let boundary = ChunkBoundaryV1 {
            start: self.chunk_start,
            end: chunk_end,
        };
        consumer
            .accept(boundary, chunk)
            .map_err(|CdcBoundaryConsumerErrorV1::Refused| CoreError::SinkRefused)?;

        self.start = if self.start + chunk_len >= MAXIMUM_CHUNK_BYTES {
            self.start + chunk_len - MAXIMUM_CHUNK_BYTES
        } else {
            self.start + chunk_len
        };
        self.len -= chunk_len;
        self.hash = PROFILE_SEED;
        self.chunk_start = chunk_end;
        Ok(())
    }

    fn poison<T>(&mut self, error: CoreError) -> CoreResult<T> {
        self.state = StreamState::Poisoned(error);
        Err(error)
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

const fn shifted_gear() -> [u64; 256] {
    let mut shifted = [0_u64; 256];
    let mut index = 0;
    while index < GEAR.len() {
        shifted[index] = GEAR[index].wrapping_shl(1);
        index += 1;
    }
    shifted
}

const GEAR_LS: [u64; 256] = shifted_gear();
