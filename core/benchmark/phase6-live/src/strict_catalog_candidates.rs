//! Persisted signature ring rows, bounded pages and domain-qualified hydration.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct CandidateRow {
    pub slot: u16,
    pub stamp: u64,
    pub id: ObjectId,
    pub signature: [u8; 32],
}
impl StrictCatalog {
    pub fn stage_candidates(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        rows: &[CandidateRow],
    ) -> Result<(), String> {
        self.write()?;
        if scope.own_save != Some(save) || rows.len() > PAGE {
            return Err("candidate owner/page admission".into());
        }
        let mut db = self.db.lock().map_err(|_| "catalog owner")?;
        let tx = db.transaction().map_err(err)?;
        active(&tx, save)?;
        for row in rows {
            if row.stamp == 0
                || row.slot as u64 != (row.stamp - 1) % 8192
                || row.stamp > i64::MAX as u64
            {
                return Err("candidate slot/stamp admission".into());
            }
            if location(&tx, scope, domain, row.id)?.is_none() {
                return Err("candidate required placement missing".into());
            }
            tx.execute(
                "INSERT OR IGNORE INTO candidate_slots VALUES(?1,?2)",
                params![domain as i64, row.slot],
            )
            .map_err(err)?;
            tx.execute("INSERT INTO candidate_entries VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(domain,slot,save_id) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>candidate_entries.stamp",params![domain as i64,row.slot,row.stamp as i64,row.id.as_bytes().as_slice(),row.signature.as_slice(),save]).map_err(err)?;
        }
        tx.commit().map_err(err)
    }
    /// Selected latest eligible ring occupant per slot, oldest stamp first.
    pub fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>, String> {
        if limit == 0 || limit > PAGE || after > i64::MAX as u64 {
            return Err("candidate page admission".into());
        }
        let db = self.db.lock().map_err(|_| "catalog owner")?;
        let mut q=db.prepare_cached("SELECT c.slot,c.stamp,c.object_id,c.signature FROM candidate_slots k CROSS JOIN candidate_entries c INDEXED BY candidate_slot ON c.domain=k.domain AND c.slot=k.slot AND c.stamp=(SELECT cc.stamp FROM candidate_entries cc JOIN saves ss USING(save_id) WHERE cc.domain=k.domain AND cc.slot=k.slot AND ((ss.status IN(0,1) AND cc.save_id=?3) OR(ss.status=2 AND ss.publication<=?4)) ORDER BY cc.stamp DESC LIMIT 1) WHERE k.domain=?1 AND c.stamp>?2 ORDER BY c.stamp LIMIT ?5").map_err(err)?;
        let mut rows = q
            .query(params![
                domain as i64,
                after as i64,
                scope.own_save,
                eligible(scope)?,
                limit as i64
            ])
            .map_err(err)?;
        let mut out = Vec::with_capacity(limit);
        while let Some(row) = rows.next().map_err(err)? {
            let bytes: Vec<u8> = row.get(2).map_err(err)?;
            let sig: Vec<u8> = row.get(3).map_err(err)?;
            let stamp: i64 = row.get(1).map_err(err)?;
            out.push(CandidateRow {
                slot: row.get(0).map_err(err)?,
                stamp: u64::try_from(stamp).map_err(|_| "candidate stamp")?,
                id: ObjectId::from_bytes(&bytes).map_err(|e| e.to_string())?,
                signature: sig.try_into().map_err(|_| "candidate signature width")?,
            });
        }
        Ok(out)
    }
}
