//! One live acquisition operation: bounded windows, native evidence and custody.
//!
//! Every input-sized row of an Init lives in the provider behind the
//! acquisition port. This module holds the resident side only: one read window
//! per stream, one write window, and the counters that describe them. Each
//! port call is one attempt; nothing here repeats a unit that failed.
use crate::error::{acquisition, malformed, ProjectError as Failure};
use crate::{NamespaceWork, RetainedAcquisition};
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition::{
    Acquisition, AcquisitionError, AcquisitionResult, Directory, Entry, EntryKey, FileRoot, Job,
    Limits, NativeIdentity, NewEntry, Owner, Phase, Placed, Unplaced, EVIDENCE_BYTES,
    WRITE_ROW_BYTES, WRITE_WINDOW_BYTES, WRITE_WINDOW_ROWS,
};
use layerfs_storage::port::PersistenceError;
use std::{cell::Cell, collections::VecDeque, fs::Metadata, os::unix::fs::MetadataExt};

/// Rows the job queue or one resident child buffer may hold.
pub(crate) const WINDOW_ROWS: usize = 512;

/// Native identity and change evidence observed for one source inode.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Stamp {
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub mode: u32,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
}
impl Stamp {
    pub(crate) fn of(metadata: &Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            mode: metadata.mode(),
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_nsec: metadata.ctime_nsec(),
        }
    }
    /// True while `metadata` is still the same unchanged regular file.
    pub(crate) fn matches_file(&self, metadata: &Metadata) -> bool {
        metadata.is_file() && !metadata.file_type().is_symlink() && *self == Self::of(metadata)
    }
    /// The identity and byte-exact evidence every path of this inode must share.
    pub(crate) fn identity(&self) -> NativeIdentity {
        let mut evidence = [0; EVIDENCE_BYTES];
        evidence[..8].copy_from_slice(&self.len.to_be_bytes());
        evidence[8..12].copy_from_slice(&self.mode.to_be_bytes());
        let times = [self.mtime, self.mtime_nsec, self.ctime, self.ctime_nsec];
        for (slot, value) in evidence[12..].chunks_exact_mut(8).zip(times) {
            slot.copy_from_slice(&value.to_be_bytes());
        }
        NativeIdentity {
            device: self.dev,
            inode: self.ino,
            evidence,
        }
    }
    pub(crate) fn from_identity(native: &NativeIdentity) -> Self {
        let field = |at: usize| {
            let mut value = [0; 8];
            value.copy_from_slice(&native.evidence[at..at + 8]);
            value
        };
        let mut mode = [0; 4];
        mode.copy_from_slice(&native.evidence[8..12]);
        Self {
            dev: native.device,
            ino: native.inode,
            len: u64::from_be_bytes(field(0)),
            mode: u32::from_be_bytes(mode),
            mtime: i64::from_be_bytes(field(12)),
            mtime_nsec: i64::from_be_bytes(field(20)),
            ctime: i64::from_be_bytes(field(28)),
            ctime_nsec: i64::from_be_bytes(field(36)),
        }
    }
}

/// Native regular-file paths bound so far, by whether they named a new identity.
#[derive(Clone, Copy, Default)]
pub(crate) struct Bound {
    pub unique: usize,
    pub aliases: usize,
}
impl Bound {
    /// Counts each path against the canonical position its identity returned.
    fn count(
        &mut self,
        positions: impl Iterator<Item = u64>,
        canonical: &[u64],
    ) -> Result<(), Failure> {
        let mut bound = canonical.iter();
        for position in positions {
            match bound.next() {
                Some(first) if *first == position => self.unique += 1,
                Some(_) => self.aliases += 1,
                None => return Err(malformed()),
            }
        }
        match bound.next() {
            None => Ok(()),
            Some(_) => Err(malformed()),
        }
    }
}

