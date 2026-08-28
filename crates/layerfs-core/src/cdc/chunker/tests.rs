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
