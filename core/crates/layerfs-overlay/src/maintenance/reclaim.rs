//! Ready-only indexed terminal deletion, with fixed rows and BLOB-byte windows.
use crate::{
    close::CLOSE_KEY,
    db::{integer, unsigned},
    Overlay, OverlayResult, StatementKind,
};
use rusqlite::{types::Value, ToSql};
const BYTES: u64 = 65536;
const READY:&str="SELECT ns,cursor FROM reclaim INDEXED BY reclaim_ready WHERE queue_key=?1 AND ns>?2 ORDER BY ns LIMIT 1";
/// One terminal table. `page` reads a fixed number of leading rows of one
/// namespace in primary-key order: `keys` key columns, then the bytes the
/// step counts for the row. `through` deletes every row of the namespace up
/// to one key: one statement for the page, whatever its rows.
struct Terminal {
    page: &'static str,
    through: &'static str,
    keys: usize,
    take: Take,
}
/// How much of a page one step deletes.
#[derive(Clone, Copy)]
enum Take {
    /// Every row of the page.
    Rows,
    /// Leading rows inside the value-byte window; always the first.
    Bytes,
    /// Leading rows inside one maintenance page of payload cells.
    Cells,
}
const fn table(page: &'static str, through: &'static str, keys: usize, take: Take) -> Terminal {
    Terminal {
        page,
        through,
        keys,
        take,
    }
}
// The page of earlier queue rows names the close key as a literal.
const _: () = assert!(CLOSE_KEY == i64::MAX);
/// The terminal tables in the order of the namespace's reclaim cursor.
const PHASES: [Terminal; 11] = [
    table(
        "SELECT serial,gen,cell_offset,length(data)+ifnull(length(validity),0) FROM payload WHERE ns=?1 ORDER BY serial,gen,cell_offset LIMIT 14",
        "DELETE FROM payload WHERE ns=?1 AND (serial,gen,cell_offset)<=(?2,?3,?4)",
        3,
        Take::Cells,
    ),
    table(
        "SELECT parent,name,gen,length(name) FROM directory_entry WHERE ns=?1 ORDER BY parent,name,gen LIMIT 64",
        "DELETE FROM directory_entry WHERE ns=?1 AND (parent,name,gen)<=(?2,?3,?4)",
        3,
        Take::Rows,
    ),
    table(
        "SELECT serial,gen,0 FROM inode WHERE ns=?1 ORDER BY serial,gen LIMIT 64",
        "DELETE FROM inode WHERE ns=?1 AND (serial,gen)<=(?2,?3)",
        2,
        Take::Rows,
    ),
    table(
        "SELECT operation,kind,key,length(value) FROM operation_record WHERE ns=?1 ORDER BY operation,kind,key LIMIT 64",
        "DELETE FROM operation_record WHERE ns=?1 AND (operation,kind,key)<=(?2,?3,?4)",
        3,
        Take::Bytes,
    ),
    table(
        "SELECT queue_key,0 FROM reclaim WHERE ns=?1 AND queue_key<9223372036854775807 ORDER BY queue_key LIMIT 64",
        "DELETE FROM reclaim WHERE ns=?1 AND queue_key<=?2",
        1,
        Take::Rows,
    ),
    table(
        "SELECT serial,gen,depth,0 FROM shrink WHERE ns=?1 ORDER BY serial,gen,depth LIMIT 64",
        "DELETE FROM shrink WHERE ns=?1 AND (serial,gen,depth)<=(?2,?3,?4)",
        3,
        Take::Rows,
    ),
    table(
        "SELECT kind,resource,target,0 FROM maintenance WHERE ns=?1 ORDER BY kind,resource,target LIMIT 64",
        "DELETE FROM maintenance WHERE ns=?1 AND (kind,resource,target)<=(?2,?3,?4)",
        3,
        Take::Rows,
    ),
    table(
        "SELECT serial,0 FROM orphan WHERE ns=?1 ORDER BY serial LIMIT 64",
        "DELETE FROM orphan WHERE ns=?1 AND serial<=?2",
        1,
        Take::Rows,
    ),
    table(
        "SELECT serial,0 FROM file_custody WHERE ns=?1 ORDER BY serial LIMIT 64",
        "DELETE FROM file_custody WHERE ns=?1 AND serial<=?2",
        1,
        Take::Rows,
    ),
    table(
        "SELECT operation,kind,key,length(value) FROM owned_operation_record WHERE ns=?1 ORDER BY operation,kind,key LIMIT 64",
        "DELETE FROM owned_operation_record WHERE ns=?1 AND (operation,kind,key)<=(?2,?3,?4)",
        3,
        Take::Bytes,
    ),
    table(
        "SELECT gen,serial,0 FROM orphan_wait WHERE ns=?1 ORDER BY gen,serial LIMIT 64",
        "DELETE FROM orphan_wait WHERE ns=?1 AND (gen,serial)<=(?2,?3)",
        2,
        Take::Rows,
    ),
];
/// The cursor of the orphan table, whose deletion updates the engine's hint.
const ORPHANS: i64 = 7;

