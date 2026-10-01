//! Exact mutation boundaries and bounded tail retirement over final spans.
use crate::engine::{now, Engine};
use rusqlite::{params, Connection, OptionalExtension, StatementStatus};
pub const BOUNDARY:&str="SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 ORDER BY start DESC LIMIT 1";
pub const DELETE_RANGE: &str = "DELETE FROM extents WHERE ino=?1 AND start>=?2 AND start<?3";
pub(crate) type Span = (i64, i64, i64, i64);
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub boundary_queries: u64,
    pub delete_queries: u64,
    pub deleted_rows: u64,
    pub vm_steps: u64,
    pub truncate_batches: u64,
    pub truncated_rows: u64,
}
pub(crate) fn boundary(
    db: &Connection,
    id: i64,
    at: i64,
    work: &mut Work,
) -> Result<Option<Span>, String> {
    let mut q = db.prepare_cached(BOUNDARY).map_err(|e| e.to_string())?;
    q.reset_status(StatementStatus::VmStep);
    let row: Option<Span> = q
        .query_row(params![id, at], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    work.boundary_queries += 1;
    work.vm_steps += q.get_status(StatementStatus::VmStep).max(0) as u64;
    Ok(row.filter(|r| r.1 > at))
}
pub(crate) fn delete_range(
    db: &Connection,
    id: i64,
    lower: i64,
    end: i64,
    work: &mut Work,
) -> Result<(), String> {
    let mut q = db.prepare_cached(DELETE_RANGE).map_err(|e| e.to_string())?;
    q.reset_status(StatementStatus::VmStep);
    work.deleted_rows += q
        .execute(params![id, lower, end])
        .map_err(|e| e.to_string())? as u64;
    work.delete_queries += 1;
    work.vm_steps += q.get_status(StatementStatus::VmStep).max(0) as u64;
    Ok(())
}
impl Engine {
    pub fn truncate(&mut self, id: i64, size: i64) -> Result<(), String> {
        self.source_ready()?;
        if size < 0 {
            return Err("EFBIG".into());
        }
        if self.node(id)?.kind != 1 {
            return Err("EISDIR".into());
        }
        let (sec, nano) = now();
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        if tx.execute("UPDATE inodes SET size=?2,base_visible=min(base_visible,?2),seconds=?3,nanos=?4,dirty=CASE WHEN links>0 OR published=1 THEN 1 ELSE 0 END WHERE id=?1",params![id,size,sec,nano]).map_err(|e|e.to_string())?!=1{return Err("truncate inode identity".into());}
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        // Visibility is published first; partial cleanup cannot expose deleted tail.
        let result = self.trim_captured_tail(id, size);
        if result.is_err() {
            self.installation_failed = true;
        }
        result
    }
    fn trim_captured_tail(&mut self, id: i64, size: i64) -> Result<(), String> {
        let mut work = self.mutation_work.get();
        if let Some((start, _, _, _)) = boundary(&self.db, id, size, &mut work)? {
            self.db
                .execute(
                    "UPDATE extents SET end=?3 WHERE ino=?1 AND start=?2",
                    params![id, start, size],
                )
                .map_err(|e| e.to_string())?;
        }
        loop {
            let tx = self.db.transaction().map_err(|e| e.to_string())?;
            let n=tx.execute("DELETE FROM extents WHERE ino=?1 AND start IN (SELECT start FROM extents WHERE ino=?1 AND start>=?2 ORDER BY start LIMIT 64)",params![id,size]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if n > crate::sql_windows::ROWS {
                return Err("truncate SQL window".into());
            }
            work.truncate_batches += 1;
            work.truncated_rows += n as u64;
        }
        self.mutation_work.set(work);
        self.retire_sources()?;
        Ok(())
    }
}
