use crate::engine::{Engine, Result};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
};

impl Engine {
    pub fn capture(&mut self, w: i64) -> Result<Option<i64>> {
        self.begin()?;
        let g = self.scalar("SELECT active FROM workspace WHERE w=?1", &[&w])?;
        let pending = self.scalar("SELECT COUNT(*) FROM captures WHERE w=?1", &[&w])?;
        if pending != 0 {
            self.rollback()?;
            return Ok(None);
        }
        self.exec("INSERT INTO captures VALUES(?1,?2)", &[&w, &g])?;
        self.exec(
            "UPDATE workspace SET active=active+1,pending=?2 WHERE w=?1",
            &[&w, &g],
        )?;
        self.commit()?;
        Ok(Some(g))
    }
    pub fn release(&mut self, w: i64) -> Result<()> {
        self.begin()?;
        if self.exec("DELETE FROM captures WHERE w=?1", &[&w])? != 1 {
            return Err("missing capture".into());
        }
        self.exec("UPDATE workspace SET pending=NULL WHERE w=?1", &[&w])?;
        self.commit()
    }
    pub fn retire(&mut self, w: i64) -> Result<()> {
        let pending = self.scalar("SELECT COUNT(*) FROM captures WHERE w=?1", &[&w])?;
        if pending != 0 {
            return Err("retirement with pending capture".into());
        }
        let g = self.scalar("SELECT active FROM workspace WHERE w=?1", &[&w])?;
        loop {
            let page=self.ints("SELECT ino,born FROM inodes WHERE w=?1 AND dead<=?2 ORDER BY dead,ino,born LIMIT 128",&[&w,&g])?;
            if page.is_empty() {
                break;
            }
            self.begin()?;
            for r in page {
                self.exec(
                    "DELETE FROM inodes WHERE w=?1 AND ino=?2 AND born=?3",
                    &[&w, &r[0], &r[1]],
                )?;
            }
            self.commit()?;
        }
        Ok(())
    }
    pub fn selected(&mut self, w: i64, g: i64, after: i64) -> Result<Vec<Vec<i64>>> {
        self.ints("SELECT ino,value FROM inodes WHERE w=?1 AND ino>?2 AND born<=?3 AND dead>?3 ORDER BY ino LIMIT 256",&[&w,&after,&g])
    }
}
fn emit(out: &mut BufWriter<File>, w: i64, g: i64, rows: &[Vec<i64>]) -> Result<()> {
    for r in rows {
        writeln!(out, "{w}\t{g}\t{}\t{}", r[0], r[1])?;
    }
    Ok(())
}
fn updates(e: &mut Engine, w: i64, round: i64) -> Result<()> {
    for i in 1..=128 {
        e.begin()?;
        e.inode(w, i, round * 100000 + i, 0)?;
        e.commit()?;
    }
    Ok(())
}
pub fn generations(e: &mut Engine, path: &Path) -> Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    for round in 1..=8 {
        let g = e.capture(1)?.ok_or("capture refusal")?;
        updates(e, 1, round)?;
        let mut after = 0;
        loop {
            let rows = e.selected(1, g, after)?;
            if rows.is_empty() {
                break;
            }
            after = rows.last().unwrap()[0];
            emit(&mut out, 1, g, &rows)?;
        }
        e.release(1)?;
        e.retire(1)?;
    }
    out.flush()?;
    Ok(())
}
pub fn overlap(e: Engine, count: i64, path: &Path) -> Result<(Engine, usize)> {
    let shared = Arc::new(Mutex::new(e));
    let mut out = BufWriter::new(File::create(path)?);
    let mut jobs = Vec::new();
    let mut first = Vec::new();
    for w in 1..=count {
        let mut e = shared.lock().map_err(|_| "poisoned engine")?;
        let g = e.capture(w)?.ok_or("capture refusal")?;
        if e.capture(w)?.is_some() {
            return Err("duplicate capture accepted".into());
        }
        let page = e.selected(w, g, 0)?;
        first.push((w, g, page));
        drop(e);
        let (start_tx, start_rx) = mpsc::sync_channel::<()>(0);
        let (done_tx, done_rx) = mpsc::sync_channel::<()>(0);
        let owner = Arc::clone(&shared);
        let job = thread::spawn(move || -> Result<()> {
            start_rx.recv()?;
            for i in 1..=128 {
                let mut e = owner.lock().map_err(|_| "poisoned engine")?;
                e.begin()?;
                e.inode(w, i, 100000 + i, 0)?;
                e.commit()?;
            }
            done_tx.send(())?;
            Ok(())
        });
        jobs.push((start_tx, done_rx, job));
    }
    // Every first page was read under a real selected generation, and every
    // writer completes while those captures remain owned, before traversal resumes.
    for (tx, _, _) in &jobs {
        tx.send(())?;
    }
    for (_, rx, _) in &jobs {
        rx.recv()?;
    }
    let mut completed = 0;
    for (_, _, job) in jobs {
        job.join().map_err(|_| "mutator panicked")??;
        completed += 1;
    }
    for (w, g, page) in first {
        emit(&mut out, w, g, &page)?;
        let mut after = page.last().unwrap()[0];
        loop {
            let rows = shared
                .lock()
                .map_err(|_| "poisoned engine")?
                .selected(w, g, after)?;
            if rows.is_empty() {
                break;
            }
            after = rows.last().unwrap()[0];
            emit(&mut out, w, g, &rows)?;
        }
        let mut e = shared.lock().map_err(|_| "poisoned engine")?;
        e.release(w)?;
        e.retire(w)?;
    }
    out.flush()?;
    let e = Arc::try_unwrap(shared)
        .map_err(|_| "engine owner leaked")?
        .into_inner()
        .map_err(|_| "poisoned engine")?;
    Ok((e, completed))
}
