//! Streaming `EditFile` input: descriptor-block parse and a replayable spool.
//!
//! The request's `Begin` frame declares the edit count and the replacement
//! total; the packed descriptors — 24 bytes per edit, in declared order — are
//! the prefix of the operation's body stream, and the replacement bytes follow
//! them. This module parses that prefix, validates the stream arithmetic the
//! `Begin` frame cannot see (ordering, overlap and the accumulated final
//! length, in current-result coordinates), and takes the replacement bytes
//! once into a **bounded** spool: a resident window for small totals, a
//! service-side file for large ones. The builder's comparison and construction
//! passes both read replacements through the same spool, so the C1 seam stays
//! exactly the replayable `EditSource` it always was, and one operation's
//! resident replacement cost never grows past the window plus the copy buffer.

use crate::service::input::Exact;
use layerfs_bridge::contract::{Code, Edit, Failure, MAX_FILE};
use layerfs_content::{ContentError, ContentResult, EditSource};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

/// Replacement bytes one operation may hold resident before the spool goes to
/// a service-side file. An explicit resource budget: the common Commit shape
/// (a few small edits) replays from memory; a larger declared total streams to
/// disk, so resident cost stays bounded whatever the file size.
pub(crate) const SPOOL_MEMORY_BYTES: u64 = 64 * 1024;
/// One bounded copy quantum between the network stream and the spool file.
const COPY_BYTES: usize = 64 * 1024;
/// Bytes per edit descriptor on the wire: three big-endian u64 values.
const DESCRIPTOR_BYTES: usize = 24;

/// The parsed and validated descriptor block plus the replayable spool.
pub(crate) struct EditInput {
    edits: Vec<Edit>,
    /// Logical spool offset where each edit's replacement bytes start.
    starts: Vec<u64>,
    spool: Spool,
    pub(crate) bytes: u64,
}

enum Spool {
    Memory(Vec<u8>),
    File(SpoolFile),
}

/// A service-side replay file with deterministic cleanup on drop.
struct SpoolFile {
    path: PathBuf,
    file: std::fs::File,
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn next_spool_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "layerfs-edit-spool-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Reads and validates one operation's descriptor block and spools its
/// replacement bytes from the remaining body stream.
pub(crate) fn read(
    input: &mut dyn Read,
    base_length: u64,
    count: u32,
    replacement: u64,
    deadline: Instant,
) -> Result<EditInput, Failure> {
    let descriptors = u64::from(count)
        .checked_mul(DESCRIPTOR_BYTES as u64)
        .ok_or(Code::Capacity)?;
    let mut block = Vec::with_capacity(count as usize);
    {
        let mut source = Exact::new(input, descriptors, deadline);
        let mut buffer = [0u8; COPY_BYTES];
        let mut remaining = descriptors;
        while remaining > 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            source.read_exact(&mut buffer[..take])?;
            block.extend_from_slice(&buffer[..take]);
            remaining -= take as u64;
        }
    }
    let mut edits = Vec::with_capacity(count as usize);
    let mut starts = Vec::with_capacity(count as usize);
    let mut spooled = 0u64;
    // The stream arithmetic the Begin frame cannot see: descriptors are in
    // current-result coordinates, ordered, non-overlapping and never reaching
    // into bytes an earlier replacement introduced, and the accumulated final
    // length stays inside the file ceiling.
    let mut length = base_length;
    let mut previous = 0u64;
    for index in 0..count as usize {
        let at = index * DESCRIPTOR_BYTES;
        let value = |offset: usize| -> Result<u64, Failure> {
            let bytes: [u8; 8] = block[at + offset..at + offset + 8]
                .try_into()
                .map_err(|_| Code::InvalidInput)?;
            Ok(u64::from_be_bytes(bytes))
        };
        let start = value(0)?;
        let end = value(8)?;
        let edit_replacement = value(16)?;
        if start < previous || start > end || end > length {
            return Err(Code::InvalidInput.into());
        }
        length = length
            .checked_sub(end - start)
            .and_then(|n| n.checked_add(edit_replacement))
            .ok_or(Code::InvalidInput)?;
        previous = start
            .checked_add(edit_replacement)
            .ok_or(Code::InvalidInput)?;
        if length > MAX_FILE {
            return Err(Code::Capacity.into());
        }
        starts.push(spooled);
        spooled = spooled
            .checked_add(edit_replacement)
            .ok_or(Code::Capacity)?;
        edits.push(Edit {
            start,
            end,
            replacement: edit_replacement,
        });
    }
    if spooled != replacement {
        // The declared replacement total is a stream total: the descriptors
        // and the bytes that follow them must describe the same operation.
        return Err(Code::InvalidInput.into());
    }
    let spool = if replacement <= SPOOL_MEMORY_BYTES {
        let mut memory = Vec::with_capacity(replacement as usize);
        let mut source = Exact::new(input, replacement, deadline);
        let mut buffer = [0u8; COPY_BYTES];
        let mut remaining = replacement;
        while remaining > 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            source.read_exact(&mut buffer[..take])?;
            memory.extend_from_slice(&buffer[..take]);
            remaining -= take as u64;
        }
        Spool::Memory(memory)
    } else {
        let path = next_spool_path();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(Failure::from)?;
        let mut source = Exact::new(input, replacement, deadline);
        let mut buffer = [0u8; COPY_BYTES];
        let mut remaining = replacement;
        while remaining > 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            source.read_exact(&mut buffer[..take])?;
            file.write_all(&buffer[..take]).map_err(|_| Code::Io)?;
            remaining -= take as u64;
        }
        file.flush().map_err(|_| Code::Io)?;
        Spool::File(SpoolFile { path, file })
    };
    Ok(EditInput {
        edits,
        starts,
        spool,
        bytes: replacement,
    })
}

