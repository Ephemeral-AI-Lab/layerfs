//! Sparse map parsing (GNU oldgnu & PAX 1.0), chunk validation, and streaming
//! hole/data expansion.

use std::io::{self, Read};

use super::header::{invalid, octal, BLOCK};
use super::pax::{EXTENDED_MAX, SPARSE_CHUNKS_MAX};

pub(super) struct SparseStream {
    pub(super) chunks: Vec<(u64, u64)>,
    pub(super) chunk_idx: usize,
    /// Stored bytes remaining to copy from the current chunk's input stream.
    pub(super) chunk_left: u64,
    /// Current logical byte offset emitted into the expanded regular file.
    pub(super) pos: u64,
    pub(super) realsize: u64,
    /// Trailing tar block-alignment zero padding remaining after `realsize`.
    pub(super) out_pad: u64,
    /// Remaining input padding bytes to discard once all chunks are read.
    pub(super) input_skip: u64,
}

impl SparseStream {
    pub(super) fn new(
        chunks: Vec<(u64, u64)>,
        realsize: u64,
        out_pad: u64,
        input_skip: u64,
    ) -> Self {
        Self {
            chunks,
            chunk_idx: 0,
            chunk_left: 0,
            pos: 0,
            realsize,
            out_pad,
            input_skip,
        }
    }

    pub(super) fn read_chunk<R: Read>(
        &mut self,
        input: &mut R,
        buffer: &mut [u8],
    ) -> io::Result<Option<usize>> {
        loop {
            if self.chunk_left > 0 {
                let want = (buffer.len() as u64).min(self.chunk_left) as usize;
                let n = input.read(&mut buffer[..want])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected EOF in sparse tar chunk",
                    ));
                }
                self.chunk_left -= n as u64;
                self.pos += n as u64;
                if self.chunk_left == 0 {
                    self.chunk_idx += 1;
                }
                return Ok(Some(n));
            }
            if self.pos < self.realsize {
                let hole_end = self
                    .chunks
                    .get(self.chunk_idx)
                    .map_or(self.realsize, |(off, _)| *off);
                if self.pos < hole_end {
                    let zeros = (hole_end - self.pos).min(buffer.len() as u64) as usize;
                    buffer[..zeros].fill(0);
                    self.pos += zeros as u64;
                    if self.pos == hole_end {
                        if let Some((_, len)) = self.chunks.get(self.chunk_idx) {
                            self.chunk_left = *len;
                        }
                    }
                    return Ok(Some(zeros));
                }
                if let Some((_, len)) = self.chunks.get(self.chunk_idx) {
                    self.chunk_left = *len;
                    continue;
                }
            }
            let to_skip = std::mem::take(&mut self.input_skip);
            if to_skip > 0 {
                skip_bytes(input, to_skip)?;
            }
            if self.out_pad > 0 {
                let zeros = self.out_pad.min(buffer.len() as u64) as usize;
                buffer[..zeros].fill(0);
                self.out_pad -= zeros as u64;
                return Ok(Some(zeros));
            }
            return Ok(None);
        }
    }
}

/// Reads exactly `buffer.len()` bytes, or none at a clean end.
pub(super) fn read_block<R: Read>(input: &mut R, buffer: &mut [u8]) -> io::Result<bool> {
    let mut read = 0;
    while read < buffer.len() {
        let n = input.read(&mut buffer[read..])?;
        if n == 0 {
            if read == 0 {
                return Ok(false);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the image tar ends inside a block",
            ));
        }
        read += n;
    }
    Ok(true)
}

