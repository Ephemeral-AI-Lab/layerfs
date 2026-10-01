use crate::engine::{Engine, Result};
use rusqlite::StatementStatus;
use std::fmt::Write as FmtWrite;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

impl Engine {
    fn selected_names(&mut self, parent: i64, g: i64, after: &[u8]) -> Result<Vec<(Vec<u8>, i64)>> {
        let sql="SELECT name,ino FROM names WHERE w=1 AND parent=?1 AND name>?2 AND born<=?3 AND dead>?3 ORDER BY name LIMIT 60";
        let t = Instant::now();
        let mut s = self.db.prepare_cached(sql)?;
        for stat in [
            StatementStatus::VmStep,
            StatementStatus::FullscanStep,
            StatementStatus::Sort,
        ] {
            s.reset_status(stat);
        }
        let mut rows = s.query(rusqlite::params![parent, after, g])?;
        let mut result = Vec::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let name: Vec<u8> = row.get(0)?;
            if name.len() > 255 {
                return Err("name profile exceeded".into());
            }
            bytes += name.len() + 8;
            if bytes > 16384 {
                return Err("name cursor bytes exceeded".into());
            }
            result.push((name, row.get::<_, i64>(1)?));
        }
        drop(rows);
        self.m.sql_calls += 1;
        self.m.sql_ns += t.elapsed().as_nanos();
        self.m.sql_texts.insert(sql.to_owned());
        self.m.vm_steps += s.get_status(StatementStatus::VmStep) as u64;
        self.m.fullscan_steps += s.get_status(StatementStatus::FullscanStep) as u64;
        self.m.sorts += s.get_status(StatementStatus::Sort) as u64;
        self.m.page_rows = self.m.page_rows.max(result.len());
        self.m.page_fields_bytes = self.m.page_fields_bytes.max(bytes);
        Ok(result)
    }
    pub fn selected_extents(&mut self, g: i64, after: i64) -> Result<Vec<Vec<i64>>> {
        self.ints("SELECT start,end,source,source_offset,born,dead FROM extents WHERE w=1 AND ino=1 AND start>?1 AND born<=?2 AND dead>?2 ORDER BY start LIMIT 128",&[&after,&g])
    }
    fn emit_inode(&mut self, g: i64, ino: i64, out: &mut BufWriter<File>) -> Result<u64> {
        let value = self.ints(
            "SELECT value,size FROM inodes WHERE w=1 AND ino=?1 AND born<=?2 AND dead>?2",
            &[&ino, &g],
        )?;
        if value.len() != 1 {
            return Err("selected inode cardinality".into());
        }
        let kind = self.scalar("SELECT kind FROM kinds WHERE ino=?1", &[&ino])?;
        writeln!(
            out,
            "I\t{g}\t{ino}\t{}\t{}\t{kind}",
            value[0][0], value[0][1]
        )?;
        let mut count = 1;
        if kind == 1 {
            let mut after = Vec::new();
            loop {
                let page = self.selected_names(ino, g, &after)?;
                if page.is_empty() {
                    break;
                }
                after = page.last().unwrap().0.clone();
                for (name, child) in page {
                    let mut encoded = String::with_capacity(name.len() * 2);
                    for byte in name {
                        write!(&mut encoded, "{byte:02x}")?;
                    }
                    writeln!(out, "N\t{g}\t{ino}\t{child}\t{encoded}")?;
                    count += 1;
                }
            }
        } else if ino == 1 {
            let mut after = -1;
            loop {
                let page = self.selected_extents(g, after)?;
                if page.is_empty() {
                    break;
                }
                after = page.last().unwrap()[0];
                for r in page {
                    writeln!(out, "E\t{g}\t{ino}\t{}\t{}\t{}\t{}", r[0], r[1], r[2], r[3])?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }
    pub fn cohort_prepare(&mut self, path: &Path) -> Result<(i64, u64, u64)> {
        let g = self.capture(1)?.ok_or("submission already pending")?;
        let mut out = BufWriter::new(File::create(path)?);
        let (mut after, mut inodes, mut rows) = (0, 0, 0);
        loop {
            let page = self.ints(
                "SELECT ino FROM changed WHERE w=1 AND g=?1 AND ino>?2 ORDER BY ino LIMIT 128",
                &[&g, &after],
            )?;
            if page.is_empty() {
                break;
            }
            after = page.last().unwrap()[0];
            for r in page {
                rows += self.emit_inode(g, r[0], &mut out)?;
                inodes += 1;
            }
        }
        out.flush()?;
        Ok((g, inodes, rows))
    }
    pub fn cohort_retire(&mut self) -> Result<()> {
        let g = self.scalar(
            "SELECT COALESCE(MIN(g),(SELECT active FROM workspace WHERE w=1)) FROM pins WHERE w=1",
            &[],
        )?;
        for table in ["inodes", "names", "extents"] {
            loop {
                let sql=match table {
                    "inodes"=>"SELECT ino,born FROM inodes WHERE w=1 AND dead<=?1 ORDER BY dead,ino,born LIMIT 128",
                    "extents"=>"SELECT ino,start,born FROM extents WHERE w=1 AND dead<=?1 ORDER BY dead,ino,start,born LIMIT 128",
                    "names"=>{
                        self.begin()?;
                        let removed=self.exec("DELETE FROM names WHERE (w,parent,name,born) IN (SELECT w,parent,name,born FROM names WHERE w=1 AND dead<=?1 ORDER BY dead,parent,name,born LIMIT 128)",&[&g])?;
                        self.commit()?;
                        if removed==0 {break;}
                        continue;
                    },
                    _=>unreachable!(),
                };
                let rows = self.ints(sql, &[&g])?;
                if rows.is_empty() {
                    break;
                }
                self.begin()?;
                for r in rows {
                    if table == "inodes" {
                        self.exec(
                            "DELETE FROM inodes WHERE w=1 AND ino=?1 AND born=?2",
                            &[&r[0], &r[1]],
                        )?;
                    } else {
                        self.exec(
                            "DELETE FROM extents WHERE w=1 AND ino=?1 AND start=?2 AND born=?3",
                            &[&r[0], &r[1], &r[2]],
                        )?;
                    }
                }
                self.commit()?;
            }
        }
        Ok(())
    }
    pub fn cohort_complete(&mut self, g: i64) -> Result<()> {
        loop {
            let rows = self.ints(
                "SELECT ino FROM changed WHERE w=1 AND g=?1 ORDER BY ino LIMIT 128",
                &[&g],
            )?;
            if rows.is_empty() {
                break;
            }
            self.begin()?;
            for r in rows {
                self.exec(
                    "DELETE FROM changed WHERE w=1 AND g=?1 AND ino=?2",
                    &[&g, &r[0]],
                )?;
            }
            self.commit()?;
        }
        self.release(1)?;
        self.cohort_retire()
    }
    pub fn cohort_pin_result(&mut self, g: i64, path: &Path) -> Result<u64> {
        let mut out = BufWriter::new(File::create(path)?);
        let rows = self.emit_inode(g, 1, &mut out)?;
        out.flush()?;
        Ok(rows)
    }
}
