//! Exact predecessor plus bounded start-range scan over nonoverlapping live spans.
use crate::engine::Engine;
use rusqlite::{params, OptionalExtension, StatementStatus};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
pub const PREDECESSOR: &str =
    "SELECT start,end FROM extents WHERE ino=?1 AND start<=?2 ORDER BY start DESC LIMIT 1";
pub const RANGE:&str="SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start>=?2 AND start<?3 ORDER BY start";
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub predecessor_queries: u64,
    pub range_queries: u64,
    pub rows: u64,
    pub vm_steps: u64,
}
impl Engine {
    pub(crate) fn read_local_spans(
        &self,
        id: i64,
        at: i64,
        end: i64,
        out: &mut [u8],
    ) -> Result<(), String> {
        let mut work = self.span_work.get();
        let mut q = self
            .db
            .prepare_cached(PREDECESSOR)
            .map_err(|e| e.to_string())?;
        q.reset_status(StatementStatus::VmStep);
        let predecessor: Option<(i64, i64)> = q
            .query_row(params![id, at], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(|e| e.to_string())?;
        work.predecessor_queries += 1;
        work.vm_steps += q.get_status(StatementStatus::VmStep).max(0) as u64;
        drop(q);
        let lower = predecessor
            .filter(|(_, stop)| *stop > at)
            .map_or(at, |(start, _)| start);
        let mut q = self.db.prepare_cached(RANGE).map_err(|e| e.to_string())?;
        q.reset_status(StatementStatus::VmStep);
        let mut rows = q
            .query(params![id, lower, end])
            .map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let start: i64 = row.get(0).map_err(|e| e.to_string())?;
            let stop: i64 = row.get(1).map_err(|e| e.to_string())?;
            let source: i64 = row.get(2).map_err(|e| e.to_string())?;
            let offset: i64 = row.get(3).map_err(|e| e.to_string())?;
            let a = start.max(at);
            let z = stop.min(end);
            if a >= z {
                return Err("live span range invariant".into());
            }
            let mut file =
                File::open(self.sources.join(source.to_string())).map_err(|e| e.to_string())?;
            file.seek(SeekFrom::Start((offset + a - start) as u64))
                .map_err(|e| e.to_string())?;
            file.read_exact(&mut out[(a - at) as usize..(z - at) as usize])
                .map_err(|e| e.to_string())?;
            work.rows += 1;
            let mut reads = self.reads.get();
            reads.local += (z - a) as u64;
            self.reads.set(reads);
        }
        drop(rows);
        work.range_queries += 1;
        work.vm_steps += q.get_status(StatementStatus::VmStep).max(0) as u64;
        self.span_work.set(work);
        Ok(())
    }
}