/// The port, this operation's owner and the unit counters of its windows.
pub(crate) struct Backing<'a> {
    port: &'a dyn Acquisition,
    owner: Owner,
    read_units: Cell<u64>,
    write_units: Cell<u64>,
    read_rows: Cell<usize>,
    write_rows: Cell<usize>,
    write_bytes: Cell<usize>,
}
impl<'a> Backing<'a> {
    pub(crate) fn new(port: &'a dyn Acquisition, owner: Owner) -> Self {
        Self {
            port,
            owner,
            read_units: Cell::new(0),
            write_units: Cell::new(0),
            read_rows: Cell::new(0),
            write_rows: Cell::new(0),
            write_bytes: Cell::new(0),
        }
    }
    fn read<T>(&self, rows: AcquisitionResult<Vec<T>>) -> Result<Vec<T>, Failure> {
        self.read_units.set(self.read_units.get() + 1);
        let rows = rows.map_err(acquisition)?;
        self.read_rows.set(self.read_rows.get().max(rows.len()));
        Ok(rows)
    }
    fn wrote(&self, rows: usize, bytes: usize) {
        self.write_units.set(self.write_units.get() + 1);
        self.write_rows.set(self.write_rows.get().max(rows));
        self.write_bytes.set(self.write_bytes.get().max(bytes));
    }
    /// Every entry in acquisition order.
    pub(crate) fn entries(&self) -> Stream<'_, 'a, Entry, EntryKey> {
        Stream::new(
            self,
            |backing, after| {
                let port = backing.port;
                backing.read(port.entries(backing.owner, after, Limits::MAXIMUM))
            },
            |entry| entry.key.clone(),
        )
    }
    /// Every directory in position order, including ones placed after a window.
    pub(crate) fn directories(&self) -> Stream<'_, 'a, Directory, u64> {
        Stream::new(
            self,
            |backing, after| {
                let port = backing.port;
                backing.read(port.directories(backing.owner, after.copied(), Limits::MAXIMUM))
            },
            |directory| directory.position,
        )
    }
    /// The first path of every native identity in position order.
    pub(crate) fn jobs(&self) -> Stream<'_, 'a, Job, u64> {
        Stream::new(
            self,
            |backing, after| {
                let port = backing.port;
                backing.read(port.jobs(backing.owner, after.copied(), Limits::MAXIMUM))
            },
            |job| job.position,
        )
    }
    /// Every native identity's constructed root and later-path count.
    pub(crate) fn file_roots(&self) -> Stream<'_, 'a, FileRoot, u64> {
        Stream::new(
            self,
            |backing, after| {
                let port = backing.port;
                backing.read(port.file_roots(backing.owner, after.copied(), Limits::MAXIMUM))
            },
            |root| root.position,
        )
    }
    /// One name-ordered window of a wide directory's unpositioned children.
    pub(crate) fn unplaced(
        &self,
        parent: u64,
        after: Option<&[u8]>,
    ) -> Result<Vec<Unplaced>, Failure> {
        let limits = Limits::MAXIMUM;
        self.read(
            self.port
                .unplaced_children(self.owner, parent, after, limits),
        )
    }
    /// Positions one window of children. A later path whose evidence differs
    /// from its identity's first is a source that changed during the scan.
    pub(crate) fn place(
        &self,
        parent: u64,
        placed: &mut Vec<Placed>,
        bound: &mut Bound,
    ) -> Result<(), Failure> {
        if placed.is_empty() {
            return Ok(());
        }
        let bytes = placed.iter().map(placed_bytes).sum();
        self.wrote(placed.len(), bytes);
        let canonical = self
            .port
            .place_children(self.owner, parent, placed)
            .map_err(changed_source)?;
        let regular = placed.iter().filter(|child| child.native.is_some());
        bound.count(regular.map(|child| child.position), &canonical)?;
        placed.clear();
        Ok(())
    }
    pub(crate) fn directory_path(&self, position: u64) -> Result<Vec<u8>, Failure> {
        self.read_units.set(self.read_units.get() + 1);
        let path = self.port.directory_path(self.owner, position);
        path.map_err(acquisition)?.ok_or_else(malformed)
    }
    pub(crate) fn job(&self, position: u64) -> Result<Job, Failure> {
        self.read_units.set(self.read_units.get() + 1);
        let job = self.port.job(self.owner, position);
        job.map_err(acquisition)?.ok_or_else(malformed)
    }
    /// Records constructed file roots and empties the window.
    pub(crate) fn complete_files(&self, roots: &mut Vec<(u64, ObjectId)>) -> Result<(), Failure> {
        if !roots.is_empty() {
            self.wrote(roots.len(), roots.len() * ROOT_ROW_BYTES);
            let recorded = self.port.complete_files(self.owner, roots);
            recorded.map_err(acquisition)?;
            roots.clear();
        }
        Ok(())
    }
    /// Records built directory roots and empties the window.
    pub(crate) fn set_directory_roots(
        &self,
        roots: &mut Vec<(u64, ObjectId)>,
    ) -> Result<(), Failure> {
        if !roots.is_empty() {
            self.wrote(roots.len(), roots.len() * ROOT_ROW_BYTES);
            let recorded = self.port.set_directory_roots(self.owner, roots);
            recorded.map_err(acquisition)?;
            roots.clear();
        }
        Ok(())
    }
    /// Removes every working row in budgeted jobs, then the operation record.
    /// The first unit that fails decides; nothing after it is attempted.
    pub(crate) fn finish(&self, work: &mut NamespaceWork) -> Result<(), RetainedCleanup> {
        self.counters(work);
        let removed = self
            .discard(work)
            .and_then(|()| self.port.release(self.owner));
        removed.map_err(|error| self.retained(error))
    }
    fn discard(&self, work: &mut NamespaceWork) -> AcquisitionResult<()> {
        loop {
            let removed = self.port.discard(self.owner, WRITE_WINDOW_ROWS)?;
            work.backing_rows += removed.rows;
            work.backing_bytes += removed.bytes;
            work.discard_units += 1;
            if removed.remaining_rows == 0 {
                return Ok(());
            }
            if removed.rows == 0 {
                // Rows remain that no job removes: the charge and the tables disagree.
                return Err(PersistenceError::Malformed.into());
            }
        }
    }
    /// Settles a failed construction. An unknown outcome leaves everything in
    /// place; a definite failure records itself and removes its working rows.
    pub(crate) fn settle(&self, error: Failure, work: &mut NamespaceWork) -> Failure {
        if matches!(error, Failure::Cleanup { .. } | Failure::Uncertain { .. }) {
            return error;
        }
        if error.unknown_outcome() {
            return Failure::Uncertain {
                cause: Box::new(error),
                retained: RetainedAcquisition {
                    owner: self.owner,
                    work: None,
                },
            };
        }
        let failed = self.port.advance(self.owner, Phase::Failed);
        match failed.map_err(|error| self.retained(error)) {
            Ok(()) => match self.finish(work) {
                Ok(()) => error,
                Err(cleanup) => cleanup.after(Some(error)),
            },
            Err(cleanup) => cleanup.after(Some(error)),
        }
    }
    fn retained(&self, error: AcquisitionError) -> RetainedCleanup {
        RetainedCleanup {
            error,
            retained: RetainedAcquisition {
                owner: self.owner,
                // One read of the charges for the report; its refusal is not an error here.
                work: self.port.work(self.owner).ok(),
            },
        }
    }
    fn counters(&self, work: &mut NamespaceWork) {
        work.read_units = self.read_units.get();
        work.write_units = self.write_units.get();
        work.read_window_rows = self.read_rows.get();
        work.write_window_rows = self.write_rows.get();
        work.write_window_bytes = self.write_bytes.get();
    }
}

