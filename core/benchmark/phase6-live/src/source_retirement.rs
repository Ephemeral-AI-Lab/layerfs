//! Actual local extent references and bounded zero-reference source retirement.
use crate::engine::Engine;
use rusqlite::params;
pub const BATCH: usize = 64;
pub const SCHEMA:&str="
CREATE TABLE source_retirement(source INTEGER PRIMARY KEY REFERENCES sources(id));
CREATE TRIGGER extent_source_insert AFTER INSERT ON extents BEGIN
 UPDATE sources SET refs=refs+1 WHERE id=NEW.source;
 DELETE FROM source_retirement WHERE source=NEW.source;
END;
CREATE TRIGGER extent_source_delete AFTER DELETE ON extents BEGIN
 UPDATE sources SET refs=refs-1 WHERE id=OLD.source;
 INSERT OR IGNORE INTO source_retirement SELECT id FROM sources WHERE id=OLD.source AND refs=0;
END;
CREATE TRIGGER extent_source_immutable BEFORE UPDATE OF source ON extents WHEN NEW.source<>OLD.source BEGIN
 SELECT RAISE(ABORT,'immutable extent source');
END;
";
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub batches: u64,
    pub retired: u64,
    pub peak_rows: usize,
}
impl Engine {
    pub fn source_ready(&self) -> Result<(), String> {
        if self.retirement_failed || self.installation_failed {
            return Err("source retirement owner quarantined".into());
        }
        Ok(())
    }
    pub fn retirement_pending(&self) -> Result<bool, String> {
        self.db
            .query_row("SELECT EXISTS(SELECT 1 FROM source_retirement)", [], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())
    }
    pub fn retire_sources(&mut self) -> Result<usize, String> {
        self.source_ready()?;
        let result = self.retire_source_batch();
        if result.is_err() {
            self.retirement_failed = true;
        }
        result
    }
    fn retire_source_batch(&mut self) -> Result<usize, String> {
        let mut q=self.db.prepare_cached("SELECT r.source,s.refs FROM source_retirement r JOIN sources s ON s.id=r.source ORDER BY r.source LIMIT 64").map_err(|e|e.to_string())?;
        let mut rows = q.query([]).map_err(|e| e.to_string())?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(BATCH)
            .map_err(|_| "source retirement reservation")?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let id: i64 = row.get(0).map_err(|e| e.to_string())?;
            let refs: i64 = row.get(1).map_err(|e| e.to_string())?;
            if refs != 0 || id <= 0 {
                return Err("source retirement ownership/reference invariant".into());
            }
            ids.push(id);
        }
        drop(rows);
        drop(q);
        let count = ids.len();
        if count == 0 {
            return Ok(0);
        }
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        for id in ids {
            // The engine mutation owner excludes readers/new references throughout.
            std::fs::remove_file(self.sources.join(id.to_string()))
                .map_err(|e| format!("source retirement file: {e}"))?;
            if tx
                .execute("DELETE FROM source_retirement WHERE source=?1", [id])
                .map_err(|e| e.to_string())?
                != 1
                || tx
                    .execute("DELETE FROM sources WHERE id=?1 AND refs=0", params![id])
                    .map_err(|e| e.to_string())?
                    != 1
            {
                return Err("source retirement SQL identity".into());
            }
        }
        tx.commit()
            .map_err(|e| format!("source retirement SQL outcome: {e}"))?;
        let mut w = self.retirement_work.get();
        w.batches += 1;
        w.retired += count as u64;
        w.peak_rows = w.peak_rows.max(count);
        self.retirement_work.set(w);
        Ok(count)
    }
    pub fn drain_sources(&mut self) -> Result<(), String> {
        self.source_ready()?;
        while self.retirement_pending()? {
            self.retire_sources()?;
        }
        Ok(())
    }
}