/// One short physical cleanup step. Bytes count payload, names and raw operation_record
/// values, not structured identity keys or pages. SQL work records delivered
/// key/value bytes separately; shared allocation remains a distinct observation.
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
            // Tables that hold nothing for this namespace are passed in the
            // same step: one step deletes one page of the first table that
            // still holds rows, or ends the namespace.
            let mut at = phase;
            let (count, bytes) = loop {
                let deleted = match usize::try_from(at).ok() {
                    Some(table) if table < PHASES.len() => self.delete_page(ns, &PHASES[table])?,
                    Some(table) if table == PHASES.len() => {
                        self.delete_indexed_operation_record(ns, None)?
                    }
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
                if deleted.0 != 0 {
                    break deleted;
                }
                at += 1;
            };
            if at == ORPHANS {
                self.orphans_deleted()?;
            }
            if at != phase {
                self.execute(
                    StatementKind::Reclaim,
                    "UPDATE reclaim SET cursor=?3 WHERE ns=?1 AND queue_key=?2",
                    &[&ns, &CLOSE_KEY, &at],
                    24,
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
    /// Query plans of the ready queue and of every terminal page and delete.
    pub fn explain_closed_reclaim(&self) -> OverlayResult<Vec<String>> {
        let mut plans = self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {READY}"),
            &[&CLOSE_KEY, &0_i64],
            16,
            |r| r.get(3),
        )?;
        let zero: [&dyn ToSql; 5] = [&0_i64; 5];
        for table in &PHASES {
            for (sql, bound) in [(table.page, 1), (table.through, 1 + table.keys)] {
                plans.extend(self.query(
                    StatementKind::Explain,
                    &format!("EXPLAIN QUERY PLAN {sql}"),
                    &zero[..bound],
                    8 * bound as u64,
                    |r| r.get::<_, String>(3),
                )?);
            }
        }
        Ok(plans)
    }
    /// Deletes one page of one terminal table with one statement and returns
    /// its rows and counted bytes.
    fn delete_page(&self, ns: i64, table: &Terminal) -> OverlayResult<(u64, u64)> {
        let rows = self.query(StatementKind::Reclaim, table.page, &[&ns], 8, |r| {
            let mut key = Vec::with_capacity(table.keys);
            for column in 0..table.keys {
                key.push(r.get::<_, Value>(column)?);
            }
            Ok((key, unsigned(r, table.keys)?))
        })?;
        let sizes = rows.iter().map(|row| row.1);
        let taken = match table.take {
            Take::Rows => rows.len(),
            Take::Cells => crate::maintenance::PAGE.fit(sizes).0,
            Take::Bytes => {
                let (mut count, mut held) = (0, 0);
                for size in sizes {
                    if count != 0 && held + size > BYTES {
                        break;
                    }
                    count += 1;
                    held += size;
                }
                count
            }
        };
        let Some((last, _)) = taken.checked_sub(1).and_then(|last| rows.get(last)) else {
            return Ok((0, 0));
        };
        let mut bound: Vec<&dyn ToSql> = vec![&ns];
        let mut bytes = 8;
        for value in last {
            bytes += match value {
                Value::Blob(name) => name.len() as u64,
                _ => 8,
            };
            bound.push(value);
        }
        let deleted = self.execute(StatementKind::Reclaim, table.through, &bound, bytes)?;
        Ok((deleted, rows[..taken].iter().map(|row| row.1).sum()))
    }
}
