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
        self.with_transaction(|tx| {
        if read_state(tx)?.active {
            return Err("candidate snapshot busy: staging during hydrate".into());
        }
        active(tx, save)?;
        for row in rows {
            if row.stamp == 0
                || row.slot as u64 != (row.stamp - 1) % 8192
                || row.stamp > i64::MAX as u64
            {
                return Err("candidate slot/stamp admission".into());
            }
            if location(tx, scope, domain, row.id)?.is_none() {
                return Err("candidate required placement missing".into());
            }
            tx.execute(
                "INSERT OR IGNORE INTO candidate_slots VALUES(?1,?2)",
                params![domain as i64, row.slot],
            )
            .map_err(err)?;
            tx.execute("INSERT INTO candidate_entries VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(domain,slot,save_id) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>candidate_entries.stamp",params![domain as i64,row.slot,row.stamp as i64,row.id.as_bytes().as_slice(),row.signature.as_slice(),save]).map_err(err)?;
        }
        Ok(())
        })
    }

    /// Hydrates a frozen selected ring once; subsequent pages use the stamp index.
    /// One engine has one snapshot lease. A different/interleaved walk is refused.
    pub fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>, String> {
        if !self.writable {
            return Err("readonly candidate snapshot unsupported".into());
        }
        if limit == 0 || limit > PAGE || after > i64::MAX as u64 {
            return Err("candidate page admission".into());
        }
        self.with_transaction(|tx| {
        let db: &Connection = tx;
        let state = read_state(db)?;
        let same = state.matches(self.cache_namespace(), scope, domain)?;
        if after == 0 {
            if state.active {
                return Err("candidate snapshot busy: overlapping walk".into());
            }
            build_snapshot(db, self.cache_namespace(), scope, domain)?;
        } else {
            if !same || state.expected_after != after {
                return Err("candidate snapshot busy: scope/domain/cursor mismatch".into());
            }
            if !state.active {
                if state.completed {
                    db.execute("UPDATE temp.candidate_snapshot_state SET completed=0,pages=pages+1 WHERE singleton=1",[]).map_err(err)?;
                    return Ok(Ok(Vec::new()));
                }
                return Err("candidate snapshot unavailable".into());
            }
            if owner_eligible(db, scope.own_save)? != state.owner_eligible {
                release_snapshot(db, false)?;
                return Ok(Err("candidate snapshot owner eligibility changed".into()));
            }
        }
        let mut q=db.prepare_cached("SELECT slot,stamp,object_id,signature FROM temp.candidate_snapshot WHERE stamp>?1 ORDER BY stamp LIMIT ?2").map_err(err)?;
        q.reset_status(rusqlite::StatementStatus::VmStep);
        q.reset_status(rusqlite::StatementStatus::FullscanStep);
        q.reset_status(rusqlite::StatementStatus::Sort);
        let out = {
            let mut rows = q.query(params![after as i64, limit as i64]).map_err(err)?;
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
            out
        };
        let vm = q.get_status(rusqlite::StatementStatus::VmStep);
        let scan = q.get_status(rusqlite::StatementStatus::FullscanStep);
        let sorts = q.get_status(rusqlite::StatementStatus::Sort);
        drop(q);
        let next = out.last().map_or(after, |r| r.stamp);
        db.execute("UPDATE temp.candidate_snapshot_state SET expected_after=?1,pages=pages+1,vm_steps=vm_steps+?2,page_vm_steps=page_vm_steps+?2,page_fullscan_steps=page_fullscan_steps+?3,page_sorts=page_sorts+?4 WHERE singleton=1",params![next as i64,vm,scan,sorts]).map_err(err)?;
        if out.len() < limit {
            release_snapshot(db, !out.is_empty())?;
        }
        Ok(Ok(out))
        })?
    }

    /// Actual bounded candidate hydration work, useful for storage diagnostics.
    pub fn candidate_snapshot_work(&self) -> Result<CandidateSnapshotWork, String> {
        if !self.writable {
            return Err("readonly candidate snapshot unsupported".into());
        }
        let db = self.connection()?;
        let pages: i64 = db
            .pragma_query_value(Some("temp"), "page_count", |r| r.get(0))
            .map_err(err)?;
        let page_bytes: i64 = db
            .pragma_query_value(Some("temp"), "page_size", |r| r.get(0))
            .map_err(err)?;
        let raw: [i64;7] = db.query_row("SELECT builds,slot_lookups,pages,vm_steps,page_vm_steps,page_fullscan_steps,page_sorts FROM temp.candidate_snapshot_state WHERE singleton=1",[],|r|Ok([r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?])).map_err(err)?;
        Ok(CandidateSnapshotWork {
            builds: unsigned(raw[0])?,
            slot_lookups: unsigned(raw[1])?,
            pages: unsigned(raw[2])?,
            vm_steps: unsigned(raw[3])?,
            page_vm_steps: unsigned(raw[4])?,
            page_fullscan_steps: unsigned(raw[5])?,
            page_sorts: unsigned(raw[6])?,
            temp_pages: unsigned(pages)?,
            temp_page_bytes: unsigned(page_bytes)?,
        })
    }
}