/// Skips `count` bytes of input.
pub(super) fn skip_bytes<R: Read>(input: &mut R, count: u64) -> io::Result<()> {
    if count == 0 {
        return Ok(());
    }
    let copied = io::copy(&mut input.take(count), &mut io::sink())?;
    if copied != count {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

/// Reads GNU oldgnu extended sparse header blocks that follow a `b'S'` header.
pub(super) fn read_gnu_sparse_map<R: Read>(
    input: &mut R,
    header: &[u8; BLOCK],
    entry_size: u64,
) -> io::Result<Vec<(u64, u64)>> {
    let mut chunks = Vec::new();
    let mut stored = 0u64;
    for i in 0..4 {
        let base = 386 + i * 24;
        push_gnu_slot(
            &mut chunks,
            &mut stored,
            &header[base..base + 24],
            entry_size,
        )?;
    }
    let mut extended = header[482] != 0;
    while extended {
        let mut ext = [0u8; BLOCK];
        if !read_block(input, &mut ext)? {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mut added = false;
        for slot in 0..21 {
            let base = slot * 24;
            if push_gnu_slot(&mut chunks, &mut stored, &ext[base..base + 24], entry_size)? {
                added = true;
            }
        }
        extended = ext[504] != 0;
        if !added && extended {
            return Err(invalid("empty GNU sparse extended block"));
        }
    }
    Ok(chunks)
}

fn push_gnu_slot(
    chunks: &mut Vec<(u64, u64)>,
    stored: &mut u64,
    slot: &[u8],
    entry_size: u64,
) -> io::Result<bool> {
    let off = octal(&slot[..12])?;
    let len = octal(&slot[12..24])?;
    if len == 0 {
        return Ok(false);
    }
    if (chunks.len() as u64) >= entry_size
        || chunks.len() >= SPARSE_CHUNKS_MAX
        || stored.checked_add(len).is_none_or(|s| s > entry_size)
    {
        return Err(invalid("GNU sparse chunks exceed tar entry size"));
    }
    *stored += len;
    chunks.push((off, len));
    Ok(true)
}

/// Reads the PAX `1.0` inline newline-delimited sparse map (512-byte padded)
/// from the beginning of the entry's payload, returning `(chunks, map_bytes)`.
pub(super) fn read_pax1_sparse_map<R: Read>(
    input: &mut R,
    max_bytes: u64,
) -> io::Result<(Vec<(u64, u64)>, u64)> {
    let mut raw = Vec::new();
    let mut num_chunks: Option<usize> = None;
    let mut needed_lines: usize = 1;
    loop {
        if raw.len() as u64 + BLOCK as u64 > max_bytes.min(EXTENDED_MAX) {
            return Err(invalid(
                "PAX sparse map exceeds entry size or extended header limit",
            ));
        }
        let mut blk = [0u8; BLOCK];
        if !read_block(input, &mut blk)? {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        raw.extend_from_slice(&blk);
        if num_chunks.is_none() {
            if let Some(nl) = raw.iter().position(|b| *b == b'\n') {
                let count: usize = std::str::from_utf8(&raw[..nl])
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .ok_or_else(|| invalid("invalid PAX sparse chunk count"))?;
                needed_lines = 1usize
                    .checked_add(count.saturating_mul(2))
                    .ok_or_else(|| invalid("sparse map overflow"))?;
                num_chunks = Some(count);
            }
        }
        if num_chunks.is_some() {
            let newlines = raw.iter().filter(|b| **b == b'\n').count();
            if newlines >= needed_lines {
                break;
            }
        }
    }
    let map_bytes = raw.len() as u64;
    let count = num_chunks.unwrap_or(0);
    let mut lines = raw
        .split(|b| *b == b'\n')
        .take(needed_lines)
        .map(|l| std::str::from_utf8(l).map(str::trim));
    let _ = lines.next();
    let mut chunks = Vec::with_capacity(count);
    let mut next_u64 = |msg: &str| -> io::Result<u64> {
        lines
            .next()
            .transpose()
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| invalid(msg))
    };
    for _ in 0..count {
        let off = next_u64("invalid PAX sparse offset")?;
        let len = next_u64("invalid PAX sparse length")?;
        if len > 0 {
            chunks.push((off, len));
        }
    }
    Ok((chunks, map_bytes))
}

pub(super) fn validate_sparse_chunks(
    mut chunks: Vec<(u64, u64)>,
    realsize: u64,
) -> io::Result<(Vec<(u64, u64)>, u64)> {
    let mut prev_end = 0u64;
    let mut stored_bytes = 0u64;
    for &(off, len) in &chunks {
        if off > realsize {
            return Err(invalid("sparse chunk starts past realsize"));
        }
        if len == 0 {
            continue;
        }
        if off < prev_end {
            return Err(invalid("sparse chunks overlap or are out of order"));
        }
        let end = off
            .checked_add(len)
            .ok_or_else(|| invalid("sparse chunk end overflow"))?;
        if end > realsize {
            return Err(invalid("sparse chunk extends past realsize"));
        }
        stored_bytes = stored_bytes
            .checked_add(len)
            .ok_or_else(|| invalid("sparse byte count overflow"))?;
        prev_end = end;
    }
    chunks.retain(|(_, len)| *len > 0);
    Ok((chunks, stored_bytes))
}
