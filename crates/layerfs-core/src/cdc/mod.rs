//! Bounded streaming content-defined chunking.

use std::io::Read;

use crate::{CoreError, CoreResult};

mod gear;

use gear::GEAR;

pub const MINIMUM_CHUNK_BYTES: usize = 8_192;
pub const TARGET_CHUNK_BYTES: usize = 16_384;
pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;
pub const NORMALIZATION_SHIFT: u32 = 2;
pub const PROFILE_SEED: u64 = 0;

const SMALL_MASK: u64 = 0x0000_d903_0353_7000;
const LARGE_MASK: u64 = 0x0000_d901_0353_0000;
const SHIFTED_SMALL_MASK: u64 = 0x0001_b206_06a6_e000;
const SHIFTED_LARGE_MASK: u64 = 0x0001_b202_06a6_0000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CdcCounters {
    pub bytes_scanned: u64,
    pub chunks_emitted: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FastCdc;

impl FastCdc {
    pub const fn new() -> Self {
        Self
    }

    pub fn scan<R: Read, F: FnMut(&[u8]) -> CoreResult<()>>(
        self,
        mut reader: R,
        mut on_chunk: F,
    ) -> CoreResult<CdcCounters> {
        let mut scanner = Scanner::new();
        let mut input = [0; MAXIMUM_CHUNK_BYTES];
        let mut counters = CdcCounters::default();

        loop {
            let read = reader.read(&mut input).map_err(|_| CoreError::Io)?;
            if read == 0 {
                scanner.finish(&mut on_chunk, &mut counters)?;
                return Ok(counters);
            }
            counters.bytes_scanned = counters
                .bytes_scanned
                .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            scanner.consume(&input[..read], &mut on_chunk, &mut counters)?;
        }
    }
}

struct Scanner {
    chunk: Vec<u8>,
    pending: Option<u8>,
    hash: u64,
}

#[inline(never)]
fn scan_region(bytes: &[u8], mut hash: u64, shifted_mask: u64, mask: u64) -> (u64, Option<usize>) {
    let mut cursor = 0;
    while cursor < bytes.len() {
        let first = bytes[cursor];
        let second = bytes[cursor + 1];
        hash = hash
            .wrapping_shl(NORMALIZATION_SHIFT)
            .wrapping_add(GEAR[usize::from(first)].wrapping_shl(1));
        if hash & shifted_mask == 0 {
            return (hash, Some(cursor));
        }
        hash = hash.wrapping_add(GEAR[usize::from(second)]);
        if hash & mask == 0 {
            return (hash, Some(cursor + 1));
        }
        cursor += 2;
    }
    (hash, None)
}

impl Scanner {
    fn new() -> Self {
        Self {
            chunk: Vec::with_capacity(MAXIMUM_CHUNK_BYTES),
            pending: None,
            hash: PROFILE_SEED,
        }
    }

    fn consume<F: FnMut(&[u8]) -> CoreResult<()>>(
        &mut self,
        mut bytes: &[u8],
        on_chunk: &mut F,
        counters: &mut CdcCounters,
    ) -> CoreResult<()> {
        if let Some(first) = self.pending.take() {
            let second = bytes[0];
            bytes = &bytes[1..];
            self.process_pending_pair(first, second, on_chunk, counters)?;
        }

        while !bytes.is_empty() {
            if self.chunk.len() < MINIMUM_CHUNK_BYTES {
                let needed = MINIMUM_CHUNK_BYTES - self.chunk.len();
                let take = needed.min(bytes.len());
                self.chunk.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if bytes.is_empty() {
                    return Ok(());
                }
            }

            let mut position = self.chunk.len();
            let mut hash = self.hash;
            let available_pairs = bytes.len() / 2;
            let small_pairs = if position < TARGET_CHUNK_BYTES {
                ((TARGET_CHUNK_BYTES - position) / 2).min(available_pairs)
            } else {
                0
            };
            let small_end = small_pairs * 2;
            let (next_hash, cut) =
                scan_region(&bytes[..small_end], hash, SHIFTED_SMALL_MASK, SMALL_MASK);
            hash = next_hash;
            let mut cursor = small_end;

            let cut = if let Some(end) = cut {
                Some(end)
            } else {
                position += small_end;
                let available_pairs = (bytes.len() - cursor) / 2;
                let large_pairs = ((MAXIMUM_CHUNK_BYTES - position) / 2).min(available_pairs);
                let large_end = cursor + large_pairs * 2;
                let (next_hash, cut) = scan_region(
                    &bytes[cursor..large_end],
                    hash,
                    SHIFTED_LARGE_MASK,
                    LARGE_MASK,
                );
                hash = next_hash;
                position += large_end - small_end;
                let start = cursor;
                cursor = large_end;
                cut.map(|end| start + end)
            };

            self.hash = hash;
            if let Some(end) = cut {
                self.chunk.extend_from_slice(&bytes[..end]);
                self.emit(on_chunk, counters)?;
                bytes = &bytes[end..];
                continue;
            }

            self.chunk.extend_from_slice(&bytes[..cursor]);
            bytes = &bytes[cursor..];
            if position == MAXIMUM_CHUNK_BYTES {
                self.emit(on_chunk, counters)?;
                continue;
            }
            if let Some(&byte) = bytes.first() {
                self.pending = Some(byte);
            }
            return Ok(());
        }
        Ok(())
    }

    fn process_pending_pair<F: FnMut(&[u8]) -> CoreResult<()>>(
        &mut self,
        first: u8,
        second: u8,
        on_chunk: &mut F,
        counters: &mut CdcCounters,
    ) -> CoreResult<()> {
        let small = self.chunk.len() < TARGET_CHUNK_BYTES;

        self.hash = self
            .hash
            .wrapping_shl(NORMALIZATION_SHIFT)
            .wrapping_add(GEAR[usize::from(first)].wrapping_shl(1));
        if self.hash
            & if small {
                SHIFTED_SMALL_MASK
            } else {
                SHIFTED_LARGE_MASK
            }
            == 0
        {
            self.emit(on_chunk, counters)?;
            self.chunk.extend_from_slice(&[first, second]);
        } else {
            self.hash = self.hash.wrapping_add(GEAR[usize::from(second)]);
            if self.hash & if small { SMALL_MASK } else { LARGE_MASK } == 0 {
                self.chunk.push(first);
                self.emit(on_chunk, counters)?;
                self.chunk.push(second);
            } else {
                self.chunk.extend_from_slice(&[first, second]);
                if self.chunk.len() == MAXIMUM_CHUNK_BYTES {
                    self.emit(on_chunk, counters)?;
                }
            }
        }
        Ok(())
    }

    fn emit<F: FnMut(&[u8]) -> CoreResult<()>>(
        &mut self,
        on_chunk: &mut F,
        counters: &mut CdcCounters,
    ) -> CoreResult<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        on_chunk(&self.chunk)?;
        counters.chunks_emitted = counters
            .chunks_emitted
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.chunk.clear();
        self.hash = PROFILE_SEED;
        Ok(())
    }

    fn finish<F: FnMut(&[u8]) -> CoreResult<()>>(
        &mut self,
        on_chunk: &mut F,
        counters: &mut CdcCounters,
    ) -> CoreResult<()> {
        if let Some(byte) = self.pending.take() {
            self.chunk.push(byte);
        }
        self.emit(on_chunk, counters)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use super::*;

    fn collect<R: Read>(reader: R) -> (Vec<usize>, CdcCounters) {
        let mut lengths = Vec::new();
        let counters = FastCdc::new()
            .scan(reader, |chunk| {
                assert!(!chunk.is_empty());
                assert!(chunk.len() <= MAXIMUM_CHUNK_BYTES);
                lengths.push(chunk.len());
                Ok(())
            })
            .unwrap();
        (lengths, counters)
    }

    fn input(len: usize) -> Vec<u8> {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..len)
            .map(|_| {
                state ^= state.wrapping_shl(7);
                state ^= state.wrapping_shr(9);
                state ^= state.wrapping_shl(8);
                state as u8
            })
            .collect()
    }

    #[test]
    fn frozen_boundaries_are_deterministic() {
        let data = input(100_000);
        let (lengths, counters) = collect(Cursor::new(data));
        assert_eq!(lengths, [16_396, 17_093, 16_413, 20_273, 19_016, 10_809]);
        assert_eq!(counters.bytes_scanned, 100_000);
        assert_eq!(counters.chunks_emitted, 6);
    }

    #[test]
    fn fragmentation_does_not_change_boundaries() {
        struct Fragmented {
            data: Vec<u8>,
            offset: usize,
            sizes: &'static [usize],
            index: usize,
        }

        impl Read for Fragmented {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                if self.offset == self.data.len() {
                    return Ok(0);
                }
                let size = self.sizes[self.index % self.sizes.len()];
                self.index += 1;
                let count = size.min(output.len()).min(self.data.len() - self.offset);
                output[..count].copy_from_slice(&self.data[self.offset..self.offset + count]);
                self.offset += count;
                Ok(count)
            }
        }

        let data = input(100_000);
        let expected = collect(Cursor::new(data.clone()));
        for sizes in [
            &[1][..],
            &[2][..],
            &[MAXIMUM_CHUNK_BYTES - 1][..],
            &[MAXIMUM_CHUNK_BYTES][..],
            &[1, 17, 4096, 3, 8192][..],
        ] {
            let actual = collect(Fragmented {
                data: data.clone(),
                offset: 0,
                sizes,
                index: 0,
            });
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn short_and_maximum_edges_are_bounded() {
        for length in [
            0,
            1,
            MINIMUM_CHUNK_BYTES - 1,
            MINIMUM_CHUNK_BYTES,
            MAXIMUM_CHUNK_BYTES,
            MAXIMUM_CHUNK_BYTES + 1,
        ] {
            let (lengths, counters) = collect(Cursor::new(input(length)));
            assert_eq!(counters.bytes_scanned, length as u64);
            assert_eq!(lengths.iter().sum::<usize>(), length);
            assert!(lengths.iter().all(|&chunk| chunk <= MAXIMUM_CHUNK_BYTES));
        }
    }

    #[test]
    fn callback_failure_is_propagated_exactly() {
        let mut callbacks = 0;
        let error = FastCdc::new()
            .scan(Cursor::new(input(100_000)), |_| {
                callbacks += 1;
                if callbacks == 2 {
                    return Err(CoreError::PublicationConflict);
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error, CoreError::PublicationConflict);
        assert_eq!(callbacks, 2);
    }

    #[test]
    fn scanner_buffer_capacity_is_fixed() {
        let scanner = Scanner::new();
        assert_eq!(scanner.chunk.capacity(), MAXIMUM_CHUNK_BYTES);
        assert!(scanner.chunk.is_empty());
    }
}
