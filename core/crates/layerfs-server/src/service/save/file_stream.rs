//! Bounded replay of one frozen final Base/Local/Zero file sequence.
use crate::service::input::Exact;
use layerfs_bridge::contract::{Code, Failure, MAX_FILE};
use layerfs_content::{ContentError, ContentResult, Edit, EditSequence, EditSource};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const RECORD_BYTES: u64 = 24;
const EDIT_BYTES: u64 = 32;
const WINDOW_BYTES: usize = 64 * 1024;
const SPOOL_DISK_BYTES: u64 = MAX_FILE * 2;

struct SpoolFile {
    path: PathBuf,
    file: File,
}

impl SpoolFile {
    fn create(label: &str) -> Result<Self, Failure> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-save-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(Failure::from)?;
        Ok(Self { path, file })
    }

    fn remove(&mut self) -> Result<(), Failure> {
        std::fs::remove_file(&self.path).map_err(Failure::from)?;
        self.path.clear();
        Ok(())
    }
}

impl Drop for SpoolFile {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

enum Bytes {
    Memory(Vec<u8>),
    File(SpoolFile),
}

pub(crate) struct FileInput {
    edits: SpoolFile,
    edit_count: usize,
    base_length: u64,
    final_length: u64,
    bytes: Bytes,
    replacement: u64,
}

impl FileInput {
    pub(crate) fn cleanup(&mut self) -> Result<(), Failure> {
        self.edits.remove()?;
        if let Bytes::File(file) = &mut self.bytes {
            file.remove()?;
        }
        Ok(())
    }
    pub(crate) fn resident(&self) -> bool {
        matches!(self.bytes, Bytes::Memory(_))
    }

    pub(crate) fn replacement_bytes(&self) -> u64 {
        self.replacement
    }

    pub(crate) fn reader(&self) -> FileReader<'_> {
        FileReader { input: self, at: 0 }
    }

    fn record(&self, index: usize) -> ContentResult<(Edit, u64)> {
        if index >= self.edit_count {
            return Err(ContentError::InvalidEdit { what: "edit index" });
        }
        let at = (index as u64)
            .checked_mul(EDIT_BYTES)
            .ok_or(ContentError::LengthOverflow)?;
        let mut file = &self.edits.file;
        file.seek(SeekFrom::Start(at))
            .map_err(|_| ContentError::Io)?;
        let mut b = [0; EDIT_BYTES as usize];
        file.read_exact(&mut b).map_err(|_| ContentError::Io)?;
        let word = |at: usize| u64::from_be_bytes(b[at..at + 8].try_into().unwrap());
        Ok((Edit::new(word(0), word(8), word(16)), word(24)))
    }

    fn read_bytes(&self, at: u64, out: &mut [u8]) -> ContentResult<usize> {
        let available = self
            .replacement
            .checked_sub(at)
            .ok_or(ContentError::LengthOverflow)?;
        let take = out
            .len()
            .min(usize::try_from(available).unwrap_or(usize::MAX));
        if take == 0 {
            return Ok(0);
        }
        match &self.bytes {
            Bytes::Memory(bytes) => {
                let start = usize::try_from(at).map_err(|_| ContentError::LengthOverflow)?;
                out[..take].copy_from_slice(&bytes[start..start + take]);
            }
            Bytes::File(spool) => {
                let mut file = &spool.file;
                file.seek(SeekFrom::Start(at))
                    .map_err(|_| ContentError::Io)?;
                file.read_exact(&mut out[..take])
                    .map_err(|_| ContentError::Io)?;
            }
        }
        Ok(take)
    }
}

impl EditSequence for FileInput {
    fn base_len(&self) -> u64 {
        self.base_length
    }
    fn final_len(&self) -> u64 {
        self.final_length
    }
    fn len(&self) -> usize {
        self.edit_count
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        self.record(index).map(|(edit, _)| edit)
    }
}

