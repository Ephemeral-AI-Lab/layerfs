//! Marking an export base and reading a Store's change state.
use super::connection::{finish, open_store};
use super::digest::base_state;
use super::lookup::{catalog_of, last_seq, state_of, table_exists};
use super::{ChangeCursor, ChangeError};
use rusqlite::{params, OpenFlags};
use std::path::Path;

/// Makes the offline Store at `path` the start of an export chain and returns
/// its cursor. Clears the change log down to one sequence marker and enables
/// logging. Marking again starts a new chain at the current sequence. `token`
/// makes the base identity unique; supply fresh random bytes.
///
/// A copy of the closed Store file is then a replica at the returned cursor.
/// When another connection reads the Store, the mark still commits but returns
/// [`ChangeError::NotCheckpointed`], and the file alone is not yet that replica.
pub fn mark_base(path: &Path, token: [u8; 32]) -> Result<ChangeCursor, ChangeError> {
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    c.execute_batch("BEGIN IMMEDIATE")?;
    let marked = (|| {
        if !table_exists(&c, "main", "change_log")? {
            return Err(ChangeError::Integrity(
                "the Store has no change log; open it writable once to migrate it",
            ));
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

/// The change state recorded in the Store at `path`, if it is an export base.
pub fn read_state(path: &Path) -> Result<Option<ChangeCursor>, ChangeError> {
    let c = open_store(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !table_exists(&c, "main", "change_state")? {
        return Ok(None);
    }
    state_of(&c, "main")
}
