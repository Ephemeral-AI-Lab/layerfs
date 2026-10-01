//! Draft6 fixed reset only after actual Finished, exact empty relations and native ACK.
use super::{profile, session::Resource, ScratchSession};
use crate::{StorageError, StorageResult};
use rusqlite::Connection;
const TABLES: &[&str] = &[
    "draft_headers",
    "draft_bodies",
    "draft_references",
    "draft_predecessors",
    "draft_counts",
    "draft_jobs",
    "draft_committed",
    "draft_emissions",
];
fn empty(c: &Connection) -> StorageResult<()> {
    for table in TABLES {
        if !super::count_index::empty(c, table)? {
            return Err(StorageError::Integrity("draft pool nonempty projection"));
        }
    }
    Ok(())
}
pub(crate) fn verify(c: &Connection, r: &Resource) -> StorageResult<()> {
    if !c.is_autocommit() {
        return Err(StorageError::Integrity("draft pool open transaction"));
    }
    verify_rows(c, r)
}
pub(crate) fn verify_transaction(c: &Connection, r: &Resource) -> StorageResult<()> {
    if c.is_autocommit() {
        return Err(StorageError::Integrity(
            "draft pool missing rebind transaction",
        ));
    }
    verify_rows(c, r)
}
fn verify_rows(c: &Connection, r: &Resource) -> StorageResult<()> {
    let draft = r
        .draft
        .as_ref()
        .ok_or(StorageError::Integrity("draft pool owner"))?;
    let header = r
        .header
        .as_ref()
        .ok_or(StorageError::Integrity("draft pool header"))?;
    let session:i64=c.query_row("SELECT COUNT(*) FROM session_owner WHERE id=1 AND header=?1 AND scope IS NULL AND sealed=0 AND records=0 AND record_bytes=0 AND digest IS NULL",[header.as_bytes()],|row|row.get(0))?;
    let owner:i64=c.query_row("SELECT COUNT(*) FROM draft_owner WHERE id=1 AND scope=?1 AND stage=0 AND records=0 AND bytes=0 AND next_job=1 AND selected IS NULL",[draft.scope.as_bytes().as_slice()],|row|row.get(0))?;
    if session != 1
        || owner != 1
        || !draft.ledger.ended
        || draft.failed.get()
        || draft.attempt.is_some()
    {
        return Err(StorageError::Integrity("draft pool exact reset metadata"));
    }
    empty(c)
}
impl ScratchSession {
    /// Reset only the known successful empty private draft profile; S stays owned.
    pub fn reset_completed_drafts(&mut self) -> StorageResult<()> {
        let result = (|| {
            let r = self
                .resource
                .as_mut()
                .ok_or(StorageError::Integrity("draft pool released owner"))?;
            let draft = r
                .draft
                .as_ref()
                .ok_or(StorageError::Integrity("draft pool selected profile"))?;
            if r.plan.version() != 6
                || r.known_clean
                || r.unknown.get()
                || r.native.quarantined
                || r.release_attempted
                || !draft.ledger.ended
                || draft.failed.get()
                || draft.attempt.is_some()
                || draft.ledger.records != 0
                || draft.ledger.bytes != 0
                || draft.ledger.selected.is_some()
            {
                return Err(StorageError::Integrity(
                    "draft pool successful Finished required",
                ));
            }
            r.verify()?;
            empty(r.connection.as_ref().unwrap())?;
            r.native.reserve()?;
            let c = r.connection.as_ref().unwrap();
            crate::sqlite::write::begin_immediate(c)?;
            let change = (|| {
                super::draft_index::verify(c, r.draft.as_ref().unwrap())?;
                empty(c)?;
                if c.execute("UPDATE draft_owner SET stage=0,records=0,bytes=0,next_job=1,selected=NULL WHERE id=1 AND scope=?1 AND stage=1 AND records=0 AND bytes=0 AND selected IS NULL",[r.draft.as_ref().unwrap().scope.as_bytes().as_slice()])?!=1 {return Err(StorageError::Integrity("draft pool reset acknowledgement"));}
                Ok(())
            })();
            profile::finish_write_guarded(c, change, r.engine)?;
            r.native.observe_allocation()?;
            verify(r.connection.as_ref().unwrap(), r)?;
            r.known_clean = true;
            r.logical_released = true;
            Ok(())
        })();
        self.finish(result)
    }
}
