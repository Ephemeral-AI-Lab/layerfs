//! The acquisition port over the Store's one session.
//!
//! Each unit is one transaction or single-statement read snapshot on the
//! session storage and history share. No unit opens, attaches or creates
//! anything, and contention is the existing one-attempt refusal.
use crate::backend::{
    records::BackendError,
    sqlite::acquisition::{accounting, cleanup, reads, statements::Failure, writes},
    Session, Transaction,
};
use crate::SqliteAcquisitionSchema;
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition::{
    Abandoned, Acquisition, AcquisitionResult, AcquisitionWork, Begin, Binding, Directory,
    Discarded, Disposal, Entry, EntryKey, FileRoot, Job, Limits, NewEntry, Owner, Phase, Placed,
    Unplaced, READ_WINDOW_ROWS, WRITE_ROW_BYTES, WRITE_WINDOW_BYTES, WRITE_WINDOW_ROWS,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Application-owned acquisition working state of one opened Store.
pub struct AcquisitionProvider {
    session: Arc<Session>,
    /// Identity of this session among all that ever began an operation: the
    /// identity of its own first acquired operation, zero until then.
    epoch: Mutex<u64>,
    directory: Option<PathBuf>,
}

impl AcquisitionProvider {
    pub(crate) fn new(session: Arc<Session>, directory: Option<PathBuf>) -> Self {
        Self {
            session,
            epoch: Mutex::new(0),
            directory,
        }
    }

    fn epoch(&self) -> Result<u64, Failure> {
        match self.epoch.try_lock() {
            Ok(epoch) => Ok(*epoch),
            Err(_) => Err(Failure::Backend(BackendError::Busy)),
        }
    }

    /// One transaction for a live operation of this session.
    ///
    /// The owner is matched against its stored row before the body runs, so a
    /// released or foreign operation is `Stale` before any row is read or
    /// written: the working rows' keys carry the operation but not the epoch.
    fn unit<T>(
        &self,
        owner: Owner,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, Failure>,
    ) -> AcquisitionResult<T> {
        self.unit_work(owner, writable, |tx, _| body(tx))
    }

    fn unit_work<T>(
        &self,
        owner: Owner,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>, AcquisitionWork) -> Result<T, Failure>,
    ) -> AcquisitionResult<T> {
        self.live(owner)?;
        Ok(self.session.run(writable, |tx| {
            let (_, work) = accounting::owner(tx, owner)?;
            body(tx, work)
        })?)
    }

    /// Session identity before a statement whose SQL validates the live owner.
    fn read<T>(
        &self,
        owner: Owner,
        body: impl FnOnce(&Session) -> Result<T, Failure>,
    ) -> AcquisitionResult<T> {
        self.live(owner)?;
        Ok(body(&self.session)?)
    }

    fn live(&self, owner: Owner) -> Result<(), Failure> {
        self.available()?;
        if owner.epoch == 0 || owner.epoch != self.epoch()? {
            return Err(Failure::Stale);
        }
        Ok(())
    }

    /// Refuses every unit on a Store created without the acquisition tables.
    fn available(&self) -> Result<(), Failure> {
        match self.session.acquisition {
            SqliteAcquisitionSchema::Tables => Ok(()),
            SqliteAcquisitionSchema::Absent => Err(Failure::Unavailable),
        }
    }
}

/// Refuses a write window above the port's row or payload-byte maximum.
fn bounded(rows: usize, bytes: usize) -> Result<(), Failure> {
    if rows > WRITE_WINDOW_ROWS || bytes > WRITE_WINDOW_BYTES {
        return Err(Failure::Bounds);
    }
    Ok(())
}

fn budget(rows: usize) -> i64 {
    rows.min(WRITE_WINDOW_ROWS) as i64
}

impl Acquisition for AcquisitionProvider {
    fn begin(&self, facts: &Begin) -> AcquisitionResult<Owner> {
        self.available()?;
        // Held across the transaction so two first operations cannot both
        // name the session's epoch; set only after the commit is known.
        let mut epoch = self
            .epoch
            .try_lock()
            .map_err(|_| Failure::Backend(BackendError::Busy))?;
        let known = *epoch;
        let owner = self
            .session
            .run(true, |tx| writes::begin(tx, known, facts))?;
        *epoch = owner.epoch;
        Ok(owner)
    }

    fn placement(&self) -> Option<PathBuf> {
        self.directory.clone()
    }

