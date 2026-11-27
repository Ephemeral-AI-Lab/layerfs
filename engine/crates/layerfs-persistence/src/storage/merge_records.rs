//! Merge build records (#355): one merge's working state in the Store file.
//!
//! A merge registers an owner row with a random token; its records carry the
//! owner's number. Every job checks the owner and token in the same statement
//! or transaction, so records of a removed owner are never read as absent.
//! A merge's records start resident: while they take at most
//! [`RESIDENT_BYTES`], every record is in memory and none is in the file, so
//! reads, guards and key windows run no statement (#364). Past that bound
//! they are written to the file in one write transaction, and from then on
//! guarded batches apply to a write-back buffer of at most [`BUFFER_BYTES`];
//! a full buffer, and every key enumeration, flushes it in one write
//! transaction, so writes are few short jobs through the Store's one writer.
//! Reads answer from the buffer first, and a key window from the file in one
//! statement. Nothing here takes the publication lane. The merge removes its records when it ends, a window
//! at a time. A writable open removes the records of owners that made no call
//! for [`MERGE_LEASE`]: a live merge refreshes its owner well within it.
//!
//! The statements are each engine's: the SQLite Store file's `merge_*` tables
//! (`backend/sqlite/merge_records.rs`), or on a PostgreSQL Store (#506 d)
//! temporary tables of the writer's connection
//! (`backend/postgresql/merge_records.rs`), which the connection's end removes.

use crate::backend::{postgresql, sqlite};
use layerfs_content::{
    ContentError, ContentResult, EditRecordApply, EditRecordChange, EditRecordExpected,
    EditRecordKey, IndexedEditBacking,
};
use layerfs_storage::port::PersistenceError;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Idle time after which a writable open removes a merge's records.
pub const MERGE_LEASE: Duration = Duration::from_secs(600);
/// A live merge refreshes its owner after this much time since the last.
const REFRESH: Duration = Duration::from_secs(60);
/// Keys one window or batch holds.
pub(crate) const WINDOW: usize = 64;
/// Value bytes one batch may write.
const BATCH_BYTES: usize = 64 * 1024;
/// Bytes of buffered changes, keys and values, after which a batch flushes.
/// It bounds both the buffer's memory and one flush's writer hold.
pub const BUFFER_BYTES: usize = 256 * 1024;
/// Bytes of records, keys and values, a merge keeps resident before it writes
/// them to the file: the bound a merge's own held state has (#355).
pub const RESIDENT_BYTES: usize = 8 << 20;
/// Buffer bytes one change is charged besides its value.
const CHANGE_BYTES: usize = 48;

/// Work of one merge's records: jobs, and how long its write transactions
/// held the Store's writer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MergeRecordsWork {
    /// Owner-checked read statements.
    pub reads: u64,
    /// Keys asked by reads, answered from the buffer or the file.
    pub keys_read: u64,
    /// Of those, keys the buffer answered.
    pub buffer_hits: u64,
    /// Guarded batches.
    pub batches: u64,
    /// Changes those batches carried.
    pub changes: u64,
    /// Buffer flushes (each one write transaction).
    pub flushes: u64,
    /// Write transactions: registration, batches, refreshes and removal.
    pub writes: u64,
    /// Total writer hold of those transactions.
    pub write_ns: u64,
    /// Longest single writer hold.
    pub max_write_ns: u64,
}

impl MergeRecordsWork {
    fn write(&mut self, start: Instant) {
        let held = start.elapsed().as_nanos() as u64;
        self.writes += 1;
        self.write_ns += held;
        self.max_write_ns = self.max_write_ns.max(held);
    }

    /// Adds another merge's work.
    pub fn add(&mut self, other: Self) {
        self.reads += other.reads;
        self.keys_read += other.keys_read;
        self.buffer_hits += other.buffer_hits;
        self.batches += other.batches;
        self.changes += other.changes;
        self.flushes += other.flushes;
        self.writes += other.writes;
        self.write_ns += other.write_ns;
        self.max_write_ns = self.max_write_ns.max(other.max_write_ns);
    }
}