impl EditSource for FileInput {
    fn replacement_len(&self, index: usize) -> u64 {
        self.record(index)
            .map_or(0, |(edit, _)| edit.replacement_len())
    }
    fn read_at(&self, index: usize, offset: u64, out: &mut [u8]) -> ContentResult<usize> {
        let (edit, at) = self.record(index)?;
        if offset > edit.replacement_len() {
            return Err(ContentError::InvalidEdit {
                what: "replacement offset",
            });
        }
        let take = out.len().min((edit.replacement_len() - offset) as usize);
        self.read_bytes(at + offset, &mut out[..take])
    }
}

pub(crate) struct FileReader<'a> {
    input: &'a FileInput,
    at: u64,
}

impl Read for FileReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let count = self
            .input
            .read_bytes(self.at, out)
            .map_err(std::io::Error::other)?;
        self.at += count as u64;
        Ok(count)
    }
}

/// Parse, validate and spool the declared sequence before opening a C2 save.
pub(crate) fn read(
    input: &mut dyn Read,
    has_base: bool,
    base_length: u64,
    final_length: u64,
    extents: u64,
    replacement: u64,
    deadline: Instant,
) -> Result<FileInput, Failure> {
    if extents
        .checked_add(1)
        .and_then(|count| count.checked_mul(EDIT_BYTES))
        .and_then(|records| extents.checked_mul(16)?.checked_add(records))
        .and_then(|records| records.checked_add(replacement))
        .is_none_or(|bytes| bytes > SPOOL_DISK_BYTES)
    {
        return Err(Code::Capacity.into());
    }
    let mut edits = SpoolFile::create("runs")?;
    let mut zeros = SpoolFile::create("zeros")?;
    let mut zero_count = 0u64;
    let mut count = 0u64;
    let mut position = 0u64;
    let mut base = 0u64;
    let mut delta = 0i128;
    let mut pending = 0u64;
    let mut consumed = 0u64;
    let mut run_start = 0u64;
    let mut descriptors = Exact::new(
        input,
        extents.checked_mul(RECORD_BYTES).ok_or(Code::Capacity)?,
        deadline,
    );
    for _ in 0..extents {
        let mut b = [0u8; RECORD_BYTES as usize];
        descriptors.read_exact(&mut b)?;
        let word = |at: usize| u64::from_be_bytes(b[at..at + 8].try_into().unwrap());
        let kind = word(0);
        let offset = word(8);
        let length = word(16);
        if length == 0
            || position
                .checked_add(length)
                .is_none_or(|end| end > final_length)
        {
            return Err(Code::InvalidInput.into());
        }
        match kind {
            0 if has_base
                && offset >= base
                && offset
                    .checked_add(length)
                    .is_some_and(|end| end <= base_length) =>
            {
                close(
                    &mut edits.file,
                    &mut count,
                    &mut delta,
                    base,
                    offset,
                    pending,
                    run_start,
                )?;
                base = offset + length;
                pending = 0;
                run_start = consumed;
            }
            1 | 2 if offset == 0 => {
                if kind == 2 {
                    for word in [consumed, length] {
                        zeros
                            .file
                            .write_all(&word.to_be_bytes())
                            .map_err(|_| Code::Io)?;
                    }
                    zero_count += 1;
                }
                pending = pending.checked_add(length).ok_or(Code::Capacity)?;
                consumed = consumed.checked_add(length).ok_or(Code::Capacity)?;
            }
            _ => return Err(Code::InvalidInput.into()),
        }
        position += length;
    }
    if position != final_length || consumed != replacement {
        return Err(Code::InvalidInput.into());
    }
    close(
        &mut edits.file,
        &mut count,
        &mut delta,
        base,
        base_length,
        pending,
        run_start,
    )?;
    if i128::from(base_length) + delta != i128::from(final_length) || final_length > MAX_FILE {
        return Err(Code::InvalidInput.into());
    }
    let mut bytes_input = Exact::new(input, replacement, deadline);
    zeros.file.rewind().map_err(|_| Code::Io)?;
    let mut next_zero = read_zero(&mut zeros.file, &mut zero_count)?;
    let mut bytes_at = 0u64;
    let bytes = if replacement <= WINDOW_BYTES as u64 {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(replacement as usize)
            .map_err(|_| Code::Capacity)?;
        let mut buffer = [0u8; WINDOW_BYTES];
        let mut left = replacement;
        while left > 0 {
            let take = left.min(WINDOW_BYTES as u64) as usize;
            bytes_input.read_exact(&mut buffer[..take])?;
            check_zeros(
                &mut zeros.file,
                &mut zero_count,
                &mut next_zero,
                bytes_at,
                &buffer[..take],
            )?;
            bytes.extend_from_slice(&buffer[..take]);
            bytes_at += take as u64;
            left -= take as u64;
        }
        Bytes::Memory(bytes)
    } else {
        let mut file = SpoolFile::create("bytes")?;
        let mut buffer = [0u8; WINDOW_BYTES];
        let mut left = replacement;
        while left > 0 {
            let take = left.min(WINDOW_BYTES as u64) as usize;
            bytes_input.read_exact(&mut buffer[..take])?;
            check_zeros(
                &mut zeros.file,
                &mut zero_count,
                &mut next_zero,
                bytes_at,
                &buffer[..take],
            )?;
            file.file.write_all(&buffer[..take]).map_err(|_| Code::Io)?;
            bytes_at += take as u64;
            left -= take as u64;
        }
        file.file.flush().map_err(|_| Code::Io)?;
        Bytes::File(file)
    };
    if next_zero.is_some() || zero_count != 0 {
        return Err(Code::InvalidInput.into());
    }
    zeros.remove()?;
    Ok(FileInput {
        edits,
        edit_count: usize::try_from(count).map_err(|_| Code::Capacity)?,
        base_length,
        final_length,
        bytes,
        replacement,
    })
}