    fn put_entries(&self, owner: Owner, entries: &[NewEntry]) -> AcquisitionResult<Vec<u64>> {
        let bytes = entries.iter().map(|entry| {
            entry.key.name.len() + entry.native_path.as_ref().map_or(0, Vec::len) + WRITE_ROW_BYTES
        });
        bounded(entries.len(), bytes.sum())?;
        self.unit(owner, true, |tx| writes::put_entries(tx, owner, entries))
    }

    fn unplaced_children(
        &self,
        owner: Owner,
        parent: u64,
        after: Option<&[u8]>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Unplaced>> {
        self.read(owner, |session| {
            reads::unplaced_children(session, owner, parent, after, limits)
        })
    }

    fn place_children(
        &self,
        owner: Owner,
        parent: u64,
        placed: &[Placed],
    ) -> AcquisitionResult<Vec<u64>> {
        let bytes = placed.iter().map(|child| {
            child.name.len()
                + child.native.as_ref().map_or(0, |(_, path)| path.len())
                + WRITE_ROW_BYTES
        });
        bounded(placed.len(), bytes.sum())?;
        self.unit(owner, true, |tx| {
            writes::place_children(tx, owner, parent, placed)
        })
    }

    fn directories(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Directory>> {
        self.unit(owner, false, |tx| {
            reads::directories(tx, owner, after, limits)
        })
    }

    fn directory_path(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Vec<u8>>> {
        self.read(owner, |session| {
            reads::directory_path(session, owner, position)
        })
    }

    fn jobs(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Job>> {
        self.unit(owner, false, |tx| reads::jobs(tx, owner, after, limits))
    }

    fn job(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Job>> {
        self.read(owner, |session| reads::job(session, owner, position))
    }

    fn complete_files(&self, owner: Owner, roots: &[(u64, ObjectId)]) -> AcquisitionResult<()> {
        bounded(roots.len(), roots.len() * 48)?;
        self.unit(owner, true, |tx| writes::complete_files(tx, owner, roots))
    }

    fn file_roots(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<FileRoot>> {
        self.read(owner, |session| {
            reads::file_roots(session, owner, after, limits)
        })
    }

    fn entries(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Entry>> {
        self.read(owner, |session| {
            reads::entries(session, owner, after, limits)
        })
    }

    fn bindings(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Binding>> {
        self.read(owner, |session| {
            reads::bindings(session, owner, after, limits)
        })
    }

    fn set_directory_roots(
        &self,
        owner: Owner,
        roots: &[(u64, ObjectId)],
    ) -> AcquisitionResult<()> {
        bounded(roots.len(), roots.len() * 48)?;
        self.unit(owner, true, |tx| {
            writes::set_directory_roots(tx, owner, roots)
        })
    }

    fn advance(&self, owner: Owner, phase: Phase) -> AcquisitionResult<()> {
        self.unit(owner, true, |tx| writes::advance(tx, owner, phase))
    }

    fn discard(&self, owner: Owner, rows: usize) -> AcquisitionResult<Discarded> {
        self.unit(owner, true, |tx| {
            cleanup::discard_validated(tx, owner, budget(rows))
        })
    }

    fn dispose(&self, owner: Owner, rows: usize) -> AcquisitionResult<Disposal> {
        self.unit_work(owner, true, |tx, _| {
            cleanup::dispose_validated(tx, owner, budget(rows))
        })
    }

    fn release(&self, owner: Owner) -> AcquisitionResult<()> {
        self.unit_work(owner, true, |tx, work| {
            cleanup::release_validated(tx, owner, work)
        })
    }

    fn work(&self, owner: Owner) -> AcquisitionResult<AcquisitionWork> {
        self.unit_work(owner, false, |_, work| Ok(work))
    }

    fn abandoned(&self, after: Option<u64>, limit: usize) -> AcquisitionResult<Vec<Abandoned>> {
        self.available()?;
        let current = self.epoch()?;
        let limit = limit.clamp(1, READ_WINDOW_ROWS) as i64;
        Ok(self
            .session
            .run(false, |tx| cleanup::abandoned(tx, current, after, limit))?)
    }

    fn discard_abandoned(&self, abandoned: Owner, rows: usize) -> AcquisitionResult<Discarded> {
        self.available()?;
        // A live operation of this session is never abandoned.
        if abandoned.epoch == self.epoch()? {
            return Err(Failure::Stale.into());
        }
        Ok(self.session.run(true, |tx| {
            cleanup::discard_abandoned(tx, abandoned, budget(rows))
        })?)
    }
}
