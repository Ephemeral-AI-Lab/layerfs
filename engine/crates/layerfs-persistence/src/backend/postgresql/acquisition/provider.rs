//! The acquisition port over a PostgreSQL Store's one session.
//!
//! Each write unit is one lane-ordered write transaction on the session
//! storage and history share; the live owner is matched against its row
//! first in the same transaction, so a released or foreign operation is
//! `Stale` before any row is written. Each read unit is one fenced
//! autocommit statement (#818 I3a): one snapshot, with the owner's row
//! joined in, so a released or foreign operation reads no row and is
//! `Stale` too (`reads`). A write unit with no wait (root windows,
//! `advance`, `discard`, `dispose`) is one pipelined message from `BEGIN`
//! to `COMMIT` when the session is eligible (`flights`, #818 I3b); a unit
//! that waits (`put_entries`, `place_children`) is one transaction over
//! one flight per wait (`unit`, I3b part A). Nothing is created, and
//! contention is the one-attempt refusal.
use super::{cleanup, flights, placement, reads, unit, writes};
use crate::backend::{
    postgresql::session::{connection::Session, transaction::Transaction},
    sqlite::acquisition::statements::{signed, Failure},
};
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition::{
    Abandoned, Acquisition, AcquisitionResult, AcquisitionWork, Begin, Binding, Directory,
    Discarded, Disposal, Entry, EntryKey, FileRoot, Job, Limits, NewEntry, Owner, Phase, Placed,
    Unplaced, READ_WINDOW_ROWS, WRITE_ROW_BYTES, WRITE_WINDOW_BYTES, WRITE_WINDOW_ROWS,
};
use std::{path::PathBuf, sync::Arc};

/// Acquisition working state of one opened PostgreSQL Store.
pub struct PostgresAcquisition {
    session: Arc<Session>,
    /// Identity of this session among all that ever began an operation: the
    /// identity of its own first acquired operation, zero until then.
    epoch: crate::backend::epoch::Epoch,
    /// The schema has the acquisition tables.
    tables: bool,
}