/// Fixed-size observations of actual SQLite statements, never a speed verdict.
#[derive(Clone, Copy, Debug, Default)]
pub struct CandidateSnapshotWork {
    pub builds: u64,
    pub slot_lookups: u64,
    pub pages: u64,
    pub vm_steps: u64,
    pub page_vm_steps: u64,
    pub page_fullscan_steps: u64,
    pub page_sorts: u64,
    pub temp_pages: u64,
    pub temp_page_bytes: u64,
}

pub(super) fn initialize_snapshot(db: &Connection) -> Result<(), String> {
    db.execute_batch(include_str!("strict_candidate_snapshot.sql"))
        .map_err(err)
}

struct SnapshotState {
    active: bool,
    completed: bool,
    engine: Option<Vec<u8>>,
    publication: Option<i64>,
    ceiling: Option<i64>,
    own_save: Option<i64>,
    domain: Option<i64>,
    expected_after: u64,
    owner_eligible: bool,
}
impl SnapshotState {
    fn matches(
        &self,
        engine: u64,
        scope: CatalogScope,
        domain: PlacementDomain,
    ) -> Result<bool, String> {
        Ok(
            self.engine.as_deref() == Some(engine.to_be_bytes().as_slice())
                && self.publication == Some(eligible(scope)?)
                && self.ceiling == Some(scope.ceiling)
                && self.own_save == scope.own_save
                && self.domain == Some(domain as i64),
        )
    }
}
fn read_state(db: &Connection) -> Result<SnapshotState, String> {
    let (mut state, after): (SnapshotState,i64) = db.query_row("SELECT active,completed,engine,publication,ceiling,own_save,domain,expected_after,owner_eligible FROM temp.candidate_snapshot_state WHERE singleton=1",[],|r|Ok((SnapshotState {active:r.get(0)?,completed:r.get(1)?,engine:r.get(2)?,publication:r.get(3)?,ceiling:r.get(4)?,own_save:r.get(5)?,domain:r.get(6)?,expected_after:0,owner_eligible:r.get(8)?},r.get(7)?))).map_err(err)?;
    state.expected_after = unsigned(after)?;
    Ok(state)
}
fn owner_eligible(db: &Connection, own: Option<i64>) -> Result<bool, String> {
    match own {
        None => Ok(false),
        Some(save) => db
            .query_row(
                "SELECT status IN(0,1) FROM saves WHERE save_id=?1",
                [save],
                |r| r.get(0),
            )
            .map_err(err),
    }
}
fn release_snapshot(db: &Connection, completed: bool) -> Result<(), String> {
    db.execute("DELETE FROM temp.candidate_snapshot", [])
        .map_err(err)?;
    db.execute(
        "UPDATE temp.candidate_snapshot_state SET active=0,completed=?1 WHERE singleton=1",
        [completed],
    )
    .map_err(err)?;
    Ok(())
}
fn build_snapshot(
    db: &Connection,
    engine: u64,
    scope: CatalogScope,
    domain: PlacementDomain,
) -> Result<(), String> {
    let owner = owner_eligible(db, scope.own_save)?;
    db.execute("DELETE FROM temp.candidate_snapshot", [])
        .map_err(err)?;
    let slots: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM candidate_slots WHERE domain=?1",
            [domain as i64],
            |r| r.get(0),
        )
        .map_err(err)?;
    if slots > 8192 {
        return Err("candidate slot population bound".into());
    }
    let mut q=db.prepare_cached("INSERT INTO temp.candidate_snapshot(stamp,slot,object_id,signature) SELECT c.stamp,c.slot,c.object_id,c.signature FROM candidate_slots k CROSS JOIN candidate_entries c INDEXED BY candidate_slot ON c.domain=k.domain AND c.slot=k.slot AND c.save_id=(SELECT cc.save_id FROM candidate_entries cc JOIN saves ss USING(save_id) WHERE cc.domain=k.domain AND cc.slot=k.slot AND ((ss.status IN(0,1) AND cc.save_id=?2) OR(ss.status=2 AND ss.publication<=?3)) ORDER BY cc.stamp DESC LIMIT 1) WHERE k.domain=?1").map_err(err)?;
    q.reset_status(rusqlite::StatementStatus::VmStep);
    q.execute(params![domain as i64, scope.own_save, eligible(scope)?])
        .map_err(err)?;
    let vm = q.get_status(rusqlite::StatementStatus::VmStep);
    drop(q);
    db.execute("UPDATE temp.candidate_snapshot_state SET active=1,completed=0,engine=?1,publication=?2,ceiling=?3,own_save=?4,domain=?5,owner_eligible=?6,expected_after=0,builds=builds+1,slot_lookups=slot_lookups+?7,vm_steps=vm_steps+?8 WHERE singleton=1",params![engine.to_be_bytes().as_slice(),eligible(scope)?,scope.ceiling,scope.own_save,domain as i64,owner,slots,vm]).map_err(err)?;
    Ok(())
}

fn unsigned(value: i64) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| "candidate snapshot negative SQL scalar".into())
}