impl EditInput {
    /// The validated descriptors, in declared order.
    pub(crate) fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// True when the replacement bytes replay from the resident window.
    pub(crate) fn resident(&self) -> bool {
        matches!(self.spool, Spool::Memory(_))
    }

    fn read_spool(&self, at: u64, out: &mut [u8]) -> ContentResult<usize> {
        let remaining = self
            .bytes
            .checked_sub(at)
            .ok_or(ContentError::LengthOverflow)?;
        let take = out
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        if take == 0 {
            return Ok(0);
        }
        match &self.spool {
            Spool::Memory(memory) => {
                let start = usize::try_from(at).map_err(|_| ContentError::LengthOverflow)?;
                out[..take].copy_from_slice(&memory[start..start + take]);
                Ok(take)
            }
            Spool::File(spool) => {
                let mut file = &spool.file;
                file.seek(SeekFrom::Start(at))
                    .map_err(|_| ContentError::Io)?;
                let mut filled = 0;
                while filled < take {
                    let read = file
                        .read(&mut out[filled..take])
                        .map_err(|_| ContentError::Io)?;
                    if read == 0 {
                        return Err(ContentError::UnexpectedEof);
                    }
                    filled += read;
                }
                Ok(take)
            }
        }
    }
}

impl EditSource for EditInput {
    fn replacement_len(&self, index: usize) -> u64 {
        self.edits.get(index).map_or(0, |edit| edit.replacement)
    }

    fn read_at(&self, index: usize, offset: u64, buffer: &mut [u8]) -> ContentResult<usize> {
        let start = self.starts.get(index).ok_or(ContentError::InvalidEdit {
            what: "replacement index",
        })?;
        let length = self.replacement_len(index);
        if offset > length {
            return Err(ContentError::InvalidEdit {
                what: "replacement offset",
            });
        }
        self.read_spool(
            start
                .checked_add(offset)
                .ok_or(ContentError::LengthOverflow)?,
            buffer,
        )
    }
}
