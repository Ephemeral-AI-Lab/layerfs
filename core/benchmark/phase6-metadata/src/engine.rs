use rusqlite::{Connection, Statement, StatementStatus, ToSql};
use std::{collections::BTreeSet, path::Path, time::Instant};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const LIVE: i64 = i64::MAX;
pub const PAGE: usize = 128;

#[derive(Default)]
pub struct Metrics {
    pub opens: u64,
    pub sql_calls: u64,
    pub sql_ns: u128,
    pub vm_steps: u64,
    pub fullscan_steps: u64,
    pub sorts: u64,
    pub reprepares: u64,
    pub changed: u64,
    pub transactions: u64,
    pub tx_max_changed: u64,
    pub tx_max_ns: u128,
    pub page_rows: usize,
    pub page_fields_bytes: usize,
    pub sql_texts: BTreeSet<String>,
    tx_start: Option<(Instant, u64)>,
}
impl Metrics {
    pub fn json(&self) -> String {
        format!("{{\"opens\":{},\"sql_calls\":{},\"distinct_sql_texts\":{},\"sql_ns\":{},\"vm_steps\":{},\"fullscan_steps\":{},\"sorts\":{},\"reprepares\":{},\"changed_rows\":{},\"transactions\":{},\"tx_max_changed_rows\":{},\"tx_max_ns\":{},\"page_max_rows\":{},\"page_max_field_bytes\":{}}}",self.opens,self.sql_calls,self.sql_texts.len(),self.sql_ns,self.vm_steps,self.fullscan_steps,self.sorts,self.reprepares,self.changed,self.transactions,self.tx_max_changed,self.tx_max_ns,self.page_rows,self.page_fields_bytes)
    }
    pub fn absorb(&mut self, other: &Metrics) {
        self.opens += other.opens;
        self.sql_calls += other.sql_calls;
        self.sql_ns += other.sql_ns;
        self.vm_steps += other.vm_steps;
        self.fullscan_steps += other.fullscan_steps;
        self.sorts += other.sorts;
        self.reprepares += other.reprepares;
        self.changed += other.changed;
        self.transactions += other.transactions;
        self.tx_max_changed = self.tx_max_changed.max(other.tx_max_changed);
        self.tx_max_ns = self.tx_max_ns.max(other.tx_max_ns);
        self.page_rows = self.page_rows.max(other.page_rows);
        self.page_fields_bytes = self.page_fields_bytes.max(other.page_fields_bytes);
        self.sql_texts.extend(other.sql_texts.iter().cloned());
    }
}
pub struct Engine {
    pub db: Connection,
    pub m: Metrics,
}
fn clear(s: &Statement<'_>) {
    for status in [
        StatementStatus::VmStep,
        StatementStatus::FullscanStep,
        StatementStatus::Sort,
        StatementStatus::RePrepare,
    ] {
        s.reset_status(status);
    }
}
fn account(m: &mut Metrics, s: &Statement<'_>, started: Instant, sql: &str) {
    m.sql_calls += 1;
    m.sql_ns += started.elapsed().as_nanos();
    m.vm_steps += s.get_status(StatementStatus::VmStep) as u64;
    m.fullscan_steps += s.get_status(StatementStatus::FullscanStep) as u64;
    m.sorts += s.get_status(StatementStatus::Sort) as u64;
    m.reprepares += s.get_status(StatementStatus::RePrepare) as u64;
    m.sql_texts.insert(sql.to_owned());
}
impl Engine {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0; PRAGMA busy_timeout=0;")?;
        db.set_prepared_statement_cache_capacity(64);
        Ok(Self {
            db,
            m: Metrics {
                opens: 1,
                ..Metrics::default()
            },
        })
    }
    pub fn exec(&mut self, sql: &str, p: &[&dyn ToSql]) -> Result<usize> {
        let t = Instant::now();
        let mut s = self.db.prepare_cached(sql)?;
        clear(&s);
        let executed = s.execute(p)?;
        let n = if matches!(sql, "BEGIN IMMEDIATE" | "COMMIT" | "ROLLBACK") {
            0
        } else {
            executed
        };
        account(&mut self.m, &s, t, sql);
        self.m.changed += n as u64;
        Ok(n)
    }
    pub fn ints(&mut self, sql: &str, p: &[&dyn ToSql]) -> Result<Vec<Vec<i64>>> {
        let t = Instant::now();
        let mut s = self.db.prepare_cached(sql)?;
        clear(&s);
        let columns = s.column_count();
        let mut rows = s.query(p)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            if out.len() >= 256 {
                return Err("query exceeds row window".into());
            }
            let v = (0..columns)
                .map(|i| row.get::<_, i64>(i))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            out.push(v);
        }
        drop(rows);
        account(&mut self.m, &s, t, sql);
        self.m.page_rows = self.m.page_rows.max(out.len());
        self.m.page_fields_bytes = self.m.page_fields_bytes.max(out.len() * columns * 8);
        Ok(out)
    }
    pub fn scalar(&mut self, sql: &str, p: &[&dyn ToSql]) -> Result<i64> {
        Ok(self.ints(sql, p)?.first().ok_or("missing scalar")?[0])
    }
    pub fn begin(&mut self) -> Result<()> {
        self.exec("BEGIN IMMEDIATE", &[])?;
        self.m.tx_start = Some((Instant::now(), self.m.changed));
        self.m.transactions += 1;
        Ok(())
    }
    pub fn commit(&mut self) -> Result<()> {
        self.exec("COMMIT", &[])?;
        let (t, before) = self.m.tx_start.take().ok_or("no transaction")?;
        self.m.tx_max_changed = self.m.tx_max_changed.max(self.m.changed - before);
        self.m.tx_max_ns = self.m.tx_max_ns.max(t.elapsed().as_nanos());
        Ok(())
    }
    pub fn rollback(&mut self) -> Result<()> {
        self.exec("ROLLBACK", &[])?;
        self.m.tx_start = None;
        Ok(())
    }
    pub fn inode(&mut self, w: i64, ino: i64, value: i64, size: i64) -> Result<()> {
        let g = self.scalar("SELECT active FROM workspace WHERE w=?1", &[&w])?;
        let old = self.ints(
            "SELECT born FROM inodes WHERE w=?1 AND ino=?2 AND dead=9223372036854775807",
            &[&w, &ino],
        )?;
        if let Some(row) = old.first() {
            if row[0] == g {
                self.exec("UPDATE inodes SET value=?3,size=?4 WHERE w=?1 AND ino=?2 AND dead=9223372036854775807",&[&w,&ino,&value,&size])?;
                return Ok(());
            }
            self.exec(
                "UPDATE inodes SET dead=?3 WHERE w=?1 AND ino=?2 AND dead=9223372036854775807",
                &[&w, &ino, &g],
            )?;
        }
        self.exec(
            "INSERT INTO inodes VALUES(?1,?2,?3,9223372036854775807,?4,?5)",
            &[&w, &ino, &g, &value, &size],
        )?;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.m = Metrics::default();
    }
}
pub fn memory(reset: bool) -> Result<(i64, i64)> {
    let (mut current, mut high) = (0_i64, 0_i64);
    let rc =
        unsafe { rusqlite::ffi::sqlite3_status64(0, &mut current, &mut high, i32::from(reset)) };
    if rc != 0 {
        return Err(format!("engine memory status: {rc}").into());
    }
    Ok((current, high))
}
