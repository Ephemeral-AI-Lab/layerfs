//! Marking, moving and ending an export base, and reading a Store's change
//! state.
use super::connection::{begin, commit, finish, lane, open_store};
use super::digest::base_state;
use super::log;
use super::lookup::{catalog_of, last_seq, range, state_of, table_exists};
use super::{ChangeCursor, ChangeError};
use rusqlite::{params, OpenFlags};
use std::path::Path;

/// Makes the offline Store at `path` the start of an export chain and returns
/// its cursor. A Store that is not yet a base gains the change log first (see
/// [`unmark_base`]); a base keeps it, cleared down to one sequence marker, so
/// marking again starts a new chain at the current sequence. `token` makes the
/// base identity unique; supply fresh random bytes.
///
/// A copy of the closed Store file is then a replica at the returned cursor.
/// When another connection reads the Store, the mark still commits but returns
/// [`ChangeError::NotCheckpointed`], and the file alone is not yet that replica.
/// Takes the Store's writer lane: refused with [`ChangeError::Busy`] while
/// another writer holds it.
pub fn mark_base(path: &Path, token: [u8; 32]) -> Result<ChangeCursor, ChangeError> {
    let _lane = lane(path)?;
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    begin(&c)?;
    let marked = (|| {
        if !table_exists(&c, "main", "change_log")? {
            log::add(&c)?;
        }
        let catalog = catalog_of(&c, "main")?;
        let recorded = state_of(&c, "main")?.map_or(0, |s| s.seq);
        let seq = last_seq(&c, "main")?.unwrap_or(0).max(recorded);
        c.execute("DELETE FROM change_log", [])?;
        if seq > 0 {
            c.execute("INSERT INTO change_log(seq,tbl) VALUES(?1,0)", [seq as i64])?;
        }
        let state = base_state(&catalog, seq, &token);
        c.execute(
            "INSERT INTO change_state(id,state,seq) VALUES(1,?1,?2) \
             ON CONFLICT(id) DO UPDATE SET state=excluded.state,seq=excluded.seq",
            params![&state[..], seq as i64],
        )?;
        Ok(ChangeCursor { state, seq })
    })();
    finish(&c, marked)
}

/// Moves the export base of the Store at `path` forward to `oldest`, the
/// cursor of the least advanced replica still followed, and deletes the change
/// log up to it. Returns the number of log entries deleted. Exports from a
/// cursor before `oldest` are refused afterwards: such a replica starts again
/// from a new base.
///
/// `oldest` is checked as [`export_changes`](super::export_changes) checks its
/// cursor: its state when it is the recorded base, its sequence above it. A
/// wrong state above the base therefore makes later exports from the true
/// cursor refused, never misapplied.
///
/// For a live Store: takes the writer lane (refused with [`ChangeError::Busy`]
/// while another writer holds it) and commits without a checkpoint.
pub fn trim_changes(path: &Path, oldest: ChangeCursor) -> Result<u64, ChangeError> {
    let _lane = lane(path)?;
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    begin(&c)?;
    let trimmed = (|| {
        let (base, _) = range(&c, oldest)?;
        if oldest == base {
            return Ok(0);
        }
        let deleted = c.execute(
            "DELETE FROM change_log WHERE seq<=?1 AND tbl>0",
            [oldest.seq as i64],
        )?;
        // The marker keeps the next sequence after every one exported.
        c.execute("DELETE FROM change_log WHERE tbl=0", [])?;
        c.execute(
            "INSERT INTO change_log(seq,tbl) VALUES(?1,0)",
            [oldest.seq as i64],
        )?;
        c.execute(
            "UPDATE change_state SET state=?1,seq=?2 WHERE id=1",
            params![&oldest.state[..], oldest.seq as i64],
        )?;
        Ok(deleted as u64)
    })();
    commit(&c, trimmed)
}

/// Ends the export chain of the Store at `path`: drops its change log and
/// logging triggers and records the schema of a Store that was never marked,
/// which a build without this module opens too. Its replicas can no longer be
/// followed. A copy of an export base, such as a backup or a Store restored
/// from one, is itself a base and logs every write until it is unmarked.
///
/// For a live Store: takes the writer lane (refused with [`ChangeError::Busy`]
/// while another writer holds it) and commits without a checkpoint.
pub fn unmark_base(path: &Path) -> Result<(), ChangeError> {
    let _lane = lane(path)?;
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    begin(&c)?;
    let removed = (|| {
        if !table_exists(&c, "main", "change_state")? {
            return Err(ChangeError::NotBase);
        }
        log::remove(&c)
    })();
    commit(&c, removed)
}

/// The change state recorded in the Store at `path`, if it is an export base.
pub fn read_state(path: &Path) -> Result<Option<ChangeCursor>, ChangeError> {
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    state_of(&c, "main")
}
