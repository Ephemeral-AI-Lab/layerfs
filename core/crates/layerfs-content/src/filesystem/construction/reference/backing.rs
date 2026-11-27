//! Caller-supplied ordering backing: owned run storage and checked cleanup.
//!
//! The reducer's runs live wherever the caller puts them. This module defines the
//! capability it needs (create a run, append, read at an offset) and one concrete
//! local implementation over ordinary files. Nothing here is free memory or free
//! disk: one explicit account owns the bytes, growth is reserved before it
//! happens, obsolete runs give their bytes back when they are dropped, and the
//! finishing cleanup is checked rather than hidden in a destructor.
//!
//! **Custody.** The account lists every run path that is still on disk with the
//! bytes that reached it, including the written part of an append that failed.
//! A path leaves the list only when its file is gone. A checked release that
//! fails keeps the paths it could not remove, records the deciding cause, and is
//! never replayed by a destructor: what remains is the caller's to dispose of.
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
    /// True once the checked release ran and left at least one path behind.
    release_failed: Cell<bool>,
    /// First removal that did not complete; later failures do not replace it.
    failure: RefCell<Option<CleanupFailure>>,
}

/// The first run removal one backing could not complete.
///
/// The path is still on disk and still counted by the backing's account. The
/// host cause is kept as its kind and raw code because the content error type
/// is comparable and cannot carry the host error itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupFailure {
    /// Run file the backing still owns.
    pub path: PathBuf,
    /// Bytes that reached that file.
    pub bytes: u64,
    /// Kind of the host failure that decided the cleanup.
    pub kind: std::io::ErrorKind,
    /// Raw host error code, when the host supplied one.
    pub os_error: Option<i32>,
}

impl CleanupFailure {
    /// The deciding host failure as an I/O error.
    pub fn to_io_error(&self) -> std::io::Error {
        match self.os_error {
            Some(code) => std::io::Error::from_raw_os_error(code),
            None => std::io::Error::from(self.kind),
        }
    }
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

    /// Forgets one removed path and gives back exactly the bytes listed for it.
    ///
    /// The list is the only source of the amount: a path that already left it
    /// returns nothing, so another path's retained bytes are never given back.
    fn released(&self, path: &Path) {
        let listed = self.paths.borrow_mut().remove(path);
        if let Some(bytes) = listed {
            self.release_bytes(bytes);
        }
    }

    /// True while `path` is still listed as owned.
    fn owns(&self, path: &Path) -> bool {
        self.paths.borrow().contains_key(path)
    }

    /// Adds `bytes` to the bytes one still-owned path holds.
    fn record(&self, path: &Path, bytes: u64) {
        if let Some(entry) = self.paths.borrow_mut().get_mut(path) {
            *entry = entry.saturating_add(bytes);
        }
    }

    /// Records one removal that did not complete; the first cause is kept.
    fn removal_failed(&self, path: &Path, bytes: u64, error: &std::io::Error) {
        self.cleanup_failed.set(true);
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            *failure = Some(CleanupFailure {
                path: path.to_path_buf(),
                bytes,
                kind: error.kind(),
                os_error: error.raw_os_error(),
            });
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
    /// True once the checked release ran, whatever it returned.
    release_attempted: bool,
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
                release_failed: Cell::new(false),
                failure: RefCell::new(None),
            }),
            released: false,
            release_attempted: false,
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

    /// Run files this backing still owns.
    pub fn owned_runs(&self) -> u64 {
        self.account.paths.borrow().len() as u64
    }

    /// The first run removal that did not complete, with its deciding cause.
    pub fn cleanup_failure(&self) -> Option<CleanupFailure> {
        self.account.failure.borrow().clone()
    }

    /// Removes every path the account still lists, reporting the first failure.
    ///
    /// A path whose file is gone leaves the account and gives its bytes back. A
    /// path that could not be removed stays listed and stays counted, so the
    /// accessors keep describing the storage this backing still owns.
    fn discard_paths(&self) -> ContentResult<()> {
        let paths: Vec<(PathBuf, u64)> = self
            .account
            .paths
            .borrow()
            .iter()
            .map(|(path, bytes)| (path.clone(), *bytes))
            .collect();
        let mut failed = false;
        for (path, bytes) in paths {
            match std::fs::remove_file(&path) {
                Ok(()) => self.account.released(&path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.account.released(&path);
                }
                Err(error) => {
                    failed = true;
                    self.account.removal_failed(&path, bytes, &error);
                }
            }
        }
        if failed {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering run cleanup",
            });
        }
        Ok(())
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
            failed: false,
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
        self.release_attempted = true;
        let outcome = self.discard_paths();
        match outcome {
            Ok(()) => self.released = true,
            Err(_) => self.account.release_failed.set(true),
        }
        outcome
    }
}

impl Drop for FileBacking {
    fn drop(&mut self) {
        // Ordinary cancellation path: an operation that never reached its
        // checked release has every run it still owns removed, and a failure is
        // recorded instead of being thrown away. A checked release that already
        // ran is final: what it could not remove stays owned and is not
        // attempted again here.
        if !self.release_attempted {
            let _ = self.discard_paths();
        }
    }
}

struct FileRun {
    file: std::fs::File,
    path: PathBuf,
    /// Bytes of complete appends; the run's logical length.
    written: u64,
    /// True once an append failed: the file may end in a partial record.
    failed: bool,
    account: Rc<Account>,
}

impl FileRun {
    /// Writes `bytes` in order and returns how many reached the file.
    fn write_counted(&mut self, bytes: &[u8]) -> usize {
        let mut done = 0;
        while done < bytes.len() {
            match self.file.write(&bytes[done..]) {
                Ok(0) => break,
                Ok(count) => done += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        done
    }
}

impl OrderingRun for FileRun {
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()> {
        if self.failed {
            // An earlier append left a partial record: offsets past it no longer
            // match the logical length, so the run takes no further rows.
            return Err(ContentError::Io);
        }
        if !self.account.owns(&self.path) {
            return Err(ContentError::ResourceUnavailable {
                what: "released ordering run",
            });
        }
        let length = bytes.len() as u64;
        self.account.reserve(length)?;
        let reached = self.write_counted(bytes) as u64;
        // Bytes that reached the file stay charged to its path whether or not
        // the append completed; only the part never written is given back.
        self.account.record(&self.path, reached);
        if reached < length {
            self.account.release_bytes(length - reached);
            self.failed = true;
            return Err(ContentError::Io);
        }
        self.written = self.written.saturating_add(length);
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
        // account stops counting it. A path the backing already removed has
        // nothing left to return, and a path a failed checked release kept is
        // retained as it is rather than attempted again.
        if !self.account.owns(&self.path) || self.account.release_failed.get() {
            return;
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => self.account.released(&self.path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.account.released(&self.path);
            }
            Err(error) => {
                // The file is still there, so it stays owned and stays counted.
                let bytes = self.account.paths.borrow().get(&self.path).copied();
                self.account
                    .removal_failed(&self.path, bytes.unwrap_or(0), &error);
            }
        }
    }
}