/// A cleanup unit that failed, with what the operation still owns.
pub(crate) struct RetainedCleanup {
    error: AcquisitionError,
    retained: RetainedAcquisition,
}
impl RetainedCleanup {
    pub(crate) fn after(self, cause: Option<Failure>) -> Failure {
        Failure::Cleanup {
            cause: cause.map(Box::new),
            error: self.error,
            retained: self.retained,
        }
    }
}

const ROOT_ROW_BYTES: usize = 48;

fn entry_bytes(entry: &NewEntry) -> usize {
    entry.key.name.len() + entry.native_path.as_ref().map_or(0, Vec::len) + WRITE_ROW_BYTES
}
pub(crate) fn placed_bytes(child: &Placed) -> usize {
    child.name.len() + child.native.as_ref().map_or(0, |(_, path)| path.len()) + WRITE_ROW_BYTES
}
/// True when adding `bytes` to a window of `rows` and `held` would exceed it.
pub(crate) fn window_full(rows: usize, held: usize, bytes: usize) -> bool {
    rows == WRITE_WINDOW_ROWS || (rows > 0 && held + bytes > WRITE_WINDOW_BYTES)
}

/// A later path of one identity carried different evidence: the source changed.
fn changed_source(error: AcquisitionError) -> Failure {
    match error {
        AcquisitionError::Changed { .. } => Failure::InvalidInput,
        error => acquisition(error),
    }
}