fn read_zero(file: &mut File, left: &mut u64) -> Result<Option<(u64, u64)>, Failure> {
    if *left == 0 {
        return Ok(None);
    }
    let mut record = [0u8; 16];
    file.read_exact(&mut record).map_err(|_| Code::Io)?;
    *left -= 1;
    Ok(Some((
        u64::from_be_bytes(record[..8].try_into().unwrap()),
        u64::from_be_bytes(record[8..].try_into().unwrap()),
    )))
}

fn check_zeros(
    file: &mut File,
    left: &mut u64,
    next: &mut Option<(u64, u64)>,
    at: u64,
    bytes: &[u8],
) -> Result<(), Failure> {
    let end = at + bytes.len() as u64;
    while let Some((start, length)) = *next {
        if start >= end {
            break;
        }
        let from = start.max(at) - at;
        let to = (start + length).min(end) - at;
        if bytes[from as usize..to as usize]
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(Code::InvalidInput.into());
        }
        if start + length > end {
            break;
        }
        *next = read_zero(file, left)?;
    }
    Ok(())
}

fn close(
    file: &mut File,
    count: &mut u64,
    delta: &mut i128,
    base: u64,
    end: u64,
    replacement: u64,
    byte_start: u64,
) -> Result<(), Failure> {
    if end == base && replacement == 0 {
        return Ok(());
    }
    let start = u64::try_from(i128::from(base) + *delta).map_err(|_| Code::InvalidInput)?;
    let stop = u64::try_from(i128::from(end) + *delta).map_err(|_| Code::InvalidInput)?;
    for word in [start, stop, replacement, byte_start] {
        file.write_all(&word.to_be_bytes()).map_err(|_| Code::Io)?;
    }
    *count = count.checked_add(1).ok_or(Code::Capacity)?;
    *delta += i128::from(replacement) - i128::from(end - base);
    Ok(())
}