/// The writable session a merge's records live in.
#[derive(Clone)]
pub(crate) enum Scratch {
    /// The SQLite Store file's `merge_*` tables.
    Sqlite(Arc<sqlite::session::connection::Session>),
    /// A PostgreSQL writer connection's temporary tables (#506 d).
    Postgres(Arc<postgresql::session::connection::Session>),
}

/// A key window: keys after a bound, or every key but one.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Window {
    After,
    Excluding,
}

/// Opens merge records over the Store's writable session.
pub struct MergeScratch {
    pub(crate) session: Scratch,
}

impl MergeScratch {
    /// Registers one merge and returns its records.
    pub fn begin(&self) -> Result<MergeRecords, PersistenceError> {
        let start = Instant::now();
        let (operation, token) = self.session.register()?;
        let mut work = MergeRecordsWork::default();
        work.write(start);
        Ok(MergeRecords {
            session: self.session.clone(),
            operation,
            token,
            refreshed: Instant::now(),
            failure: None,
            work,
            pending: BTreeMap::new(),
            pending_bytes: 0,
            resident: true,
        })
    }
}

/// One merge's records. A first failure ends every later call; the caller
/// reads it with [`MergeRecords::failure`] and calls [`MergeRecords::end`].
pub struct MergeRecords {
    session: Scratch,
    operation: i64,
    token: Vec<u8>,
    refreshed: Instant,
    failure: Option<PersistenceError>,
    work: MergeRecordsWork,
    /// Changes not yet in the file: a value, or `None` for a deletion.
    pending: BTreeMap<EditRecordKey, Option<Vec<u8>>>,
    pending_bytes: usize,
    /// True while no record was written to the file: `pending` then holds
    /// every record, and a key it lacks is absent.
    resident: bool,
}

impl MergeRecords {
    /// The first failure of a call, when one failed.
    pub fn failure(&self) -> Option<&PersistenceError> {
        self.failure.as_ref()
    }

    /// Jobs so far and their writer hold.
    pub fn work(&self) -> MergeRecordsWork {
        self.work
    }

    /// Removes every record of this merge, then its owner, a window per job.
    /// Returns the merge's work including the removal.
    pub fn end(mut self) -> Result<MergeRecordsWork, PersistenceError> {
        self.pending.clear();
        loop {
            let start = Instant::now();
            let removed = self.session.remove(self.operation, &self.token)?;
            self.work.write(start);
            if removed == 0 {
                return Ok(self.work);
            }
        }
    }