/// The one resident write window of observed entries.
#[derive(Default)]
pub(crate) struct Pending {
    entries: Vec<NewEntry>,
    bytes: usize,
}
impl Pending {
    /// Adds one entry, writing the window first when it is full.
    pub(crate) fn push(
        &mut self,
        backing: &Backing<'_>,
        entry: NewEntry,
        bound: &mut Bound,
    ) -> Result<(), Failure> {
        let bytes = entry_bytes(&entry);
        if window_full(self.entries.len(), self.bytes, bytes) {
            self.flush(backing, bound)?;
        }
        self.bytes += bytes;
        self.entries.push(entry);
        Ok(())
    }
    /// Writes the window as one unit and counts the identities it bound.
    pub(crate) fn flush(
        &mut self,
        backing: &Backing<'_>,
        bound: &mut Bound,
    ) -> Result<(), Failure> {
        if self.entries.is_empty() {
            return Ok(());
        }
        backing.wrote(self.entries.len(), self.bytes);
        let canonical = backing
            .port
            .put_entries(backing.owner, &self.entries)
            .map_err(changed_source)?;
        let regular = self.entries.iter().filter(|entry| entry.native.is_some());
        bound.count(regular.filter_map(|entry| entry.position), &canonical)?;
        self.entries.clear();
        self.bytes = 0;
        Ok(())
    }
}

type Fetch<'a, T, K> = fn(&Backing<'a>, Option<&K>) -> Result<Vec<T>, Failure>;

/// One keyset stream: a single resident window, refilled after its last key.
pub(crate) struct Stream<'b, 'a, T, K> {
    backing: &'b Backing<'a>,
    fetch: Fetch<'a, T, K>,
    key: fn(&T) -> K,
    after: Option<K>,
    window: VecDeque<T>,
    ended: bool,
}
impl<'b, 'a, T, K> Stream<'b, 'a, T, K> {
    fn new(backing: &'b Backing<'a>, fetch: Fetch<'a, T, K>, key: fn(&T) -> K) -> Self {
        Self {
            backing,
            fetch,
            key,
            after: None,
            window: VecDeque::new(),
            ended: false,
        }
    }
    /// True when the next row needs a read unit, so writes must be visible first.
    pub(crate) fn exhausted_window(&self) -> bool {
        self.window.is_empty() && !self.ended
    }
    fn fill(&mut self) -> Result<(), Failure> {
        if self.exhausted_window() {
            let rows = (self.fetch)(self.backing, self.after.as_ref())?;
            match rows.last() {
                Some(last) => self.after = Some((self.key)(last)),
                None => self.ended = true,
            }
            self.window = rows.into();
        }
        Ok(())
    }
    pub(crate) fn peek(&mut self) -> Result<Option<&T>, Failure> {
        self.fill()?;
        Ok(self.window.front())
    }
    pub(crate) fn next(&mut self) -> Result<Option<T>, Failure> {
        self.fill()?;
        Ok(self.window.pop_front())
    }
}
