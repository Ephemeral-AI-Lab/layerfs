//! Ready-only indexed terminal deletion, with fixed rows and BLOB-byte windows.
use crate::{
    close::CLOSE_KEY,
    db::{integer, unsigned},
    Overlay, OverlayResult, StatementKind,
};
const BYTES: u64 = 65536;
const READY:&str="SELECT ns,cursor FROM reclaim INDEXED BY reclaim_ready WHERE queue_key=?1 AND ns>?2 ORDER BY ns LIMIT 1";
const PAYLOAD:&str="SELECT rowid,length(data)+ifnull(length(validity),0) FROM payload INDEXED BY payload_namespace_row WHERE ns=?1 ORDER BY rowid LIMIT 14";

/// One short physical cleanup step; bytes count declared BLOB/name data, not pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReclaimStep {
    pub namespace: u64,
    pub rows: u64,
    pub data_bytes: u64,
    pub done: bool,
}
impl Overlay {
    /// One ready namespace turn, rotating after the caller's last namespace.
    /// No held namespace is probed; last-owner transitions maintain readiness.
    pub fn reclaim_closed(&self, after_namespace: u64) -> OverlayResult<Option<ReclaimStep>> {
        self.available()?;
        if !self.closed_ready.get() {
            return Ok(None);
        }
        self.atomic_cleanup(|| {
            let after = integer(after_namespace)?;
            let mut rows = self.query(
                StatementKind::Reclaim,
                READY,
                &[&CLOSE_KEY, &after],
                16,
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )?;
            if rows.is_empty() && after != 0 {
                rows = self.query(
                    StatementKind::Reclaim,
                    READY,
                    &[&CLOSE_KEY, &0_i64],
                    16,
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?;
            }
            let Some((ns, phase)) = rows.pop() else {
                self.closed_ready.set(false);
                return Ok(None);
            };
            let (count, bytes) = match phase {
                0 => self.delete_payload(ns)?,
                1 => self.delete_names(ns)?,
                2 => self.delete_inodes(ns)?,
                3 => self.delete_scratch(ns, "scratch")?,
                4 => self.delete_old_reclaim(ns)?,
                5 => self.delete_steps(ns)?,
                6 => self.delete_maintenance(ns)?,
                7 => self.delete_orphan_metadata(ns, "orphan")?,
                8 => self.delete_orphan_metadata(ns, "file_custody")?,
                9 => self.delete_scratch(ns, "owned_scratch")?,
                10 => self.delete_wait(ns)?,
                _ => {
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM reclaim WHERE ns=?1 AND queue_key=?2",
                        &[&ns, &CLOSE_KEY],
                        16,
                    )?;
                    self.execute(
                        StatementKind::Reclaim,
                        "DELETE FROM workspace WHERE ns=?1 AND lifecycle=1",
                        &[&ns],
                        8,
                    )?;
                    return Ok(Some(ReclaimStep {
                        namespace: ns as u64,
                        rows: 2,
                        data_bytes: 0,
                        done: true,
                    }));
                }
            };
            if count == 0 {
                self.execute(
                    StatementKind::Reclaim,
                    "UPDATE reclaim SET cursor=cursor+1 WHERE ns=?1 AND queue_key=?2",
                    &[&ns, &CLOSE_KEY],
                    16,
                )?;
            }
            Ok(Some(ReclaimStep {
                namespace: ns as u64,
                rows: count,
                data_bytes: bytes,
                done: false,
            }))
        })
    }
    /// Query plans for the actual ready queue and terminal payload cursor.
    pub fn explain_closed_reclaim(&self) -> OverlayResult<Vec<String>> {
        let mut plans = self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {READY}"),
            &[&CLOSE_KEY, &0_i64],
            16,
            |r| r.get(3),
        )?;
        plans.extend(self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {PAYLOAD}"),
            &[&0_i64],
            8,
            |r| r.get::<_, String>(3),
        )?);
        Ok(plans)
    }
    fn delete_orphan_metadata(&self, ns: i64, table: &str) -> OverlayResult<(u64, u64)> {
        // Only the two static names above reach this helper.
        let rows = self.query(
            StatementKind::Reclaim,
            &format!("SELECT serial FROM {table} WHERE ns=?1 ORDER BY serial LIMIT 64"),
            &[&ns],
            8,
            |r| r.get::<_, i64>(0),
        )?;
        for serial in &rows {
            self.execute(
                StatementKind::Reclaim,
                &format!("DELETE FROM {table} WHERE ns=?1 AND serial=?2"),
                &[&ns, serial],
                16,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
    fn delete_wait(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT gen,serial FROM orphan_wait WHERE ns=?1 ORDER BY gen,serial LIMIT 64",
            &[&ns],
            8,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (gen, serial) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM orphan_wait WHERE ns=?1 AND gen=?2 AND serial=?3",
                &[&ns, gen, serial],
                24,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
    fn delete_payload(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(StatementKind::Reclaim, PAYLOAD, &[&ns], 8, |r| {
            Ok((r.get::<_, i64>(0)?, unsigned(r, 1)?))
        })?;
        let mut bytes = 0;
        for (row, size) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM payload WHERE rowid=?1 AND ns=?2",
                &[row, &ns],
                16,
            )?;
            bytes += size;
        }
        Ok((rows.len() as u64, bytes))
    }
    fn delete_steps(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT serial,gen,depth FROM shrink WHERE ns=?1 ORDER BY serial,gen,depth LIMIT 64",
            &[&ns],
            8,
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )?;
        for (serial, gen, depth) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 AND depth=?4",
                &[&ns, serial, gen, depth],
                32,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
    fn delete_names(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT parent,name,gen FROM dentry WHERE ns=?1 ORDER BY parent,name,gen LIMIT 64",
            &[&ns],
            8,
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )?;
        let mut bytes = 0;
        for (parent, name, gen) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM dentry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4",
                &[&ns, parent, name, gen],
                24 + name.len() as u64,
            )?;
            bytes += name.len() as u64;
        }
        Ok((rows.len() as u64, bytes))
    }
    fn delete_inodes(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT serial,gen FROM inode WHERE ns=?1 ORDER BY serial,gen LIMIT 64",
            &[&ns],
            8,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (serial, gen) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=?3",
                &[&ns, serial, gen],
                24,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
    fn delete_scratch(&self, ns: i64, table: &str) -> OverlayResult<(u64, u64)> {
        let rows=self.query(StatementKind::Reclaim,&format!("SELECT operation,kind,key,length(value) FROM {table} WHERE ns=?1 ORDER BY operation,kind,key LIMIT 64"),&[&ns],8,|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,unsigned(r,3)?)))?;
        let mut count = 0;
        let mut bytes = 0;
        for (operation, kind, key, size) in &rows {
            if count != 0 && bytes + size > BYTES {
                break;
            }
            self.execute(
                StatementKind::Reclaim,
                &format!("DELETE FROM {table} WHERE ns=?1 AND operation=?2 AND kind=?3 AND key=?4"),
                &[&ns, operation, kind, key],
                32,
            )?;
            count += 1;
            bytes += size;
        }
        Ok((count, bytes))
    }
    fn delete_old_reclaim(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows=self.query(StatementKind::Reclaim,"SELECT queue_key FROM reclaim WHERE ns=?1 AND queue_key<?2 ORDER BY queue_key LIMIT 64",&[&ns,&CLOSE_KEY],16,|r|r.get::<_,i64>(0))?;
        for key in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM reclaim WHERE ns=?1 AND queue_key=?2",
                &[&ns, key],
                16,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
    fn delete_maintenance(&self, ns: i64) -> OverlayResult<(u64, u64)> {
        let rows = self.query(
            StatementKind::Reclaim,
            "SELECT kind,resource,target FROM maintenance
            WHERE ns=?1 ORDER BY kind,resource,target LIMIT 64",
            &[&ns],
            8,
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )?;
        for (kind, resource, target) in &rows {
            self.execute(
                StatementKind::Reclaim,
                "DELETE FROM maintenance WHERE ns=?1 AND kind=?2 AND resource=?3 AND target=?4",
                &[&ns, kind, resource, target],
                32,
            )?;
        }
        Ok((rows.len() as u64, 0))
    }
}
