//! Borrowed exact release frontier and fixed metadata rows.
use super::{
    count_index,
    release_state::{Release, Snapshot},
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{CanonicalScope, ReleaseFrame, ReleaseJob};
use rusqlite::{types::ValueRef, Connection};
fn unsigned(v: i64) -> StorageResult<u64> {
    u64::try_from(v).map_err(|_| StorageError::Integrity("negative release metadata"))
}
fn optional(v: Option<i64>) -> StorageResult<Option<u64>> {
    v.map(unsigned).transpose()
}
pub(crate) fn deferred(c: &Connection) -> StorageResult<()> {
    let n:i64=c.query_row("SELECT COUNT(*) FROM release_owner WHERE id=1 AND scope IS NULL AND stage=0 AND seeds IS NULL AND seeded=0 AND seed_after IS NULL AND sequence=0 AND pending=0 AND current_key IS NULL AND current_value IS NULL AND frames=0 AND completed=0 AND directories=0 AND maximum_depth=0",[],|r|r.get(0))?;
    if n != 1
        || !count_index::empty(c, "release_jobs")?
        || !count_index::empty(c, "release_frames")?
    {
        return Err(StorageError::Integrity("deferred release owner"));
    }
    Ok(())
}
pub(crate) fn verify(c: &Connection, state: &Release) -> StorageResult<()> {
    let _credit = state.memory.reserve(228 + 579 + 82 + 25 + 11 * 8)?;
    let mut q=c.prepare("SELECT scope,stage,seeds,seeded,seed_after,sequence,pending,current_key,current_value,frames,completed,directories,maximum_depth FROM release_owner WHERE id=1")?;
    let mut rows = q.query([])?;
    let r = rows
        .next()?
        .ok_or(StorageError::Integrity("release owner missing"))?;
    let s = &state.snapshot;
    let selected = if s.stage == 0 {
        matches!(r.get_ref(0)?, ValueRef::Null)
    } else {
        count_index::blob(r.get_ref(0)?, 228)? == state.scope.encode()
    };
    if !selected
        || unsigned(r.get(1)?)? != u64::from(s.stage)
        || unsigned(r.get(3)?)? != s.seeded
        || optional(r.get(4)?)? != s.seed_after
        || unsigned(r.get(5)?)? != s.sequence
        || unsigned(r.get(6)?)? != s.pending
        || unsigned(r.get(9)?)? != s.frames
        || unsigned(r.get(10)?)? != s.completed
        || unsigned(r.get(11)?)? != s.directories
        || unsigned(r.get(12)?)? != s.maximum_depth
    {
        return Err(StorageError::Integrity("release exact metadata"));
    }
    match (r.get_ref(2)?, &s.seeds) {
        (ValueRef::Null, None) => {}
        (ValueRef::Blob(b), Some(seeds)) if b == seeds.encode() => {}
        _ => return Err(StorageError::Integrity("release exact seed seal")),
    }
    match (r.get_ref(7)?, r.get_ref(8)?, s.current) {
        (ValueRef::Null, ValueRef::Null, None) => {}
        (ValueRef::Blob(k), ValueRef::Blob(v), Some(job))
            if ReleaseJob::decode(&state.scope, k, v)? == job => {}
        _ => return Err(StorageError::Integrity("release exact retained current")),
    }
    if rows.next()?.is_some() {
        return Err(StorageError::Integrity("release owner multiplicity"));
    }
    Ok(())
}
pub(crate) fn write(c: &Connection, scope: &CanonicalScope, s: &Snapshot) -> StorageResult<()> {
    let seeds = s.seeds.as_ref().map(|v| v.encode());
    let key = s.current.map(|j| scope.key(j.sequence)).transpose()?;
    let value = s.current.map(|j| j.encode_value()).transpose()?;
    if c.execute("UPDATE release_owner SET scope=?1,stage=?2,seeds=?3,seeded=?4,seed_after=?5,sequence=?6,pending=?7 WHERE id=1",rusqlite::params![scope.encode().as_slice(),s.stage,seeds.as_ref().map(|v|v.as_slice()),s.seeded as i64,s.seed_after.map(|v|v as i64),s.sequence as i64,s.pending as i64])?!=1 || c.execute("UPDATE release_owner SET current_key=?1,current_value=?2,frames=?3,completed=?4,directories=?5,maximum_depth=?6 WHERE id=1",rusqlite::params![key.as_ref().map(|v|v.as_slice()),value.as_ref().map(|v|v.as_slice()),s.frames as i64,s.completed as i64,s.directories as i64,s.maximum_depth as i64])?!=1 {return Err(StorageError::Integrity("release fixed acknowledgement"));}
    Ok(())
}
pub(crate) fn job(c: &Connection, scope: &CanonicalScope) -> StorageResult<Option<ReleaseJob>> {
    let mut q = c.prepare("SELECT key,value FROM release_jobs ORDER BY key LIMIT 1")?;
    let mut r = q.query([])?;
    r.next()?
        .map(|r| {
            Ok(ReleaseJob::decode(
                scope,
                count_index::blob(r.get_ref(0)?, 25)?,
                count_index::blob(r.get_ref(1)?, 82)?,
            )?)
        })
        .transpose()
}
pub(crate) fn frame(c: &Connection, scope: &CanonicalScope) -> StorageResult<Option<ReleaseFrame>> {
    let mut q = c.prepare("SELECT key,value FROM release_frames ORDER BY key DESC LIMIT 1")?;
    let mut r = q.query([])?;
    r.next()?
        .map(|r| {
            Ok(ReleaseFrame::decode(
                scope,
                count_index::blob(r.get_ref(0)?, 25)?,
                count_index::blob(r.get_ref(1)?, 291)?,
            )?)
        })
        .transpose()
}