impl PostgresAcquisition {
    pub(crate) fn new(session: Arc<Session>, tables: bool) -> Self {
        Self {
            session,
            epoch: Default::default(),
            tables,
        }
    }

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
        self.available()?;
        if owner.epoch == 0 || owner.epoch != self.epoch.get() {
            return Err(Failure::Stale.into());
        }
        Ok(self.session.run(writable, |tx| {
            let (_, work) = writes::owner(tx, owner)?;
            body(tx, work)
        })?)
    }

    /// A write unit with no wait: one flight (`flights`) when this session
    /// is eligible and `flight` takes the unit, else today's transaction.
    fn flight<T>(
        &self,
        owner: Owner,
        flight: impl FnOnce(&Session, flights::Identity) -> Option<Result<T, Failure>>,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, Failure>,
    ) -> AcquisitionResult<T> {
        self.available()?;
        if owner.epoch == 0 || owner.epoch != self.epoch.get() {
            return Err(Failure::Stale.into());
        }
        if flights::eligible(&self.session).map_err(Failure::from)? {
            let identity = signed(owner.operation).and_then(|o| Ok((o, signed(owner.epoch)?)));
            if let Some(outcome) = identity.ok().and_then(|id| flight(&self.session, id)) {
                return Ok(outcome?);
            }
        }
        self.unit(owner, true, body)
    }

    /// One read unit: one fenced autocommit statement whose read joins the
    /// owner's row (`reads`), so it needs no transaction of its own.
    fn read<T>(
        &self,
        owner: Owner,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, Failure>,
    ) -> AcquisitionResult<T> {
        self.available()?;
        if owner.epoch == 0 || owner.epoch != self.epoch.get() {
            return Err(Failure::Stale.into());
        }
        Ok(self.session.read_statement(body)?)
    }

    /// Refuses every unit on a schema made without the acquisition tables.
    fn available(&self) -> Result<(), Failure> {
        match self.tables {
            true => Ok(()),
            false => Err(Failure::Unavailable),
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

impl Acquisition for PostgresAcquisition {
    fn begin(&self, facts: &Begin) -> AcquisitionResult<Owner> {
        self.available()?;
        // A first operation claims the session's epoch; no lock a live
        // owner's read needs is held across the lane wait.
        self.epoch.begin(|known| {
            Ok(self
                .session
                .run(true, |tx| writes::begin(tx, known, facts))?)
        })
    }

    /// A PostgreSQL Store has no host directory the source could contain.
    fn placement(&self) -> Option<PathBuf> {
        None
    }

    fn put_entries(&self, owner: Owner, entries: &[NewEntry]) -> AcquisitionResult<Vec<u64>> {
        let bytes = entries.iter().map(|entry| {
            entry.key.name.len() + entry.host_path.as_ref().map_or(0, Vec::len) + WRITE_ROW_BYTES
        });
        bounded(entries.len(), bytes.sum())?;
        self.flight(
            owner,
            |session, id| {
                Some(unit::run(session, id, |unit| {
                    writes::append_entries(unit, id.0, entries)
                }))
            },
            |tx| writes::put_entries(tx, owner, entries),
        )
    }

    fn unplaced_children(
        &self,
        owner: Owner,
        parent: u64,
        after: Option<&[u8]>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Unplaced>> {
        self.read(owner, |tx| {
            reads::unplaced_children(tx, owner, parent, after, limits)
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
                + child.host.as_ref().map_or(0, |(_, path)| path.len())
                + WRITE_ROW_BYTES
        });
        bounded(placed.len(), bytes.sum())?;
        self.flight(
            owner,
            |session, id| {
                let signed_parent = signed(parent).ok()?;
                let state = std::cell::RefCell::new(Default::default());
                Some(unit::run(session, id, |unit| {
                    placement::place(unit, id.0, signed_parent, placed, &state)?;
                    let state: &crate::backend::sqlite::acquisition::placement::Placement =
                        &state.borrow();
                    Ok((state.canonical.clone(), state.rows, state.bytes))
                }))
            },
            |tx| placement::place_children(tx, owner, parent, placed),
        )
    }

    fn directories(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Directory>> {
        self.read(owner, |tx| reads::directories(tx, owner, after, limits))
    }

    fn directory_path(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Vec<u8>>> {
        self.read(owner, |tx| reads::directory_path(tx, owner, position))
    }

    fn jobs(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Job>> {
        self.read(owner, |tx| reads::jobs(tx, owner, after, limits))
    }

    fn job(&self, owner: Owner, position: u64) -> AcquisitionResult<Option<Job>> {
        self.read(owner, |tx| reads::job(tx, owner, position))
    }

    fn complete_files(&self, owner: Owner, roots: &[(u64, ObjectId)]) -> AcquisitionResult<()> {
        bounded(roots.len(), roots.len() * 48)?;
        let which = writes::Roots::Files;
        self.flight(
            owner,
            |session, id| flights::record_roots(session, id, which, roots),
            |tx| writes::record_roots(tx, owner, which, roots),
        )
    }

    fn file_roots(
        &self,
        owner: Owner,
        after: Option<u64>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<FileRoot>> {
        self.read(owner, |tx| reads::file_roots(tx, owner, after, limits))
    }

    fn entries(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Entry>> {
        self.read(owner, |tx| reads::entries(tx, owner, after, limits))
    }

    fn bindings(
        &self,
        owner: Owner,
        after: Option<&EntryKey>,
        limits: Limits,
    ) -> AcquisitionResult<Vec<Binding>> {
        self.read(owner, |tx| reads::bindings(tx, owner, after, limits))
    }

    fn set_directory_roots(
        &self,
        owner: Owner,
        roots: &[(u64, ObjectId)],
    ) -> AcquisitionResult<()> {
        bounded(roots.len(), roots.len() * 48)?;
        let which = writes::Roots::Directories;
        self.flight(
            owner,
            |session, id| flights::record_roots(session, id, which, roots),
            |tx| writes::record_roots(tx, owner, which, roots),
        )
    }

    fn advance(&self, owner: Owner, phase: Phase) -> AcquisitionResult<()> {
        self.flight(
            owner,
            |session, id| Some(flights::advance(session, id, phase)),
            |tx| writes::advance(tx, owner, phase),
        )
    }

    fn discard(&self, owner: Owner, rows: usize) -> AcquisitionResult<Discarded> {
        self.flight(
            owner,
            |session, id| {
                Some(flights::dispose(session, id, budget(rows), false).map(|d| d.discarded))
            },
            |tx| cleanup::discard(tx, owner, budget(rows)),
        )
    }

    fn dispose(&self, owner: Owner, rows: usize) -> AcquisitionResult<Disposal> {
        self.flight(
            owner,
            |session, id| Some(flights::dispose(session, id, budget(rows), true)),
            |tx| cleanup::dispose(tx, owner, budget(rows)),
        )
    }

    fn release(&self, owner: Owner) -> AcquisitionResult<()> {
        self.unit_work(owner, true, |tx, work| cleanup::release(tx, owner, work))
    }

    fn work(&self, owner: Owner) -> AcquisitionResult<AcquisitionWork> {
        self.read(owner, |tx| Ok(writes::owner(tx, owner)?.1))
    }

    fn abandoned(&self, after: Option<u64>, limit: usize) -> AcquisitionResult<Vec<Abandoned>> {
        self.available()?;
        let current = self.epoch.settled();
        let limit = limit.clamp(1, READ_WINDOW_ROWS) as i64;
        Ok(self
            .session
            .run(false, |tx| cleanup::abandoned(tx, current, after, limit))?)
    }

    fn discard_abandoned(&self, abandoned: Owner, rows: usize) -> AcquisitionResult<Discarded> {
        self.available()?;
        // A live operation of this session is never abandoned.
        if abandoned.epoch == self.epoch.settled() {
            return Err(Failure::Stale.into());
        }
        Ok(self.session.run(true, |tx| {
            writes::owner(tx, abandoned)?;
            Ok::<_, Failure>(cleanup::dispose(tx, abandoned, budget(rows))?.discarded)
        })?)
    }
}