    /// Runs one job, ending this merge's calls at its first failure.
    fn job<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, PersistenceError>,
    ) -> ContentResult<T> {
        if self.failure.is_some() {
            return Err(refused());
        }
        let refreshed = if self.refreshed.elapsed() >= REFRESH {
            self.refresh()
        } else {
            Ok(())
        };
        let result = refreshed.and_then(|()| body(self));
        result.map_err(|error| {
            self.failure = Some(error);
            refused()
        })
    }

    fn refresh(&mut self) -> Result<(), PersistenceError> {
        let start = Instant::now();
        self.session.refresh(self.operation, &self.token)?;
        self.work.write(start);
        self.refreshed = Instant::now();
        Ok(())
    }

    /// Values in the file of at most [`WINDOW`] keys, in the order asked, in
    /// one owner-checked statement.
    fn stored(&mut self, keys: &[EditRecordKey]) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let found = self.session.stored(self.operation, &self.token, keys)?;
        self.work.reads += 1;
        if found.len() != keys.len() {
            return Err(if found.is_empty() {
                PersistenceError::Missing
            } else {
                PersistenceError::Malformed
            });
        }
        Ok(found)
    }

    /// Values of at most [`WINDOW`] keys: the buffer's, else the file's.
    fn values(&mut self, keys: &[EditRecordKey]) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
        let mut out = vec![None; keys.len()];
        let mut missed = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            match self.pending.get(key) {
                Some(value) => out[index] = value.clone(),
                None => missed.push(index),
            }
        }
        self.work.keys_read += keys.len() as u64;
        self.work.buffer_hits += (keys.len() - missed.len()) as u64;
        if self.resident {
            return Ok(out);
        }
        let asked: Vec<EditRecordKey> = missed.iter().map(|index| keys[*index]).collect();
        for (index, value) in missed.into_iter().zip(self.stored(&asked)?) {
            out[index] = value;
        }
        Ok(out)
    }

    fn keys(
        &mut self,
        window: Window,
        kind: u32,
        bound: &[u8],
    ) -> Result<Vec<[u8; 32]>, PersistenceError> {
        if self.resident {
            // The window the statement would answer, from the records held.
            let first = EditRecordKey { kind, key: [0; 32] };
            return Ok(self
                .pending
                .range(first..)
                .take_while(|(key, _)| key.kind == kind)
                .map(|(key, _)| key.key)
                .filter(|key| match window {
                    Window::Excluding => key.as_slice() != bound,
                    Window::After => key.as_slice() > bound,
                })
                .take(WINDOW)
                .collect());
        }
        self.flush()?;
        let rows = self
            .session
            .keys(self.operation, &self.token, window, kind, bound)?;
        self.work.reads += 1;
        if rows.is_empty() {
            return Err(PersistenceError::Missing);
        }
        rows.into_iter()
            .flatten()
            .map(|key| <[u8; 32]>::try_from(key).map_err(|_| PersistenceError::Malformed))
            .collect()
    }

    /// Checks every guard against the buffer and the file, then applies the
    /// batch to the buffer; flushes once the buffer is full.
    fn apply_buffered(
        &mut self,
        changes: Vec<EditRecordChange>,
    ) -> Result<EditRecordApply, PersistenceError> {
        self.work.batches += 1;
        self.work.changes += changes.len() as u64;
        let keys: Vec<EditRecordKey> = changes.iter().map(|change| change.key).collect();
        let mut actual = Vec::with_capacity(keys.len());
        for window in keys.chunks(WINDOW) {
            actual.extend(self.values(window)?);
        }
        for (index, (change, actual)) in changes.iter().zip(actual).enumerate() {
            let matches = match (&change.expected, &actual) {
                (EditRecordExpected::Missing, None) => true,
                (EditRecordExpected::ExactBytes(expected), Some(actual)) => expected == actual,
                _ => false,
            };
            if !matches {
                return Ok(EditRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual,
                });
            }
        }
        for change in changes {
            let bytes = CHANGE_BYTES + change.value.as_ref().map_or(0, Vec::len);
            let old = if self.resident && change.value.is_none() {
                // Nothing is in the file to delete: the record just goes.
                self.pending.remove(&change.key)
            } else {
                self.pending_bytes += bytes;
                self.pending.insert(change.key, change.value)
            };
            if let Some(old) = old {
                self.pending_bytes -= CHANGE_BYTES + old.map_or(0, |value| value.len());
            }
        }
        if self.pending_bytes
            >= if self.resident {
                RESIDENT_BYTES
            } else {
                BUFFER_BYTES
            }
        {
            self.resident = false;
            self.flush()?;
        }
        Ok(EditRecordApply::Applied)
    }

    /// Writes the buffer to the file in one owner-checked write transaction.
    fn flush(&mut self) -> Result<(), PersistenceError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let start = Instant::now();
        let pending = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        self.session.flush(self.operation, &self.token, &pending)?;
        self.work.write(start);
        self.work.flushes += 1;
        Ok(())
    }
}

