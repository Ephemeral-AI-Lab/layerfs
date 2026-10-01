use crate::engine::{Engine, Result, PAGE};

impl Engine {
    fn put_extent(
        &mut self,
        w: i64,
        start: i64,
        end: i64,
        source: i64,
        offset: i64,
        g: i64,
    ) -> Result<()> {
        if start < end {
            self.exec(
                "INSERT INTO extents VALUES(?1,1,?2,?3,9223372036854775807,?4,?5,?6)",
                &[&w, &start, &g, &end, &source, &offset],
            )?;
        }
        Ok(())
    }
    fn remove_extent(&mut self, w: i64, start: i64, born: i64, g: i64) -> Result<()> {
        if born == g {
            self.exec(
                "DELETE FROM extents WHERE w=?1 AND ino=1 AND start=?2 AND born=?3",
                &[&w, &start, &born],
            )?;
        } else {
            self.exec(
                "UPDATE extents SET dead=?4 WHERE w=?1 AND ino=1 AND start=?2 AND born=?3",
                &[&w, &start, &born, &g],
            )?;
        }
        Ok(())
    }
    pub fn overwrite(&mut self, w: i64, lo: i64, hi: i64, source: i64) -> Result<()> {
        self.begin()?;
        let g = self.scalar("SELECT active FROM workspace WHERE w=?1", &[&w])?;
        let size = self.scalar(
            "SELECT size FROM inodes WHERE w=?1 AND ino=1 AND dead=9223372036854775807",
            &[&w],
        )?;
        let pred=self.ints("SELECT start FROM extents WHERE w=?1 AND ino=1 AND dead=9223372036854775807 AND start<=?2 ORDER BY start DESC LIMIT 1",&[&w,&lo])?;
        let mut cursor = pred.first().map_or(lo - 1, |r| r[0] - 1);
        loop {
            let page=self.ints("SELECT start,end,source,source_offset,born FROM extents WHERE w=?1 AND ino=1 AND dead=9223372036854775807 AND start>?2 AND start<?3 ORDER BY start LIMIT 128",&[&w,&cursor,&hi])?;
            if page.is_empty() {
                break;
            }
            cursor = page.last().unwrap()[0];
            for r in &page {
                let (start, end, old_source, offset, born) = (r[0], r[1], r[2], r[3], r[4]);
                if end <= lo {
                    continue;
                }
                self.remove_extent(w, start, born, g)?;
                self.put_extent(w, start, lo.min(end), old_source, offset, g)?;
                self.put_extent(
                    w,
                    hi.max(start),
                    end,
                    old_source,
                    offset + hi.max(start) - start,
                    g,
                )?;
            }
            if page.len() < PAGE {
                break;
            }
        }
        if lo > size {
            self.put_extent(w, size, lo, -1, 0, g)?;
        }
        self.put_extent(w, lo, hi, source, 0, g)?;
        self.inode(w, 1, 0, size.max(hi))?;
        self.commit()
    }
    pub fn resize(&mut self, w: i64, size: i64) -> Result<()> {
        let old = self.scalar(
            "SELECT size FROM inodes WHERE w=?1 AND ino=1 AND dead=9223372036854775807",
            &[&w],
        )?;
        if size > old {
            return self.overwrite(w, old, size, -1);
        }
        self.begin()?;
        let g = self.scalar("SELECT active FROM workspace WHERE w=?1", &[&w])?;
        let pred=self.ints("SELECT start FROM extents WHERE w=?1 AND ino=1 AND dead=9223372036854775807 AND start<?2 ORDER BY start DESC LIMIT 1",&[&w,&size])?;
        let mut cursor = pred.first().map_or(-1, |r| r[0] - 1);
        loop {
            let page=self.ints("SELECT start,end,source,source_offset,born FROM extents WHERE w=?1 AND ino=1 AND dead=9223372036854775807 AND start>?2 ORDER BY start LIMIT 128",&[&w,&cursor])?;
            if page.is_empty() {
                break;
            }
            cursor = page.last().unwrap()[0];
            for r in &page {
                if r[1] <= size {
                    continue;
                }
                self.remove_extent(w, r[0], r[4], g)?;
                self.put_extent(w, r[0], size.min(r[1]), r[2], r[3], g)?;
            }
            if page.len() < PAGE {
                break;
            }
        }
        self.inode(w, 1, 0, size)?;
        self.commit()
    }
}
pub fn run(e: &mut Engine, case: &str) -> Result<()> {
    match case {
        "extent-repeated-4097" => {
            for i in 0..4097 {
                e.overwrite(1, 4096, 4128, i + 1)?;
            }
        }
        "extent-append-512" => {
            for i in 0..512 {
                let start = 16384 + i * 32;
                e.overwrite(1, start, start + 32, i + 1)?;
            }
        }
        "extent-dispersed-512" => {
            for i in 0..512 {
                let start = ((i * 61) % 1024) * 32;
                e.overwrite(1, start, start + 32, i + 1)?;
            }
        }
        "extent-fragmented-8192" => e.overwrite(1, 0, 65536, 1)?,
        "extent-truncate-regrow" => {
            e.resize(1, 4096)?;
            e.resize(1, 65536)?;
        }
        _ => return Err("unknown extent case".into()),
    }
    Ok(())
}
