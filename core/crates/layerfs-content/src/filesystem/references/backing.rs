//! Caller-supplied ordering backing: owned run storage and checked cleanup.
//!
//! The reducer's runs live wherever the caller puts them. This module defines the
//! capability it needs (create a run, append, read at an offset) and one concrete
//! local implementation over ordinary files. Nothing here is free memory or free
//! disk: one explicit account owns the bytes, growth is reserved before it
//! happens, obsolete runs give their bytes back when they are dropped, and the
//! finishing cleanup is checked rather than hidden in a destructor.
//!
//! **Completion contract.** An operation calls [`OrderingBacking::release`] exactly
//! once, after every row consumer has closed its handles and before it reports
//! success. A release that fails fails the operation. A caller that needs the
//! resources kept past one operation owns them itself and passes a fresh backing to
//! the next one.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::{ContentError, ContentResult};

/// Default bytes one local file backing may own at once.
pub const DEFAULT_BACKING_CAPACITY_BYTES: u64 = 256 * 1024 * 1024;

/// One append-only, randomly readable run of ordering rows.
///
/// A run owns the storage it hands out: a file descriptor, a buffer, or a handle
/// into a resource the backing itself owns. It therefore never borrows from the
/// caller that created it, which is what lets the reducer keep run handles alive
/// while the rows are still being consumed.
pub trait OrderingRun {
    /// Appends bytes at the end of the run.
    ///
    /// The bytes are reserved against the owner's declared capacity before they
    /// are written, so a run that would exceed it fails before growing.
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
    /// Bytes this backing may own at once, when it declares a fixed ceiling.
    fn capacity_bytes(&self) -> Option<u64> {
        None
    }
    /// True when a cleanup this backing attempted did not complete.
    ///
    /// The operation reports its own failure; this accessor lets a caller see that
    /// the known-owned resources were not fully released on that path.
    fn cleanup_failed(&self) -> bool {
        false
    }
    /// Releases every run this backing created, reporting any failure.
    ///
    /// Called once, after the operation's row consumers have finished. A failure
    /// here is an operation failure: released bytes are no longer owned.
    fn release(&mut self) -> ContentResult<()>;
}

/// Shared byte account of one backing: held, peak, ceiling and cleanup state.
struct Account {
    held: Cell<u64>,
    peak: Cell<u64>,
    capacity: u64,
    runs: Cell<u64>,
    cleanup_failed: Cell<bool>,
    /// Every run path this backing still owns, with the bytes that path holds.
    ///
    /// A path leaves this map only when its file is gone. A removal that fails
    /// therefore keeps both the path and its bytes in the account, so
    /// `held_bytes` and `owns_storage` describe what is still on disk rather than
    /// what the operation wished it had removed.
    paths: RefCell<BTreeMap<PathBuf, u64>>,
}

impl Account {
    /// Reserves `bytes` before they are written.
    fn reserve(&self, bytes: u64) -> ContentResult<()> {
        let next = self
            .held
            .get()
            .checked_add(bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if next > self.capacity {
            return Err(ContentError::ObjectLimitExceeded {
                limit: usize::try_from(self.capacity).unwrap_or(usize::MAX),
                actual: usize::try_from(next).unwrap_or(usize::MAX),
            });
        }
        self.held.set(next);
        self.peak.set(self.peak.get().max(next));
        Ok(())
    }

    /// Gives `bytes` back when a run no longer owns them.
    fn release_bytes(&self, bytes: u64) {
        self.held.set(self.held.get().saturating_sub(bytes));
    }

    /// Gives one removed path's bytes back and forgets the path.
    fn released(&self, path: &Path, bytes: u64) {
        self.paths.borrow_mut().remove(path);
        self.release_bytes(bytes);
    }

    /// Adds `bytes` to the bytes one still-owned path holds.
    fn record(&self, path: &Path, bytes: u64) {
        if let Some(entry) = self.paths.borrow_mut().get_mut(path) {
            *entry = entry.saturating_add(bytes);
        }
    }
}

/// Highest run token already present in `directory`, or zero when there is none.
///
/// Run names are fixed-width so a plain byte comparison orders them, and the
/// starting token is derived from the directory instead of from process state:
/// a run file left behind by an earlier, crashed backing no longer collides with
/// the first run of every later backing in that directory and no longer makes
/// each of them fail. The scan is bounded; a directory holding more entries than
/// the bound contributes the tokens it showed, and a name that does not parse is
/// ignored rather than guessed at.
fn highest_run_token(directory: &Path) -> u64 {
    const RUN_NAME_PREFIX: &str = "layerfs-ordering-";
    const RUN_NAME_SUFFIX: &str = ".run";
    const SCAN_LIMIT: usize = 4_096;
    let Ok(entries) = std::fs::read_dir(directory) else {
        // No directory to read: naming starts at the first token and `create_run`
        // reports the real failure when it cannot create the file.
        return 0;
    };
    let mut highest = 0_u64;
    for entry in entries.take(SCAN_LIMIT).flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(digits) = name
            .strip_prefix(RUN_NAME_PREFIX)
            .and_then(|rest| rest.strip_suffix(RUN_NAME_SUFFIX))
        else {
            continue;
        };
        if let Ok(token) = digits.parse::<u64>() {
            highest = highest.max(token);
        }
    }
    highest
}

