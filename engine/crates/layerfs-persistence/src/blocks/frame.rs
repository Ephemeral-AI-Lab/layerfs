//! Shared block framing: LEB128 varints, zig-zag integers and the CRC32C
//! (Castagnoli) trailer that seals each block body.
use crate::backend::records::BackendError;

/// Largest body of one block: one row per 4 KiB SQLite page with no
/// overflow, two rows per 8 KiB PostgreSQL page, stored `PLAIN`.
pub(crate) const BODY_LIMIT: usize = 4_000;

/// Width of the CRC32C trailer.
pub(crate) const CRC_BYTES: usize = 4;

const fn table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut crc = byte as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0x82F6_3B78
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[byte] = crc;
        byte += 1;
    }
    table
}
static TABLE: [u32; 256] = table();

/// CRC32C of `bytes` (reflected, initial and final XOR all ones).
pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc = TABLE[((crc ^ u32::from(*byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Appends the trailer of a finished body.
pub(crate) fn seal(body: &mut Vec<u8>) {
    let crc = crc32c(body);
    body.extend_from_slice(&crc.to_le_bytes());
}

/// The bytes before a body's trailer, after checking it.
pub(crate) fn open(body: &[u8]) -> Result<&[u8], BackendError> {
    let split = body
        .len()
        .checked_sub(CRC_BYTES)
        .ok_or(BackendError::Integrity)?;
    let (content, trailer) = body.split_at(split);
    let stored = u32::from_le_bytes(trailer.try_into().map_err(|_| BackendError::Integrity)?);
    if crc32c(content) != stored {
        return Err(BackendError::Integrity);
    }
    Ok(content)
}

/// LEB128 width of `value`.
pub(crate) const fn varint_len(mut value: u64) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

pub(crate) fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

pub(crate) const fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

pub(crate) const fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

/// A strict reader over one opened body: every read is bounds checked, a
/// varint must be minimal and fit 64 bits, and the body must end exactly
/// where its last entry does.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], BackendError> {
        let end = self.at.checked_add(n).ok_or(BackendError::Integrity)?;
        let taken = self
            .bytes
            .get(self.at..end)
            .ok_or(BackendError::Integrity)?;
        self.at = end;
        Ok(taken)
    }
    pub(crate) fn byte(&mut self) -> Result<u8, BackendError> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn varint(&mut self) -> Result<u64, BackendError> {
        let mut value = 0u64;
        for index in 0..10 {
            let byte = self.byte()?;
            let bits = u64::from(byte & 0x7f);
            // The tenth byte carries the top bit only; a zero last byte
            // after the first is a non-minimal encoding.
            if (index == 9 && byte > 1) || (index > 0 && byte == 0) {
                return Err(BackendError::Integrity);
            }
            value |= bits << (7 * index);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(BackendError::Integrity)
    }
    pub(crate) fn finished(&self) -> Result<(), BackendError> {
        match self.at == self.bytes.len() {
            true => Ok(()),
            false => Err(BackendError::Integrity),
        }
    }
}
