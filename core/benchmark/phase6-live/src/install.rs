//! Known captured-row adoption, bounded SQL journals and explicit failure custody.
use crate::{
    engine::Engine,
    sql_windows::{self, Table},
};
use rusqlite::{params, OptionalExtension};
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub inodes: u64,
    pub extent_batches: u64,
    pub extents: u64,
    pub name_batches: u64,
    pub names: u64,
    pub prepared_rows: u64,
    pub edit_batches: u64,
    pub peak_rows: usize,
}
impl Engine {
    pub fn install_prepared(&mut self) -> Result<(), String> {
        self.source_ready()?;
        let result = self.install_captured_rows();
        if result.is_err() {
            self.installation_failed = true;
        }
        result
    }
    fn install_captured_rows(&mut self) -> Result<(), String> {
        let mut work = self.install_work.get();
        let mut after = 0;
        loop {
            let row: Option<(i64, u8, Option<Vec<u8>>)> = self
                .db
                .prepare_cached(
                    "SELECT id,kind,content FROM prepared WHERE id>?1 ORDER BY id LIMIT 1",
                )
                .map_err(|e| e.to_string())?
                .query_row([after], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()
                .map_err(|e| e.to_string())?;
            let Some((id, kind, root)) = row else { break };
            after = id;
            let tx = self.db.transaction().map_err(|e| e.to_string())?;
            let count=match kind {
                1=>{let root=root.ok_or("captured file root")?;if root.len()!=32{return Err("captured root width".into());}tx.execute("UPDATE inodes SET root=?2,base_size=size,base_visible=size,dirty=0,is_new=0,published=1 WHERE id=?1",params![id,root])},
                2=>tx.execute("UPDATE inodes SET dirty=0,is_new=0,published=1 WHERE id=?1",[id]),
                0=>tx.execute("UPDATE inodes SET dirty=0,is_new=0,published=0 WHERE id=?1",[id]),
                _=>return Err("captured inode kind".into()),
            }.map_err(|e|e.to_string())?;
            if count != 1 {
                return Err("captured inode identity".into());
            }
            tx.commit().map_err(|e| e.to_string())?;
            if kind == 1 {
                loop {
                    let tx = self.db.transaction().map_err(|e| e.to_string())?;
                    let n=tx.execute("DELETE FROM extents WHERE ino=?1 AND start IN (SELECT start FROM extents WHERE ino=?1 ORDER BY start LIMIT 64)",[id]).map_err(|e|e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    if n > sql_windows::ROWS {
                        return Err("captured extent window".into());
                    }
                    work.extent_batches += 1;
                    work.extents += n as u64;
                    work.peak_rows = work.peak_rows.max(n);
                    self.retire_sources()?;
                }
            }
            if kind == 0 || kind == 2 {
                loop {
                    let n=self.db.prepare_cached("DELETE FROM changed_names WHERE parent=?1 AND name IN (SELECT name FROM changed_names WHERE parent=?1 ORDER BY name LIMIT 64)").map_err(|e|e.to_string())?.execute([id]).map_err(|e|e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    if n > sql_windows::ROWS {
                        return Err("captured name window".into());
                    }
                    work.name_batches += 1;
                    work.names += n as u64;
                    work.peak_rows = work.peak_rows.max(n);
                }
            }
            if self
                .db
                .execute("DELETE FROM prepared WHERE id=?1", [id])
                .map_err(|e| e.to_string())?
                != 1
            {
                return Err("captured prepared identity".into());
            }
            work.inodes += 1;
            work.prepared_rows += 1;
            self.install_work.set(work);
        }
        work.edit_batches += sql_windows::clear(&self.db, Table::Edits)?;
        self.drain_sources()?;
        self.install_work.set(work);
        Ok(())
    }
}