/// File-backed runs in one caller-supplied directory.
pub struct FileBacking {
    directory: PathBuf,
    next: u64,
    account: Rc<Account>,
    released: bool,
}

impl FileBacking {
    /// A backing with the default capacity that creates its runs in `directory`.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self::with_capacity(directory, DEFAULT_BACKING_CAPACITY_BYTES)
    }

    /// A backing that may own at most `capacity_bytes` at once.
    pub fn with_capacity(directory: impl AsRef<Path>, capacity_bytes: u64) -> Self {
        let directory = directory.as_ref().to_path_buf();
        let next = highest_run_token(&directory);
        Self {
            directory,
            next,
            account: Rc::new(Account {
                held: Cell::new(0),
                peak: Cell::new(0),
                capacity: capacity_bytes,
                runs: Cell::new(0),
                cleanup_failed: Cell::new(false),
                paths: RefCell::new(BTreeMap::new()),
            }),
            released: false,
        }
    }

    /// Runs this backing created.
    pub fn runs(&self) -> u64 {
        self.account.runs.get()
    }

    /// True when this backing still owns storage.
    pub fn owns_storage(&self) -> bool {
        !self.account.paths.borrow().is_empty()
    }

    /// Removes every path the account still lists, reporting the first failure.
    ///
    /// A path whose file is gone leaves the account and gives its bytes back. A
    /// path that could not be removed stays listed and stays counted, so the
    /// accessors keep describing the storage this backing still owns.
    fn discard_paths(&self) -> ContentResult<()> {
        let paths = std::mem::take(&mut *self.account.paths.borrow_mut());
        let mut failure = None;
        for (path, bytes) in paths {
            match std::fs::remove_file(&path) {
                Ok(()) => self.account.released(&path, bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.account.released(&path, bytes);
                }
                Err(_) => {
                    failure = Some(());
                    self.account.paths.borrow_mut().insert(path, bytes);
                }
            }
        }
        match failure {
            Some(()) => {
                self.account.cleanup_failed.set(true);
                Err(ContentError::ResourceUnavailable {
                    what: "ordering run cleanup",
                })
            }
            None => Ok(()),
        }
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
        self.account.paths.borrow_mut().insert(path.clone(), 0);
        self.account
            .runs
            .set(self.account.runs.get().saturating_add(1));
        Ok(Box::new(FileRun {
            file,
            path,
            written: 0,
            account: self.account.clone(),
        }))
    }

    fn held_bytes(&self) -> u64 {
        self.account.held.get()
    }

    fn peak_bytes(&self) -> u64 {
        self.account.peak.get()
    }

    fn capacity_bytes(&self) -> Option<u64> {
        Some(self.account.capacity)
    }

    fn cleanup_failed(&self) -> bool {
        self.account.cleanup_failed.get()
    }

    fn release(&mut self) -> ContentResult<()> {
        if self.released {
            return Ok(());
        }
        let outcome = self.discard_paths();
        if outcome.is_ok() {
            self.released = true;
        }
        outcome
    }
}

impl Drop for FileBacking {
    fn drop(&mut self) {
        // Ordinary cancellation path: every run this backing still owns is
        // removed, and a failure is recorded instead of being thrown away. The
        // operation's own result is decided before this runs.
        let _ = self.discard_paths();
    }
}

struct FileRun {
    file: std::fs::File,
    path: PathBuf,
    written: u64,
    account: Rc<Account>,
}

impl OrderingRun for FileRun {
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()> {
        let length = bytes.len() as u64;
        self.account.reserve(length)?;
        if self.file.write_all(bytes).is_err() {
            // The write did not happen: the reservation is returned rather than
            // counted as owned storage.
            self.account.release_bytes(length);
            return Err(ContentError::Io);
        }
        self.written = self.written.saturating_add(length);
        self.account.record(&self.path, length);
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

impl Drop for FileRun {
    fn drop(&mut self) {
        // A run stops being needed when the merge that consumed it finishes, so
        // dropping it is what returns its bytes: the file goes away and the
        // account stops counting it.
        let removed = match std::fs::remove_file(&self.path) {
            Ok(()) => true,
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        };
        if removed {
            self.account.released(&self.path, self.written);
        } else {
            // The file is still there, so it stays owned and stays counted.
            self.account.cleanup_failed.set(true);
        }
    }
}
