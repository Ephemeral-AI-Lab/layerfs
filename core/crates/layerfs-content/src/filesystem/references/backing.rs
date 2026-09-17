//! Caller-supplied ordering backing: seekable run handles and owned cleanup.
//!
//! The reducer's runs live wherever the caller puts them. This module defines the
//! capability it needs (create a run, append, read at an offset) and one concrete
//! local implementation over ordinary files. Nothing here is free memory or free
//! disk: the caller's choice is charged as its real cost, its capacity is
//! explicit, and its cleanup is checked. A caller without backing can still run
//! any operation whose pending state stays inside the declared record bound; one
//! that crosses the bound fails explicitly instead of silently growing.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{ContentError, ContentResult};

/// One append-only, randomly readable run of ordering rows.
///
/// A run owns the storage it hands out: a file descriptor, a buffer, or a handle
/// into a resource the backing itself owns. It therefore never borrows from the
/// caller that created it, which is what lets the reducer keep run handles alive
/// after the backing has been released.
pub trait OrderingRun {
    /// Appends bytes at the end of the run.
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()>;
    /// Reads exactly `buffer.len()` bytes at `offset`.
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> ContentResult<()>;
    /// Flushes buffered writes so a later reader of the same run sees them.
    fn flush(&mut self) -> ContentResult<()>;
    /// Bytes written so far.
    fn len(&self) -> u64;

    /// True when nothing has been written yet.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One source of runs owned by the caller.
pub trait OrderingBacking {
    /// Creates an empty run this backing owns the resources of.
    fn create_run(&mut self) -> ContentResult<Box<dyn OrderingRun>>;
    /// Bytes of disk or memory this backing currently holds.
    fn held_bytes(&self) -> u64;
    /// Largest simultaneous bytes this backing held.
    fn peak_bytes(&self) -> u64;
    /// Releases every run this backing created, reporting any failure.
    fn release(&mut self) -> ContentResult<()>;
}

/// File-backed runs in one caller-supplied directory.
pub struct FileBacking {
    directory: PathBuf,
    created: BTreeSet<PathBuf>,
    next: u64,
    held: u64,
    peak: u64,
    released: bool,
}

impl FileBacking {
    /// A backing that will create its runs inside `directory`.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().to_path_buf(),
            created: BTreeSet::new(),
            next: 0,
            held: 0,
            peak: 0,
            released: false,
        }
    }

    /// Number of runs this backing created.
    pub fn runs(&self) -> usize {
        self.created.len()
    }
}

impl OrderingBacking for FileBacking {
    fn create_run(&mut self) -> ContentResult<Box<dyn OrderingRun>> {
        self.next = self.next.saturating_add(1);
        let path = self
            .directory
            .join(format!("layerfs-ordering-{:08}.run", self.next));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(&path)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "ordering run file",
            })?;
        self.created.insert(path);
        Ok(Box::new(FileRun { file, written: 0 }))
    }

    fn held_bytes(&self) -> u64 {
        self.held
    }

    fn peak_bytes(&self) -> u64 {
        self.peak
    }

    fn release(&mut self) -> ContentResult<()> {
        if self.released {
            return Ok(());
        }
        let mut failure = None;
        for path in std::mem::take(&mut self.created) {
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    failure = Some(());
                }
            }
        }
        self.released = failure.is_none();
        self.held = 0;
        match failure {
            Some(()) => Err(ContentError::ResourceUnavailable {
                what: "ordering run cleanup",
            }),
            None => Ok(()),
        }
    }
}

impl Drop for FileBacking {
    fn drop(&mut self) {
        // Ordinary cancellation path: remove whatever is left without hiding an
        // explicit `release` result. The operation's own completion result is
        // decided before this runs.
        for path in std::mem::take(&mut self.created) {
            let _ = std::fs::remove_file(path);
        }
        self.held = 0;
    }
}

struct FileRun {
    file: std::fs::File,
    written: u64,
}

impl OrderingRun for FileRun {
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()> {
        self.file.write_all(bytes).map_err(|_| ContentError::Io)?;
        self.written = self.written.saturating_add(bytes.len() as u64);
        Ok(())
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> ContentResult<()> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| ContentError::Io)?;
        file.read_exact(buffer).map_err(|_| ContentError::Io)
    }

    fn flush(&mut self) -> ContentResult<()> {
        self.file.flush().map_err(|_| ContentError::Io)
    }

    fn len(&self) -> u64 {
        self.written
    }
}