impl Scratch {
    /// Registers one merge's owner: `(operation, token)`.
    fn register(&self) -> Result<(i64, Vec<u8>), PersistenceError> {
        match self {
            Self::Sqlite(session) => sqlite::merge_records::register(session),
            Self::Postgres(session) => postgresql::merge_records::register(session),
        }
    }

    /// Removes one window of the merge's records, then, once none is left,
    /// its owner: the records removed (0 once the owner went).
    fn remove(&self, operation: i64, token: &[u8]) -> Result<i64, PersistenceError> {
        match self {
            Self::Sqlite(session) => sqlite::merge_records::remove(session, operation, token),
            Self::Postgres(session) => postgresql::merge_records::remove(session, operation, token),
        }
    }

    /// Marks the owner live now.
    fn refresh(&self, operation: i64, token: &[u8]) -> Result<(), PersistenceError> {
        match self {
            Self::Sqlite(session) => sqlite::merge_records::refresh(session, operation, token),
            Self::Postgres(session) => {
                postgresql::merge_records::refresh(session, operation, token)
            }
        }
    }

    /// One owner-checked key window: no rows means the owner is gone, and a
    /// NULL row means no keys.
    fn keys(
        &self,
        operation: i64,
        token: &[u8],
        window: Window,
        kind: u32,
        bound: &[u8],
    ) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
        match self {
            Self::Sqlite(session) => {
                sqlite::merge_records::keys(session, operation, token, window, kind, bound)
            }
            Self::Postgres(session) => {
                postgresql::merge_records::keys(session, operation, token, window, kind, bound)
            }
        }
    }

    /// The stored values of `keys`, in order, in one owner-checked
    /// statement; no rows means the owner is gone.
    fn stored(
        &self,
        operation: i64,
        token: &[u8],
        keys: &[EditRecordKey],
    ) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
        match self {
            Self::Sqlite(session) => sqlite::merge_records::stored(session, operation, token, keys),
            Self::Postgres(session) => {
                postgresql::merge_records::stored(session, operation, token, keys)
            }
        }
    }

    /// Writes buffered changes in one owner-checked write transaction.
    fn flush(
        &self,
        operation: i64,
        token: &[u8],
        pending: &BTreeMap<EditRecordKey, Option<Vec<u8>>>,
    ) -> Result<(), PersistenceError> {
        match self {
            Self::Sqlite(session) => {
                sqlite::merge_records::flush(session, operation, token, pending)
            }
            Self::Postgres(session) => {
                postgresql::merge_records::flush(session, operation, token, pending)
            }
        }
    }
}

fn refused() -> ContentError {
    ContentError::ProviderFailure {
        what: "merge records",
    }
}

impl IndexedEditBacking for MergeRecords {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.job(|records| Ok(records.values(&[key])?[0].is_some()))
    }

    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.job(|records| Ok(records.values(&[key])?.remove(0)))
    }

    fn get_many(&mut self, keys: &[EditRecordKey]) -> ContentResult<Vec<Option<Vec<u8>>>> {
        if keys.len() > WINDOW {
            return Err(ContentError::ProviderFailure {
                what: "merge record window",
            });
        }
        self.job(|records| records.values(keys))
    }

    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        let sorted = changes.windows(2).all(|pair| pair[0].key < pair[1].key);
        let bytes: usize = changes
            .iter()
            .map(|change| change.value.as_ref().map_or(0, Vec::len))
            .sum();
        if !sorted || bytes > BATCH_BYTES {
            return Err(ContentError::ProviderFailure {
                what: "merge record batch",
            });
        }
        self.job(|records| records.apply_buffered(changes))
    }

    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.job(|records| match excluded {
            Some(root) => records.keys(Window::Excluding, kind, &root),
            None => records.keys(Window::After, kind, &[]),
        })
    }

    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        self.job(|records| match after {
            Some(after) => records.keys(Window::After, kind, &after),
            None => records.keys(Window::After, kind, &[]),
        })
    }
}
