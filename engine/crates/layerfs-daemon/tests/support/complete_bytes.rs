//! Position-derived full-byte oracles with fixed memory windows.
use std::io::{self, Write};
pub const WINDOW: usize = 128 * 1024;
pub const DENSE: u64 = 500_000_000;
pub const SPARSE: u64 = 1_000_000_019;
pub const ISLANDS: [(u64, &[u8]); 3] = [
    (0, b"start\0\xff"),
    (500_000_003, b"middle-island"),
    (SPARSE - 9, b"tail-data"),
];
#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    Dense,
    Sparse,
}
pub fn fill(pattern: Pattern, offset: u64, out: &mut [u8]) {
    match pattern {
        Pattern::Dense => {
            let mut cursor = 0;
            while cursor < out.len() {
                let position = offset + cursor as u64;
                let mut word = (position / 8).wrapping_add(0x9e3779b97f4a7c15);
                word = (word ^ (word >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                word = (word ^ (word >> 27)).wrapping_mul(0x94d049bb133111eb);
                let bytes = (word ^ (word >> 31)).to_le_bytes();
                let start = (position % 8) as usize;
                let n = (8 - start).min(out.len() - cursor);
                out[cursor..cursor + n].copy_from_slice(&bytes[start..start + n]);
                cursor += n;
            }
        }
        Pattern::Sparse => {
            out.fill(0);
            for (at, bytes) in ISLANDS {
                let start = at.max(offset);
                let end = (at + bytes.len() as u64).min(offset + out.len() as u64);
                if start < end {
                    out[(start - offset) as usize..(end - offset) as usize]
                        .copy_from_slice(&bytes[(start - at) as usize..(end - at) as usize]);
                }
            }
        }
    }
}
pub struct Oracle {
    pub pattern: Pattern,
    pub position: u64,
    expected: Vec<u8>,
}
impl Oracle {
    pub fn new(pattern: Pattern) -> Self {
        Self {
            pattern,
            position: 0,
            expected: vec![0; WINDOW],
        }
    }
}
impl Write for Oracle {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for part in bytes.chunks(WINDOW) {
            fill(
                self.pattern,
                self.position,
                &mut self.expected[..part.len()],
            );
            if part != &self.expected[..part.len()] {
                return Err(io::Error::other(format!(
                    "full oracle mismatch at {}",
                    self.position
                )));
            }
            self.position += part.len() as u64;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
